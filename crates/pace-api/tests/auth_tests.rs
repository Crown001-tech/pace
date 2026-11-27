use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use pace_api::{
    app::{create_app, AppState},
    config::AppConfig,
    error::ProblemDetails,
};
use pace_db::{create_pool, create_user, run_migrations, CreateUser};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> (axum::Router, pace_db::DbPool, AppConfig) {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pace:pace@localhost:5432/pace".to_string());

    let pool = create_pool(&database_url)
        .await
        .expect("Failed to connect to test database");

    run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    let config = AppConfig {
        pace_env: "test".to_string(),
        host: "127.0.0.1".to_string(),
        port: 8080,
        database_url,
        session_secret: "01234567890123456789012345678901".to_string(),
        cors_origin: "http://localhost:3000".to_string(),
        s3_endpoint: "http://localhost:9000".to_string(),
        s3_bucket: "pace".to_string(),
        s3_region: "us-east-1".to_string(),
        s3_access_key: "pace".to_string(),
        s3_secret_key: "pace-secret".to_string(),
        pexels_api_key: None,
        pixabay_api_key: None,
        cloudflare_account_id: None,
        cloudflare_api_token: None,
    };

    let state = AppState::new(pool.clone(), config.clone());

    let app = create_app(state);
    (app, pool, config)
}

#[tokio::test]
async fn test_login_success_and_logout_terminates_session() {
    let (app, pool, _) = setup_test_app().await;

    // 1. Create a user with a hashed password
    let email = format!("test_login_{}@example.com", uuid::Uuid::now_v7());
    let password = "CorrectPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    let user = create_user(
        &pool,
        CreateUser {
            email: email.clone(),
            name: "Test Creator".to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    // 2. Perform POST /api/v1/auth/login with correct password
    let login_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": email,
                "password": password
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify Set-Cookie header is present
    let cookie_header = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("Missing Set-Cookie header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie_header.contains("pace_session="));
    assert!(cookie_header.contains("HttpOnly"));

    // Extract cookie value for subsequent requests
    let session_cookie = cookie_header
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let login_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(login_json["user"]["email"], email);
    let csrf_token = login_json["csrf_token"]
        .as_str()
        .expect("Missing csrf_token")
        .to_string();

    // 3. GET /api/v1/auth/me using session cookie
    let me_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, &session_cookie)
        .body(Body::empty())
        .unwrap();

    let me_resp = app.clone().oneshot(me_req).await.unwrap();
    assert_eq!(me_resp.status(), StatusCode::OK);
    let me_body = me_resp.into_body().collect().await.unwrap().to_bytes();
    let me_json: Value = serde_json::from_slice(&me_body).unwrap();
    assert_eq!(me_json["email"], email);
    assert_eq!(me_json["id"], user.id.to_string());

    // 4. POST /api/v1/auth/logout with session cookie + CSRF token
    let logout_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/logout")
        .header(header::COOKIE, &session_cookie)
        .header("X-PACE-CSRF", &csrf_token)
        .body(Body::empty())
        .unwrap();

    let logout_resp = app.clone().oneshot(logout_req).await.unwrap();
    assert_eq!(logout_resp.status(), StatusCode::OK);

    // 5. Verify session is dead on server
    let after_logout_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, &session_cookie)
        .body(Body::empty())
        .unwrap();

    let after_resp = app.oneshot(after_logout_req).await.unwrap();
    assert_eq!(after_resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_ten_failed_login_attempts_locks_account_for_fifteen_minutes() {
    let (app, pool, _) = setup_test_app().await;

    let email = format!("test_lockout_{}@example.com", uuid::Uuid::now_v7());
    let password = "LegitPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    create_user(
        &pool,
        CreateUser {
            email: email.clone(),
            name: "Lockout Test User".to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    // Perform 9 failed login attempts
    for _ in 1..=9 {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "email": email,
                    "password": "WrongPassword!"
                })
                .to_string(),
            ))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // 10th failed attempt triggers lockout
    let tenth_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": email,
                "password": "WrongPassword!"
            })
            .to_string(),
        ))
        .unwrap();

    let tenth_resp = app.clone().oneshot(tenth_req).await.unwrap();
    assert_eq!(tenth_resp.status(), StatusCode::UNAUTHORIZED);

    // 11th attempt (even with CORRECT password) is locked out!
    let locked_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": email,
                "password": password
            })
            .to_string(),
        ))
        .unwrap();

    let locked_resp = app.oneshot(locked_req).await.unwrap();
    assert_eq!(locked_resp.status(), StatusCode::TOO_MANY_REQUESTS);

    let body = locked_resp.into_body().collect().await.unwrap().to_bytes();
    let problem: ProblemDetails = serde_json::from_slice(&body).unwrap();
    assert_eq!(problem.code, "ACCOUNT_LOCKED");
}

#[tokio::test]
async fn test_temporary_password_forces_password_change() {
    let (app, pool, _) = setup_test_app().await;

    let email = format!("test_temp_{}@example.com", uuid::Uuid::now_v7());
    let temp_password = "TemporaryPass123!";
    let password_hash = pace_api::auth::passwords::hash_password(temp_password).unwrap();

    // Create user with must_change_password = true
    create_user(
        &pool,
        CreateUser {
            email: email.clone(),
            name: "Temp Pass User".to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: true, // MUST CHANGE PASSWORD
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    // 1. Sign in with temporary password
    let login_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": email,
                "password": temp_password
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["user"]["must_change_password"], true);
    let csrf_token = json["csrf_token"].as_str().unwrap().to_string();

    // 2. Try to access /api/v1/auth/me -> MUST be blocked with 403 PASSWORD_CHANGE_REQUIRED
    let me_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();

    let me_resp = app.clone().oneshot(me_req).await.unwrap();
    assert_eq!(me_resp.status(), StatusCode::FORBIDDEN);
    let me_body = me_resp.into_body().collect().await.unwrap().to_bytes();
    let problem: ProblemDetails = serde_json::from_slice(&me_body).unwrap();
    assert_eq!(problem.code, "PASSWORD_CHANGE_REQUIRED");

    // 3. Change password via POST /api/v1/auth/password
    let new_password = "BrandNewSecurePassword456!";
    let change_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/password")
        .header(header::COOKIE, &cookie)
        .header("X-PACE-CSRF", &csrf_token)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "current_password": temp_password,
                "new_password": new_password
            })
            .to_string(),
        ))
        .unwrap();

    let change_resp = app.clone().oneshot(change_req).await.unwrap();
    assert_eq!(change_resp.status(), StatusCode::OK);

    // 4. Now /api/v1/auth/me should succeed!
    let me_after_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();

    let me_after_resp = app.oneshot(me_after_req).await.unwrap();
    assert_eq!(me_after_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_deactivated_user_sessions_are_immediately_terminated() {
    let (app, pool, _) = setup_test_app().await;

    // 1. Create admin and regular user
    let admin_email = format!("admin_{}@example.com", uuid::Uuid::now_v7());
    let admin_pass = "AdminPass123!";
    let admin_hash = pace_api::auth::passwords::hash_password(admin_pass).unwrap();

    create_user(
        &pool,
        CreateUser {
            email: admin_email.clone(),
            name: "Admin User".to_string(),
            password_hash: admin_hash,
            role: "admin".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    let user_email = format!("user_to_deactivate_{}@example.com", uuid::Uuid::now_v7());
    let user_pass = "UserPass123!";
    let user_hash = pace_api::auth::passwords::hash_password(user_pass).unwrap();

    let user = create_user(
        &pool,
        CreateUser {
            email: user_email.clone(),
            name: "Victim User".to_string(),
            password_hash: user_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    // 2. Regular user signs in
    let user_login = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": user_email,
                "password": user_pass
            })
            .to_string(),
        ))
        .unwrap();
    let user_resp = app.clone().oneshot(user_login).await.unwrap();
    let user_cookie = user_resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // 3. Admin signs in
    let admin_login = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "email": admin_email,
                "password": admin_pass
            })
            .to_string(),
        ))
        .unwrap();
    let admin_resp = app.clone().oneshot(admin_login).await.unwrap();
    let admin_cookie = admin_resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let admin_body = admin_resp.into_body().collect().await.unwrap().to_bytes();
    let admin_json: Value = serde_json::from_slice(&admin_body).unwrap();
    let admin_csrf = admin_json["csrf_token"].as_str().unwrap().to_string();

    // 4. Admin deactivates regular user via PATCH /api/v1/admin/users/:id
    let deactivate_req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/api/v1/admin/users/{}", user.id))
        .header(header::COOKIE, &admin_cookie)
        .header("X-PACE-CSRF", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "is_active": false }).to_string()))
        .unwrap();

    let deact_resp = app.clone().oneshot(deactivate_req).await.unwrap();
    assert_eq!(deact_resp.status(), StatusCode::OK);

    // 5. User's active session is terminated IMMEDIATELY
    let user_try_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, &user_cookie)
        .body(Body::empty())
        .unwrap();

    let user_try_resp = app.oneshot(user_try_req).await.unwrap();
    assert_eq!(user_try_resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_origin_header_enforced_on_mutations() {
    let (app, pool, _) = setup_test_app().await;

    let email = format!("origin_{}@example.com", uuid::Uuid::now_v7());
    let password = "OriginPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    create_user(
        &pool,
        CreateUser {
            email: email.clone(),
            name: "Origin Tester".to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    // 1. Cross-origin login must be rejected with 403
    let evil_login = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::from(
            json!({ "email": email, "password": password }).to_string(),
        ))
        .unwrap();
    let evil_login_resp = app.clone().oneshot(evil_login).await.unwrap();
    assert_eq!(evil_login_resp.status(), StatusCode::FORBIDDEN);

    // 2. Same-origin login succeeds
    let ok_login = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "http://localhost:3000")
        .body(Body::from(
            json!({ "email": email, "password": password }).to_string(),
        ))
        .unwrap();
    let ok_resp = app.clone().oneshot(ok_login).await.unwrap();
    assert_eq!(ok_resp.status(), StatusCode::OK);
    let cookie = ok_resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let body = ok_resp.into_body().collect().await.unwrap().to_bytes();
    let login_json: Value = serde_json::from_slice(&body).unwrap();
    let csrf = login_json["csrf_token"].as_str().unwrap().to_string();

    // 3. Authenticated mutation with foreign Origin must be rejected with 403
    let evil_create = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/projects")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &cookie)
        .header("X-PACE-CSRF", &csrf)
        .header(header::ORIGIN, "https://attacker.example")
        .body(Body::from(
            json!({ "name": "Blocked Project", "aspect_ratio": "16:9" }).to_string(),
        ))
        .unwrap();
    let evil_create_resp = app.clone().oneshot(evil_create).await.unwrap();
    assert_eq!(evil_create_resp.status(), StatusCode::FORBIDDEN);

    // 4. Authenticated mutation with allowed Origin succeeds
    let valid_create = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/projects")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &cookie)
        .header("X-PACE-CSRF", &csrf)
        .header(header::ORIGIN, "http://localhost:3000")
        .body(Body::from(
            json!({ "name": "Allowed Project", "aspect_ratio": "16:9" }).to_string(),
        ))
        .unwrap();
    let valid_create_resp = app.oneshot(valid_create).await.unwrap();
    assert_eq!(valid_create_resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_password_change_terminates_other_sessions_and_rejects_common_passwords() {
    let (app, pool, _) = setup_test_app().await;

    let email = format!("pw_rot_{}@example.com", uuid::Uuid::now_v7());
    let password = "InitialStrongPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    create_user(
        &pool,
        CreateUser {
            email: email.clone(),
            name: "Rotation Tester".to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    // Sign in on two separate sessions (Session A & Session B)
    let mut cookies = Vec::new();
    let mut csrfs = Vec::new();
    for _ in 0..2 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "email": email, "password": password }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        cookies.push(
            resp.headers()
                .get(header::SET_COOKIE)
                .unwrap()
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_string(),
        );
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        csrfs.push(json["csrf_token"].as_str().unwrap().to_string());
    }

    // 1. Common leaked 10+ char password must be rejected
    let weak_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/password")
                .header(header::COOKIE, &cookies[0])
                .header("X-PACE-CSRF", &csrfs[0])
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "current_password": password,
                        "new_password": "password123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(weak_resp.status(), StatusCode::BAD_REQUEST);

    // 2. Valid password change from Session A
    let ok_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/password")
                .header(header::COOKIE, &cookies[0])
                .header("X-PACE-CSRF", &csrfs[0])
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "current_password": password,
                        "new_password": "BrandNewUniquePass987!"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_resp.status(), StatusCode::OK);

    // 3. Session A remains authenticated, Session B must be terminated (401)
    let me_a = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &cookies[0])
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me_a.status(), StatusCode::OK);

    let me_b = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &cookies[1])
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me_b.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_session_sliding_expiry_extends_expires_at() {
    let (app, pool, _) = setup_test_app().await;

    let email = format!("slide_{}@example.com", uuid::Uuid::now_v7());
    let password = "SlidePassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    let user = create_user(
        &pool,
        CreateUser {
            email: email.clone(),
            name: "Slide User".to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "email": email, "password": password }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // Manually wind expires_at back to 5 days from now
    pace_db::sqlx::query("UPDATE sessions SET expires_at = now() + interval '5 days' WHERE user_id = $1")
        .bind(user.id)
        .execute(&pool)
        .await
        .unwrap();

    // Accessing an authenticated endpoint must slide expires_at back toward 30 days
    let me_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me_resp.status(), StatusCode::OK);

    let remaining_days: f64 = pace_db::sqlx::query_scalar(
        "SELECT CAST(EXTRACT(EPOCH FROM (expires_at - now())) AS DOUBLE PRECISION) / 86400.0 FROM sessions WHERE user_id = $1",
    )
    .bind(user.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        remaining_days > 29.0,
        "Expected sliding session expiry > 29 days, got {remaining_days}"
    );
}

#[tokio::test]
async fn test_admin_audit_logging_validation_and_strict_isolation() {
    let (app, pool, _) = setup_test_app().await;

    let admin_email = format!("adm_audit_{}@example.com", uuid::Uuid::now_v7());
    let admin_pass = "AdminAuditPass123!";
    let admin = create_user(
        &pool,
        CreateUser {
            email: admin_email.clone(),
            name: "Audit Admin".to_string(),
            password_hash: pace_api::auth::passwords::hash_password(admin_pass).unwrap(),
            role: "admin".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "email": admin_email, "password": admin_pass }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let admin_cookie = login_resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let admin_body = login_resp.into_body().collect().await.unwrap().to_bytes();
    let admin_json: Value = serde_json::from_slice(&admin_body).unwrap();
    let admin_csrf = admin_json["csrf_token"].as_str().unwrap().to_string();

    // 1. Invalid role rejected with 400 (not 500)
    let bad_role = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/admin/users")
                .header(header::COOKIE, &admin_cookie)
                .header("X-PACE-CSRF", &admin_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "email": format!("bad_{}@example.com", uuid::Uuid::now_v7()),
                        "name": "Bad Role",
                        "temporary_password": "ValidTempPassword123!",
                        "role": "superuser"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_role.status(), StatusCode::BAD_REQUEST);

    // 2. Create creator user -> 201 + audit entry
    let creator_email = format!("cr_audit_{}@example.com", uuid::Uuid::now_v7());
    let creator_pass = "CreatorTempPass123!";
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/admin/users")
                .header(header::COOKIE, &admin_cookie)
                .header("X-PACE-CSRF", &admin_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "email": creator_email,
                        "name": "Audited Creator",
                        "temporary_password": creator_pass,
                        "role": "creator"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created_body = create_resp.into_body().collect().await.unwrap().to_bytes();
    let created_json: Value = serde_json::from_slice(&created_body).unwrap();
    let creator_id = created_json["id"].as_str().unwrap().to_string();

    // 3. Duplicate email returns 409 Conflict (not 500)
    let dup_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/admin/users")
                .header(header::COOKIE, &admin_cookie)
                .header("X-PACE-CSRF", &admin_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "email": creator_email,
                        "name": "Duplicate",
                        "temporary_password": creator_pass
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dup_resp.status(), StatusCode::CONFLICT);

    // 4. Reset password & deactivate -> audit_log records all 3 actions
    let reset_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/admin/users/{creator_id}/reset-password"))
                .header(header::COOKIE, &admin_cookie)
                .header("X-PACE-CSRF", &admin_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "temporary_password": "ResetTempPassword123!" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reset_resp.status(), StatusCode::OK);

    let deact_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/admin/users/{creator_id}"))
                .header(header::COOKIE, &admin_cookie)
                .header("X-PACE-CSRF", &admin_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "is_active": false }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deact_resp.status(), StatusCode::OK);

    // 5. Verify admin cannot access creator's project or media endpoints (AUTH-05, ADM-01)
    let creator_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "email": admin_email, "password": admin_pass }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(creator_login.status(), StatusCode::OK);

    // Create a second active creator, have them create a project, and confirm admin gets 404
    let creator2_email = format!("cr2_{}@example.com", uuid::Uuid::now_v7());
    let creator2_pass = "CreatorTwoPass123!";
    let _creator2 = create_user(
        &pool,
        CreateUser {
            email: creator2_email.clone(),
            name: "Creator Two".to_string(),
            password_hash: pace_api::auth::passwords::hash_password(creator2_pass).unwrap(),
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();
    let cr2_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "email": creator2_email, "password": creator2_pass }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cr2_cookie = cr2_login
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let cr2_body: Value = serde_json::from_slice(
        &cr2_login.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let cr2_csrf = cr2_body["csrf_token"].as_str().unwrap().to_string();

    let cr2_proj_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/projects")
                .header(header::COOKIE, &cr2_cookie)
                .header("X-PACE-CSRF", &cr2_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "name": "Creator Private Doc", "aspect_ratio": "16:9" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cr2_proj_resp.status(), StatusCode::CREATED);
    let cr2_proj_json: Value = serde_json::from_slice(
        &cr2_proj_resp.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let cr2_proj_id = cr2_proj_json["id"].as_str().unwrap();

    let admin_get_creator_proj = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{cr2_proj_id}"))
                .header(header::COOKIE, &admin_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_get_creator_proj.status(), StatusCode::NOT_FOUND);

    let admin_presign_creator = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{cr2_proj_id}/uploads/presign"))
                .header(header::COOKIE, &admin_cookie)
                .header("X-PACE-CSRF", &admin_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "filename": "snoop.png", "content_type": "image/png", "bytes": 128 }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_presign_creator.status(), StatusCode::NOT_FOUND);

    let actions: Vec<String> = pace_db::sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE actor_id = $1 AND target_id = $2 ORDER BY created_at ASC",
    )
    .bind(admin.id)
    .bind(&creator_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        actions,
        vec![
            "user.create".to_string(),
            "user.reset_password".to_string(),
            "user.deactivate".to_string(),
        ]
    );
}

#[tokio::test]
async fn test_auth_hardening_headers_cookie_format_and_password_reuse() {
    let (app, pool, _) = setup_test_app().await;

    let email = format!("harden_{}@example.com", uuid::Uuid::now_v7());
    let password = "DistinctInitialPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    create_user(
        &pool,
        CreateUser {
            email: email.clone(),
            name: "Hardening Tester".to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    // 1. Malformed session cookie is rejected before querying DB
    let bad_cookie_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, "pace_session=not-a-valid-64char-hex-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_cookie_resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        bad_cookie_resp
            .headers()
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        bad_cookie_resp
            .headers()
            .get(header::X_FRAME_OPTIONS)
            .and_then(|v| v.to_str().ok()),
        Some("DENY")
    );
    assert_eq!(
        bad_cookie_resp
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-store, max-age=0")
    );

    // 2. Empty email or password fails cleanly with 401 without panicking
    let empty_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "email": "   ", "password": "" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(empty_resp.status(), StatusCode::UNAUTHORIZED);

    // 3. Valid login receives security headers and can test password reuse rejection
    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "email": email, "password": password }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_resp.status(), StatusCode::OK);
    let cookie = login_resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let body = login_resp.into_body().collect().await.unwrap().to_bytes();
    let login_json: Value = serde_json::from_slice(&body).unwrap();
    let csrf = login_json["csrf_token"].as_str().unwrap().to_string();

    let reuse_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/password")
                .header(header::COOKIE, &cookie)
                .header("X-PACE-CSRF", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "current_password": password,
                        "new_password": password
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reuse_resp.status(), StatusCode::BAD_REQUEST);
}
