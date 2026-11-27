use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use pace_api::{
    app::{create_app, AppState},
    config::AppConfig,
    routes::exports::ExportResponse,
};
use pace_db::{create_media, create_pool, create_user, run_migrations, CreateMediaAsset, CreateUser};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> (axum::Router, pace_db::DbPool) {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pace:pace@localhost:5432/pace".to_string());
    let pool = create_pool(&database_url).await.expect("connect test db");
    run_migrations(&pool).await.expect("migrations ok");

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
    (create_app(state), pool)
}

struct TestSession {
    user_id: uuid::Uuid,
    cookie: String,
    csrf_token: String,
}

async fn create_authenticated_user(app: &axum::Router, pool: &pace_db::DbPool) -> TestSession {
    let email = format!("exp_api_{}@example.com", uuid::Uuid::now_v7());
    let password = "SecretPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    let user = create_user(
        pool,
        CreateUser {
            email: email.clone(),
            name: "Export API User".to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(50 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    let login_res = app
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

    assert_eq!(login_res.status(), StatusCode::OK);
    let cookie = login_res
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let body = login_res.into_body().collect().await.unwrap().to_bytes();
    let parsed: Value = serde_json::from_slice(&body).unwrap();
    let csrf_token = parsed["csrf_token"].as_str().unwrap().to_string();

    TestSession {
        user_id: user.id,
        cookie,
        csrf_token,
    }
}

#[tokio::test]
async fn export_validates_assignments_enqueues_render_job_and_enforces_isolation() {
    let (app, pool) = setup_test_app().await;
    let owner = create_authenticated_user(&app, &pool).await;
    let intruder = create_authenticated_user(&app, &pool).await;

    let create_proj = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/projects")
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "M8 Export Project",
                        "aspect_ratio": "9:16",
                        "fps": 30
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_proj.status(), StatusCode::CREATED);
    let proj_json: Value = serde_json::from_slice(
        &create_proj.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let project_id = uuid::Uuid::parse_str(proj_json["id"].as_str().unwrap()).unwrap();

    // Save 2 script sections
    let save_script = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{project_id}/script"))
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "sections": [
                            { "position": 0, "text": "Scene one narration text." },
                            { "position": 1, "text": "Scene two narration text." }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save_script.status(), StatusCode::OK);

    // Without assigned images, POST /exports must return 400
    let blocked = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/exports"))
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "resolution": "720p" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::BAD_REQUEST);

    // Upload/register 2 images & auto-assign
    for idx in 0..2 {
        let key = format!("u/{}/p/{}/img-{idx}.jpg", owner.user_id, project_id);
        create_media(
            &pool,
            CreateMediaAsset {
                project_id,
                owner_id: owner.user_id,
                kind: "image".to_string(),
                storage_key: key,
                preview_key: None,
                thumb_key: None,
                original_filename: format!("img-{idx}.jpg"),
                mime: "image/jpeg".to_string(),
                bytes: 2048,
                sha256: format!("{idx:064x}"),
                width: Some(1080),
                height: Some(1920),
                duration_ms: None,
                source: Some("upload".to_string()),
                prompt: None,
            },
        )
        .await
        .unwrap();
    }

    let auto_assign = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{project_id}/assignments/auto"))
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(auto_assign.status(), StatusCode::OK);

    // Start 720p export -> 202 Accepted with 720x1280 for 9:16
    let start_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/exports"))
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "resolution": "720p" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_res.status(), StatusCode::ACCEPTED);
    let created_export: ExportResponse =
        serde_json::from_slice(&start_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(created_export.width, 720);
    assert_eq!(created_export.height, 1280);
    assert_eq!(created_export.status, "queued");
    assert!(created_export.job_id.is_some());

    // Concurrent/duplicate export while one is queued/running must be rejected with 409 Conflict
    let duplicate_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/exports"))
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "resolution": "720p" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(duplicate_res.status(), StatusCode::CONFLICT);

    // Multi-tenant isolation -> 404 for another user
    let intruder_list = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/exports"))
                .header(header::COOKIE, &intruder.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(intruder_list.status(), StatusCode::NOT_FOUND);

    // Owner marks failed and tests POST /retry
    pace_db::mark_export_failed(&pool, created_export.id, "TEST_INTERRUPTION", "simulated")
        .await
        .unwrap();

    let retry_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/projects/{project_id}/exports/{}/retry",
                    created_export.id
                ))
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(retry_res.status(), StatusCode::ACCEPTED);
    let retried: ExportResponse =
        serde_json::from_slice(&retry_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(retried.status, "queued");
    assert_ne!(retried.job_id, created_export.job_id);

    // Clean up ephemeral test jobs/users so local render workers don't pick up synthetic test records.
    let _ = pace_db::sqlx::query("DELETE FROM users WHERE id IN ($1, $2)")
        .bind(owner.user_id)
        .bind(intruder.user_id)
        .execute(&pool)
        .await;
}
