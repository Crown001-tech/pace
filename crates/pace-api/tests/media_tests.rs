use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use pace_api::{
    app::{create_app, AppState},
    config::AppConfig,
    error::ProblemDetails,
    routes::media::MediaResponse,
    routes::scripts::ScriptResponse,
    routes::uploads::PresignUploadResponse,
};
use pace_db::{create_pool, create_user, run_migrations, CreateUser};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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
    user_id: uuid::Uuid,
    cookie: String,
    csrf_token: String,
}

async fn create_authenticated_user(app: &axum::Router, pool: &pace_db::DbPool) -> TestUserSession {
    let email = format!("user_{}@example.com", uuid::Uuid::now_v7());
    let password = "SecretPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    let user = create_user(
        pool,
        CreateUser {
            email: email.clone(),
            name: "Media Tester".to_string(),
            password_hash,
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(10 * 1024 * 1024), // 10 MB quota
        },
    )
    .await
    .unwrap();

    let login_payload = json!({ "email": email, "password": password });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let csrf_token = body["csrf_token"].as_str().unwrap().to_string();

    TestUserSession {
        user_id: user.id,
        cookie,
        csrf_token,
    }
}

async fn create_test_project(
    app: &axum::Router,
    session: &TestUserSession,
    name: &str,
) -> uuid::Uuid {
    let payload = json!({ "name": name, "aspect_ratio": "16:9" });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/projects")
                .header(header::COOKIE, &session.cookie)
                .header("x-pace-csrf", &session.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    uuid::Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

async fn presign_and_put_minio(
    app: &axum::Router,
    user: &TestUserSession,
    project_id: uuid::Uuid,
    filename: &str,
    content_type: &str,
    data: Vec<u8>,
) -> PresignUploadResponse {
    let presign_payload = json!({
        "filename": filename,
        "content_type": content_type,
        "bytes": data.len()
    });

    let presign_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/uploads/presign"))
                .header(header::COOKIE, &user.cookie)
                .header("x-pace-csrf", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&presign_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(presign_resp.status(), StatusCode::OK);
    let body_bytes = presign_resp.into_body().collect().await.unwrap().to_bytes();
    let presigned: PresignUploadResponse = serde_json::from_slice(&body_bytes).unwrap();

    let client = reqwest::Client::new();
    let put_orig = client
        .put(&presigned.upload_url)
        .header("Content-Type", content_type)
        .body(data)
        .send()
        .await
        .expect("MinIO PUT original must succeed");
    assert!(
        put_orig.status().is_success(),
        "MinIO PUT original failed: {}",
        put_orig.status()
    );

    let put_preview = client
        .put(&presigned.preview_upload_url)
        .header("Content-Type", "image/webp")
        .body(b"fake-webp-preview".to_vec())
        .send()
        .await
        .unwrap();
    assert!(put_preview.status().is_success());

    let put_thumb = client
        .put(&presigned.thumb_upload_url)
        .header("Content-Type", "image/webp")
        .body(b"fake-webp-thumb".to_vec())
        .send()
        .await
        .unwrap();
    assert!(put_thumb.status().is_success());

    presigned
}

#[tokio::test]
async fn test_presign_upload_generates_valid_url_and_validates_type() {
    let (app, pool) = setup_test_app().await;
    let user = create_authenticated_user(&app, &pool).await;
    let project_id = create_test_project(&app, &user, "Presign Test").await;

    let payload = json!({
        "filename": "hero_scene.png",
        "content_type": "image/png",
        "bytes": 2 * 1024 * 1024 // 2 MB
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/uploads/presign"))
                .header(header::COOKIE, &user.cookie)
                .header("x-pace-csrf", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let res: PresignUploadResponse = serde_json::from_slice(&body_bytes).unwrap();

    assert!(res.upload_url.contains("http://localhost:9000/pace/u/"));
    assert!(res.upload_url.contains("X-Amz-Signature="));
    assert!(res.storage_key.contains("original.png"));
    assert!(res.preview_key.ends_with("/preview.webp"));
    assert!(res.thumb_key.ends_with("/thumb.webp"));

    // Reject non-image MIME type
    let bad_mime = json!({
        "filename": "malware.pdf",
        "content_type": "application/pdf",
        "bytes": 1024
    });
    let bad_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/uploads/presign"))
                .header(header::COOKIE, &user.cookie)
                .header("x-pace-csrf", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&bad_mime).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_presign_upload_blocks_when_quota_exceeded() {
    let (app, pool) = setup_test_app().await;
    let user = create_authenticated_user(&app, &pool).await; // 10 MB quota
    let project_id = create_test_project(&app, &user, "Quota Test").await;

    // Request 15 MB upload when limit is 10 MB
    let payload = json!({
        "filename": "giant_image.png",
        "content_type": "image/png",
        "bytes": 15 * 1024 * 1024
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/uploads/presign"))
                .header(header::COOKIE, &user.cookie)
                .header("x-pace-csrf", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let problem: ProblemDetails = serde_json::from_slice(&body_bytes).unwrap();
    assert!(problem.detail.contains("Storage limit exceeded"));
}

#[tokio::test]
async fn test_register_media_rejects_missing_s3_object_and_foreign_key() {
    let (app, pool) = setup_test_app().await;
    let user = create_authenticated_user(&app, &pool).await;
    let project_id = create_test_project(&app, &user, "Validation Test").await;

    // 1. Foreign prefix must be rejected (400)
    let foreign_payload = json!({
        "storage_key": "u/00000000-0000-0000-0000-000000000000/p/00000000-0000-0000-0000-000000000000/media/x/original.png",
        "original_filename": "photo.png",
        "mime": "image/png",
        "bytes": 1024,
        "sha256": "fake_hash_1",
        "width": 100,
        "height": 100
    });
    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/media"))
                .header(header::COOKIE, &user.cookie)
                .header("x-pace-csrf", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&foreign_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::BAD_REQUEST);

    // 2. Valid prefix but object never uploaded to MinIO -> 400
    let missing_key = format!(
        "u/{}/p/{}/media/{}/original.png",
        user.user_id,
        project_id,
        uuid::Uuid::new_v4()
    );
    let missing_payload = json!({
        "storage_key": missing_key,
        "original_filename": "missing.png",
        "mime": "image/png",
        "bytes": 1024,
        "sha256": "fake_hash_2"
    });
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/media"))
                .header(header::COOKIE, &user.cookie)
                .header("x-pace-csrf", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&missing_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_register_duplicate_sha256_returns_conflict_and_delete_cleans_minio() {
    let (app, pool) = setup_test_app().await;
    let user = create_authenticated_user(&app, &pool).await;
    let project_id = create_test_project(&app, &user, "Duplicate & Delete Test").await;

    let sample_bytes = b"fake-png-bytes-for-m4-test".to_vec();
    let digest = sha256_hex(&sample_bytes);
    let presigned1 =
        presign_and_put_minio(&app, &user, project_id, "photo.png", "image/png", sample_bytes.clone())
            .await;

    let payload1 = json!({
        "storage_key": presigned1.storage_key,
        "preview_key": presigned1.preview_key,
        "thumb_key": presigned1.thumb_key,
        "original_filename": "photo.png",
        "mime": "image/png",
        "bytes": sample_bytes.len(),
        "sha256": digest,
        "width": 1920,
        "height": 1080
    });

    // First registration succeeds
    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/media"))
                .header(header::COOKIE, &user.cookie)
                .header("x-pace-csrf", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload1).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::CREATED);
    let created: MediaResponse = serde_json::from_slice(
        &resp1.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert!(created.preview_url.is_some());
    assert!(created.thumb_url.is_some());

    // Second upload with same sha256 returns 409 Conflict (MED-04)
    let presigned2 =
        presign_and_put_minio(&app, &user, project_id, "photo_copy.png", "image/png", sample_bytes.clone())
            .await;
    let payload2 = json!({
        "storage_key": presigned2.storage_key,
        "preview_key": presigned2.preview_key,
        "thumb_key": presigned2.thumb_key,
        "original_filename": "photo_copy.png",
        "mime": "image/png",
        "bytes": sample_bytes.len(),
        "sha256": digest,
        "width": 1920,
        "height": 1080
    });
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/media"))
                .header(header::COOKIE, &user.cookie)
                .header("x-pace-csrf", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::CONFLICT);

    // Delete first asset -> reclaims quota and removes MinIO objects
    let del_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/projects/{project_id}/media/{}", created.id))
                .header(header::COOKIE, &user.cookie)
                .header("x-pace-csrf", &user.csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    // Verify MinIO object was deleted: fetching the previously signed download URL now returns 404
    let head_deleted = reqwest::Client::new()
        .get(&created.download_url)
        .send()
        .await
        .unwrap();
    assert_eq!(head_deleted.status(), reqwest::StatusCode::NOT_FOUND);

    // Verify user quota reclaimed to 0
    let me_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &user.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let me_json: Value = serde_json::from_slice(&me_resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(me_json["storage_used_bytes"], 0);
}

#[tokio::test]
async fn test_script_sections_preserve_assignments_and_reject_cross_project_idor() {
    let (app, pool) = setup_test_app().await;
    let user_a = create_authenticated_user(&app, &pool).await;
    let user_b = create_authenticated_user(&app, &pool).await;

    let proj_a1 = create_test_project(&app, &user_a, "Project A1").await;
    let proj_a2 = create_test_project(&app, &user_a, "Project A2").await;
    let proj_b = create_test_project(&app, &user_b, "Project B").await;

    // 1. Save script in A1 with two sections
    let save_a1 = json!({
        "sections": [
            { "text": "Scene one." },
            { "text": "Scene two." }
        ]
    });
    let resp_save = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{proj_a1}/script"))
                .header(header::COOKIE, &user_a.cookie)
                .header("x-pace-csrf", &user_a.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&save_a1).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_save.status(), StatusCode::OK);
    let script_a1: ScriptResponse = serde_json::from_slice(
        &resp_save.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(script_a1.sections.len(), 2);
    let sec1_id = script_a1.sections[0].id;
    let sec2_id = script_a1.sections[1].id;

    // Save script in B
    let save_b = json!({ "sections": [{ "text": "Secret B." }] });
    let resp_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{proj_b}/script"))
                .header(header::COOKIE, &user_b.cookie)
                .header("x-pace-csrf", &user_b.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&save_b).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let script_b: ScriptResponse = serde_json::from_slice(
        &resp_b.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let sec_b_id = script_b.sections[0].id;

    // Upload media in A1 and A2
    let bytes_a1 = b"media-a1-bytes".to_vec();
    let pre_a1 = presign_and_put_minio(&app, &user_a, proj_a1, "a1.png", "image/png", bytes_a1.clone()).await;
    let reg_a1: MediaResponse = serde_json::from_slice(
        &app.clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/projects/{proj_a1}/media"))
                    .header(header::COOKIE, &user_a.cookie)
                    .header("x-pace-csrf", &user_a.csrf_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&json!({
                        "storage_key": pre_a1.storage_key,
                        "original_filename": "a1.png",
                        "mime": "image/png",
                        "bytes": bytes_a1.len(),
                        "sha256": sha256_hex(&bytes_a1)
                    })).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();

    let bytes_a2 = b"media-a2-bytes".to_vec();
    let pre_a2 = presign_and_put_minio(&app, &user_a, proj_a2, "a2.png", "image/png", bytes_a2.clone()).await;
    let reg_a2: MediaResponse = serde_json::from_slice(
        &app.clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/projects/{proj_a2}/media"))
                    .header(header::COOKIE, &user_a.cookie)
                    .header("x-pace-csrf", &user_a.csrf_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&json!({
                        "storage_key": pre_a2.storage_key,
                        "original_filename": "a2.png",
                        "mime": "image/png",
                        "bytes": bytes_a2.len(),
                        "sha256": sha256_hex(&bytes_a2)
                    })).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();

    // Assign reg_a1 -> sec1_id in proj_a1
    let assign_ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{proj_a1}/assignments/{sec1_id}"))
                .header(header::COOKIE, &user_a.cookie)
                .header("x-pace-csrf", &user_a.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&json!({
                    "media_id": reg_a1.id,
                    "fit_mode": "fit_blur",
                    "focal_x": 0.4,
                    "focal_y": 0.6
                })).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(assign_ok.status(), StatusCode::OK);

    // Edit & reorder script in A1 keeping sec1_id -> assignment must survive!
    let save_a1_v2 = json!({
        "sections": [
            { "id": sec2_id, "text": "Scene two moved first." },
            { "id": sec1_id, "text": "Scene one edited." }
        ]
    });
    let resp_v2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{proj_a1}/script"))
                .header(header::COOKIE, &user_a.cookie)
                .header("x-pace-csrf", &user_a.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&save_a1_v2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_v2.status(), StatusCode::OK);

    let list_assign_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{proj_a1}/assignments"))
                .header(header::COOKIE, &user_a.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let assign_list: Vec<Value> = serde_json::from_slice(
        &list_assign_resp.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(assign_list.len(), 1);
    assert_eq!(assign_list[0]["section_id"], json!(sec1_id));
    assert_eq!(assign_list[0]["media_id"], json!(reg_a1.id));

    // Cross-project IDOR: assign media from proj_a2 to sec1_id in proj_a1 -> 404
    let idor_media = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{proj_a1}/assignments/{sec1_id}"))
                .header(header::COOKIE, &user_a.cookie)
                .header("x-pace-csrf", &user_a.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&json!({ "media_id": reg_a2.id })).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idor_media.status(), StatusCode::NOT_FOUND);

    // Cross-user IDOR: assign in proj_a1 using user_b's section -> 404
    let idor_sec = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{proj_a1}/assignments/{sec_b_id}"))
                .header(header::COOKIE, &user_a.cookie)
                .header("x-pace-csrf", &user_a.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&json!({ "media_id": reg_a1.id })).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idor_sec.status(), StatusCode::NOT_FOUND);

    // Cross-project swap -> 404
    let idor_swap = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{proj_a1}/assignments/swap"))
                .header(header::COOKIE, &user_a.cookie)
                .header("x-pace-csrf", &user_a.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&json!({
                    "section_id_a": sec1_id,
                    "section_id_b": sec_b_id
                })).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idor_swap.status(), StatusCode::NOT_FOUND);

    // Cross-project delete media -> 404
    let idor_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/projects/{proj_a2}/media/{}", reg_a1.id))
                .header(header::COOKIE, &user_a.cookie)
                .header("x-pace-csrf", &user_a.csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idor_del.status(), StatusCode::NOT_FOUND);

    // Cross-user script GET -> 404
    let idor_script = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{proj_a1}/script"))
                .header(header::COOKIE, &user_b.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idor_script.status(), StatusCode::NOT_FOUND);

    // Clean up MinIO test objects
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/projects/{proj_a1}/media/{}", reg_a1.id))
                .header(header::COOKIE, &user_a.cookie)
                .header("x-pace-csrf", &user_a.csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/projects/{proj_a2}/media/{}", reg_a2.id))
                .header(header::COOKIE, &user_a.cookie)
                .header("x-pace-csrf", &user_a.csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
}
