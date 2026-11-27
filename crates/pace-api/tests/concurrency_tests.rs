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

/// PRD PRJ-05: Optimistic Concurrency Control with If-Match.
///
/// Modifying a project requires an If-Match header with the expected version.
/// Stale versions are rejected with 409 Conflict (PROJECT_VERSION_CONFLICT).
#[tokio::test]
async fn test_optimistic_concurrency_control_with_if_match_header() {
    let (app, pool) = setup_test_app().await;
    let user = create_and_login_user(&app, &pool, "author").await;

    // 1. Create a project
    let create_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/projects")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &user.cookie)
        .header("X-PACE-CSRF", &user.csrf_token)
        .body(Body::from(
            json!({
                "name": "Original Title",
                "aspect_ratio": "16:9"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let etag = resp
        .headers()
        .get(header::ETAG)
        .expect("Missing ETag header on created project")
        .to_str()
        .unwrap();
    assert_eq!(etag, "\"1\"");

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let project: Value = serde_json::from_slice(&body).unwrap();
    let project_id = project["id"].as_str().expect("Missing project id");
    assert_eq!(project["version"], 1);

    // 2. Perform concurrent edit simulation:
    // Tab 1 updates project with If-Match: "1" -> should succeed and bump to version 2
    let patch_req_1 = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/api/v1/projects/{}", project_id))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &user.cookie)
        .header("X-PACE-CSRF", &user.csrf_token)
        .header(header::IF_MATCH, "\"1\"")
        .body(Body::from(
            json!({
                "name": "Updated by Tab 1"
            })
            .to_string(),
        ))
        .unwrap();

    let resp_1 = app.clone().oneshot(patch_req_1).await.unwrap();
    assert_eq!(resp_1.status(), StatusCode::OK);

    let etag_1 = resp_1
        .headers()
        .get(header::ETAG)
        .expect("Missing ETag on update")
        .to_str()
        .unwrap();
    assert_eq!(etag_1, "\"2\"");

    let body_1 = resp_1.into_body().collect().await.unwrap().to_bytes();
    let project_1: Value = serde_json::from_slice(&body_1).unwrap();
    assert_eq!(project_1["name"], "Updated by Tab 1");
    assert_eq!(project_1["version"], 2);

    // 3. Tab 2 (holding stale version 1) attempts update with If-Match: "1"
    // MUST fail with 409 CONFLICT and ProblemDetails
    let patch_req_stale = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/api/v1/projects/{}", project_id))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &user.cookie)
        .header("X-PACE-CSRF", &user.csrf_token)
        .header(header::IF_MATCH, "\"1\"")
        .body(Body::from(
            json!({
                "name": "Conflicting Update from Tab 2"
            })
            .to_string(),
        ))
        .unwrap();

    let resp_stale = app.clone().oneshot(patch_req_stale).await.unwrap();
    assert_eq!(resp_stale.status(), StatusCode::CONFLICT);

    let body_stale = resp_stale.into_body().collect().await.unwrap().to_bytes();
    let problem: ProblemDetails = serde_json::from_slice(&body_stale).unwrap();
    assert_eq!(problem.code, "PROJECT_VERSION_CONFLICT");
    assert_eq!(problem.status, 409);

    // 4. Tab 2 fetches fresh state, gets version 2, and updates with If-Match: "2"
    let patch_req_fresh = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/api/v1/projects/{}", project_id))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &user.cookie)
        .header("X-PACE-CSRF", &user.csrf_token)
        .header(header::IF_MATCH, "\"2\"")
        .body(Body::from(
            json!({
                "name": "Resolved Update from Tab 2"
            })
            .to_string(),
        ))
        .unwrap();

    let resp_fresh = app.clone().oneshot(patch_req_fresh).await.unwrap();
    assert_eq!(resp_fresh.status(), StatusCode::OK);

    let etag_fresh = resp_fresh
        .headers()
        .get(header::ETAG)
        .expect("Missing ETag on fresh update")
        .to_str()
        .unwrap();
    assert_eq!(etag_fresh, "\"3\"");

    let body_fresh = resp_fresh.into_body().collect().await.unwrap().to_bytes();
    let project_fresh: Value = serde_json::from_slice(&body_fresh).unwrap();
    assert_eq!(project_fresh["name"], "Resolved Update from Tab 2");
    assert_eq!(project_fresh["version"], 3);
}

#[tokio::test]
async fn test_project_validation_rejects_invalid_inputs() {
    let (app, pool) = setup_test_app().await;
    let user = create_and_login_user(&app, &pool, "validator").await;

    // 1. Invalid aspect ratio (only "16:9" and "9:16" allowed per PRD PRJ-01)
    let bad_aspect_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/projects")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &user.cookie)
        .header("X-PACE-CSRF", &user.csrf_token)
        .body(Body::from(
            json!({
                "name": "Invalid Ratio Video",
                "aspect_ratio": "4:3"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(bad_aspect_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let problem: ProblemDetails = serde_json::from_slice(&body).unwrap();
    assert_eq!(problem.code, "BAD_REQUEST");

    // 2. Empty project name
    let empty_name_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/projects")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &user.cookie)
        .header("X-PACE-CSRF", &user.csrf_token)
        .body(Body::from(
            json!({
                "name": "   ",
                "aspect_ratio": "16:9"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(empty_name_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 3. Unsupported fps (v1 supports fps = 30 only)
    let bad_fps_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/projects")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &user.cookie)
        .header("X-PACE-CSRF", &user.csrf_token)
        .body(Body::from(
            json!({
                "name": "Bad FPS Project",
                "aspect_ratio": "16:9",
                "fps": 24
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(bad_fps_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 4. Create a valid project then reject invalid status update
    let valid_proj_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/projects")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, &user.cookie)
                .header("X-PACE-CSRF", &user.csrf_token)
                .body(Body::from(
                    json!({
                        "name": "Status Check Project",
                        "aspect_ratio": "16:9"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(valid_proj_resp.status(), StatusCode::CREATED);
    let proj_body: Value = serde_json::from_slice(
        &valid_proj_resp.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let proj_id = proj_body["id"].as_str().unwrap();

    let bad_status_resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{proj_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, &user.cookie)
                .header("X-PACE-CSRF", &user.csrf_token)
                .header(header::IF_MATCH, "\"1\"")
                .body(Body::from(json!({ "status": "bogus" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_status_resp.status(), StatusCode::BAD_REQUEST);
}

