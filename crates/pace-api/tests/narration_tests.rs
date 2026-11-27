use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use pace_api::{
    app::{create_app, AppState},
    config::AppConfig,
    routes::narration::{
        BoundaryDto, NarrationStateResponse, PresignNarrationResponse, RegisterNarrationResponse,
        SubtitleCueDto,
    },
};
use pace_db::{create_pool, create_user, run_migrations, CreateUser};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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

struct TestUserSession {
    user_id: uuid::Uuid,
    cookie: String,
    csrf_token: String,
}

async fn create_authenticated_user(app: &axum::Router, pool: &pace_db::DbPool) -> TestUserSession {
    let email = format!("nar_api_{}@example.com", uuid::Uuid::now_v7());
    let password = "SecretPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    let user = create_user(
        pool,
        CreateUser {
            email: email.clone(),
            name: "Narration API User".to_string(),
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

    TestUserSession {
        user_id: user.id,
        cookie,
        csrf_token,
    }
}

async fn create_test_project(app: &axum::Router, session: &TestUserSession, name: &str) -> uuid::Uuid {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/projects")
                .header(header::COOKIE, &session.cookie)
                .header("x-pace-csrf", &session.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "name": name, "aspect_ratio": "16:9" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let val: Value = serde_json::from_slice(&body).unwrap();
    uuid::Uuid::parse_str(val["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn test_narration_presign_register_job_boundary_subtitle_and_idor() {
    let (app, pool) = setup_test_app().await;
    let owner = create_authenticated_user(&app, &pool).await;
    let intruder = create_authenticated_user(&app, &pool).await;
    let project_id = create_test_project(&app, &owner, "Narration Flow").await;

    // 1. Reject unsupported MIME type
    let bad_presign = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/narration/presign"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "filename": "notes.txt",
                        "content_type": "text/plain",
                        "bytes": 128
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_presign.status(), StatusCode::BAD_REQUEST);

    // 2. Presign valid audio & upload dummy bytes to MinIO
    let payload_bytes = b"RIFF....WAVEfmt dummy audio bytes for integration test".to_vec();
    let presign_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/narration/presign"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "filename": "voice.wav",
                        "content_type": "audio/wav",
                        "bytes": payload_bytes.len()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(presign_res.status(), StatusCode::OK);
    let presign_body = presign_res.into_body().collect().await.unwrap().to_bytes();
    let presign: PresignNarrationResponse = serde_json::from_slice(&presign_body).unwrap();

    let client = reqwest::Client::new();
    let put_res = client
        .put(&presign.upload_url)
        .header("content-type", "audio/wav")
        .body(payload_bytes.clone())
        .send()
        .await
        .unwrap();
    assert!(put_res.status().is_success());

    let mut hasher = Sha256::new();
    hasher.update(&payload_bytes);
    let sha256 = hex_encode(&hasher.finalize());

    // 3. Register narration -> queues job and returns 202 Accepted
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/narration"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "media_id": presign.media_id,
                        "storage_key": presign.storage_key,
                        "original_filename": "voice.wav",
                        "mime": "audio/wav",
                        "bytes": payload_bytes.len(),
                        "sha256": sha256
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg_res.status(), StatusCode::ACCEPTED);
    let reg_body = reg_res.into_body().collect().await.unwrap().to_bytes();
    let registered: RegisterNarrationResponse = serde_json::from_slice(&reg_body).unwrap();
    assert_eq!(registered.job.queue, "audio");
    assert_eq!(registered.job.status, "queued");

    // 4. Poll job & verify cross-user IDOR isolation returns 404
    let job_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/jobs/{}", registered.job.id))
                .header(header::COOKIE, &owner.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(job_res.status(), StatusCode::OK);

    let idor_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/narration"))
                .header(header::COOKIE, &intruder.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idor_res.status(), StatusCode::NOT_FOUND);

    // 5. Simulate processed boundaries + subtitle cue and test PATCH endpoints
    for (idx, ms) in [(0, 0), (1, 3000), (2, 7000)] {
        pace_db::sqlx::query(
            "INSERT INTO boundaries (project_id, index, time_ms, source, confidence, flags) VALUES ($1, $2, $3, 'auto', 0.92, '{}')"
        )
        .bind(project_id)
        .bind(idx)
        .bind(ms)
        .execute(&pool)
        .await
        .unwrap();
    }
    let cue_id: uuid::Uuid = pace_db::sqlx::query_scalar(
        "INSERT INTO subtitle_cues (project_id, position, start_ms, end_ms, text) VALUES ($1, 0, 100, 2500, 'Original text') RETURNING id"
    )
    .bind(project_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let patch_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{project_id}/boundaries/1"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "time_ms": 3400 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch_b.status(), StatusCode::OK);
    let updated_b: BoundaryDto = serde_json::from_slice(&patch_b.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(updated_b.time_ms, 3400);
    assert_eq!(updated_b.source, "manual");

    let patch_s = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{project_id}/subtitles/{cue_id}"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "text": "Corrected subtitle text" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch_s.status(), StatusCode::OK);
    let updated_s: SubtitleCueDto = serde_json::from_slice(&patch_s.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(updated_s.edited);
    assert_eq!(updated_s.text, "Corrected subtitle text");

    // 6. GET /narration returns signed audio URL, boundaries, subtitle cues, and active job
    let state_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/narration"))
                .header(header::COOKIE, &owner.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(state_res.status(), StatusCode::OK);
    let nar_state: NarrationStateResponse = serde_json::from_slice(&state_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(nar_state.media_id, Some(presign.media_id));
    assert!(nar_state.audio_url.is_some());
    assert_eq!(nar_state.boundaries.len(), 3);
    assert_eq!(nar_state.subtitle_cues.len(), 1);

    // 7. DELETE /narration removes MinIO object, DB state, and reclaims storage quota
    let del_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/projects/{project_id}/narration"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_res.status(), StatusCode::NO_CONTENT);

    let user_row = pace_db::find_user_by_id(&pool, owner.user_id).await.unwrap().unwrap();
    assert_eq!(user_row.storage_used_bytes, 0);
}

#[tokio::test]
async fn test_timeline_get_and_boundary_invariants_and_batch_update() {
    let (app, pool) = setup_test_app().await;
    let owner = create_authenticated_user(&app, &pool).await;
    let intruder = create_authenticated_user(&app, &pool).await;
    let project_id = create_test_project(&app, &owner, "Timeline M6 Test").await;

    // 1. Seed script sections, boundaries (with flags on boundary 1), and narration duration
    let save_script_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{project_id}/script"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "raw_text": "Scene one.\n\nScene two.\n\nScene three.",
                        "sections": [
                            { "text": "Scene one." },
                            { "text": "Scene two." },
                            { "text": "Scene three." }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save_script_res.status(), StatusCode::OK);

    for (idx, ms, flags) in [
        (0, 0, vec![] as Vec<&str>),
        (1, 3000, vec!["Low confidence"]),
        (2, 6000, vec![]),
        (3, 10000, vec![]),
    ] {
        pace_db::sqlx::query(
            "INSERT INTO boundaries (project_id, index, time_ms, source, confidence, flags) VALUES ($1, $2, $3, 'auto', 0.42, $4)"
        )
        .bind(project_id)
        .bind(idx)
        .bind(ms)
        .bind(&flags)
        .execute(&pool)
        .await
        .unwrap();
    }

    // 2. GET /timeline returns scenes, boundaries, and flagged index [1]; IDOR blocked with 404
    let idor_timeline = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/timeline"))
                .header(header::COOKIE, &intruder.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idor_timeline.status(), StatusCode::NOT_FOUND);

    let tl_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/timeline"))
                .header(header::COOKIE, &owner.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tl_res.status(), StatusCode::OK);
    let tl_json: Value =
        serde_json::from_slice(&tl_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(tl_json["fps"], 30);
    assert_eq!(tl_json["scenes"].as_array().unwrap().len(), 3);
    assert_eq!(tl_json["flagged_boundary_indices"], json!([1]));

    // 3. Enforce 0.5s (500ms) min gap and locked start/end boundaries
    let too_close = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{project_id}/boundaries/1"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "time_ms": 400 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(too_close.status(), StatusCode::BAD_REQUEST);

    let locked_start = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{project_id}/boundaries/0"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "time_ms": 100 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(locked_start.status(), StatusCode::BAD_REQUEST);

    // 4. Manual edit clears resolved warning flags and preserves sibling cuts
    let fix_flag = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{project_id}/boundaries/1"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "time_ms": 3200 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fix_flag.status(), StatusCode::OK);
    let fixed_dto: BoundaryDto =
        serde_json::from_slice(&fix_flag.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(fixed_dto.time_ms, 3200);
    assert_eq!(fixed_dto.source, "manual");
    assert!(fixed_dto.flags.is_empty());

    // 5. Batch PATCH /boundaries rejects invalid overlap atomically and commits valid updates
    let bad_batch = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{project_id}/boundaries"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "boundaries": [
                            { "index": 1, "time_ms": 2800 },
                            { "index": 2, "time_ms": 3100 }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_batch.status(), StatusCode::BAD_REQUEST);

    let ok_batch = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{project_id}/boundaries"))
                .header(header::COOKIE, &owner.cookie)
                .header("x-pace-csrf", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "boundaries": [
                            { "index": 1, "time_ms": 2800 },
                            { "index": 2, "time_ms": 6500 }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_batch.status(), StatusCode::OK);
    let batch_list: Vec<BoundaryDto> =
        serde_json::from_slice(&ok_batch.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(batch_list[1].time_ms, 2800);
    assert_eq!(batch_list[2].time_ms, 6500);
    assert_eq!(batch_list[3].time_ms, 10000);
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}
