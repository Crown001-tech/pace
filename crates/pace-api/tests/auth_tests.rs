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
    };

    let state = AppState {
        db: pool.clone(),
        config: config.clone(),
    };

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
    let session_cookie = cookie_header.split(';').next().unwrap().trim().to_string();

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
