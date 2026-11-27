use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use pace_api::{
    app::{create_app, AppState},
    config::AppConfig,
};
use pace_db::{create_pool, create_user, run_migrations, CreateUser};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> (axum::Router, pace_db::DbPool) {
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

    let state = AppState::new(pool.clone(), config);

    let app = create_app(state);
    (app, pool)
}

struct TestUserSession {
    #[allow(dead_code)]
    user_id: uuid::Uuid,
    cookie: String,
    csrf_token: String,
}

async fn create_and_login_user(
    app: &axum::Router,
    pool: &pace_db::DbPool,
    name: &str,
) -> TestUserSession {
    let email = format!("{}_{}@example.com", name.to_lowercase(), uuid::Uuid::now_v7());
    let password = "SecretPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    let user = create_user(
        pool,
        CreateUser {
            email: email.clone(),
            name: name.to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(2 * 1024 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

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

    let cookie_header = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("Missing Set-Cookie header")
        .to_str()
        .unwrap()
        .to_string();

    let session_cookie = cookie_header
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_json: Value = serde_json::from_slice(&body).unwrap();
    let csrf_token = body_json["csrf_token"].as_str().unwrap().to_string();

    TestUserSession {
        user_id: user.id,
        cookie: session_cookie,
        csrf_token,
    }
}

/// PRD AUTH-05: Strict Data Ownership Isolation.
///
/// Any request to access or modify a resource owned by another user MUST return 404 Not Found
/// (never 403 Forbidden), preventing malicious user enumeration of project IDs.
#[tokio::test]
async fn test_strict_multi_tenancy_isolation_returns_404_across_all_endpoints() {
    let (app, pool) = setup_test_app().await;

    // 1. Create two independent users: User A and User B
    let user_a = create_and_login_user(&app, &pool, "user_a").await;
    let user_b = create_and_login_user(&app, &pool, "user_b").await;

    // 2. User A creates a project
    let create_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/projects")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &user_a.cookie)
        .header("X-PACE-CSRF", &user_a.csrf_token)
        .body(Body::from(
            json!({
                "name": "User A Documentary",
                "aspect_ratio": "16:9"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let project_a: Value = serde_json::from_slice(&body).unwrap();
    let project_a_id = project_a["id"].as_str().expect("Missing project id");

    // 3. User B lists projects -> User A's project must NOT be returned (PRJ-02, AUTH-05)
    let list_b_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/projects")
        .header(header::COOKIE, &user_b.cookie)
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(list_b_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let list_b_body = resp.into_body().collect().await.unwrap().to_bytes();
    let list_b: Vec<Value> = serde_json::from_slice(&list_b_body).unwrap();
    assert!(
        !list_b.iter().any(|p| p["id"] == project_a_id),
        "User A's project must not appear in User B's project list"
    );

    // 4. User B attempts GET /api/v1/projects/:id -> MUST return 404 NOT_FOUND (NOT 403)
    let get_req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/projects/{}", project_a_id))
        .header(header::COOKIE, &user_b.cookie)
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "Accessing another user's project must return 404 Not Found"
    );

    // 5. User B attempts PATCH /api/v1/projects/:id -> MUST return 404 NOT_FOUND
    let patch_req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/api/v1/projects/{}", project_a_id))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &user_b.cookie)
        .header("X-PACE-CSRF", &user_b.csrf_token)
        .header(header::IF_MATCH, "\"1\"")
        .body(Body::from(
            json!({
                "name": "Hijacked Project"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(patch_req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "Modifying another user's project must return 404 Not Found"
    );

    // 6. User B attempts DELETE /api/v1/projects/:id -> MUST return 404 NOT_FOUND
    let delete_req = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/v1/projects/{}", project_a_id))
        .header(header::COOKIE, &user_b.cookie)
        .header("X-PACE-CSRF", &user_b.csrf_token)
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(delete_req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "Deleting another user's project must return 404 Not Found"
    );

    // 7. User A deletes their project -> 204 NO_CONTENT
    let delete_a_req = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/v1/projects/{}", project_a_id))
        .header(header::COOKIE, &user_a.cookie)
        .header("X-PACE-CSRF", &user_a.csrf_token)
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(delete_a_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 8. User A attempts GET on deleted project -> 404 NOT_FOUND
    let get_deleted_req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/projects/{}", project_a_id))
        .header(header::COOKIE, &user_a.cookie)
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(get_deleted_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // 9. User A lists projects -> deleted project is not returned
    let list_a_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/projects")
        .header(header::COOKIE, &user_a.cookie)
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(list_a_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let list_a_body = resp.into_body().collect().await.unwrap().to_bytes();
    let list_a: Vec<Value> = serde_json::from_slice(&list_a_body).unwrap();
    assert!(
        !list_a.iter().any(|p| p["id"] == project_a_id),
        "Soft-deleted project must not appear in project list"
    );
}

