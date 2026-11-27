use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use pace_api::{
    app::{create_app, AppState},
    config::AppConfig,
    routes::{
        music::{MusicStateResponse, SubtitleStyleDto, SubtitlesStateResponse},
        narration::SubtitleCueDto,
        timeline::TimelineStateResponse,
    },
};
use pace_db::{create_pool, create_user, run_migrations, CreateUser};
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
    let email = format!("m9_api_{}@example.com", uuid::Uuid::now_v7());
    let password = "SecretPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    let user = create_user(
        pool,
        CreateUser {
            email: email.clone(),
            name: "M9 Music & Subtitles Tester".to_string(),
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
async fn music_presign_settings_and_subtitle_split_merge_style_roundtrip() {
    let (app, pool) = setup_test_app().await;
    let user = create_authenticated_user(&app, &pool).await;

    // 1. Create a 16:9 project
    let create_proj = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/projects")
                .header(header::COOKIE, &user.cookie)
                .header("X-PACE-CSRF", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "M9 Music & Subtitles Project",
                        "aspect_ratio": "16:9",
                        "fps": 30
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_proj.status(), StatusCode::CREATED);
    let proj_body: Value = serde_json::from_slice(
        &create_proj.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let project_id = uuid::Uuid::parse_str(proj_body["id"].as_str().unwrap()).unwrap();

    // 2. Presign background music upload
    let presign_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/music/presign"))
                .header(header::COOKIE, &user.cookie)
                .header("X-PACE-CSRF", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "filename": "ambient-score.mp3",
                        "content_type": "audio/mpeg",
                        "bytes": 102400
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(presign_res.status(), StatusCode::OK);
    let presign_json: Value = serde_json::from_slice(
        &presign_res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert!(presign_json["storage_key"]
        .as_str()
        .unwrap()
        .contains("/music/"));

    // 3. Patch music volume, ducking, loop crossfade, and fade-in/out settings
    let patch_music = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{project_id}/music"))
                .header(header::COOKIE, &user.cookie)
                .header("X-PACE-CSRF", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "volume_db": -15.5,
                        "ducking_enabled": true,
                        "duck_db": -12.0,
                        "loop_enabled": true,
                        "crossfade_ms": 800,
                        "fade_in_ms": 1200,
                        "fade_out_ms": 1800
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch_music.status(), StatusCode::OK);
    let music_state: MusicStateResponse = serde_json::from_slice(
        &patch_music.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(music_state.settings.volume_db, -15.5);
    assert_eq!(music_state.settings.duck_db, -12.0);
    assert_eq!(music_state.settings.crossfade_ms, 800);
    assert_eq!(music_state.settings.fade_in_ms, 1200);
    assert_eq!(music_state.settings.fade_out_ms, 1800);

    // 4. Seed two initial subtitle cues
    let seeded_cues = pace_db::replace_project_subtitle_cues(
        &pool,
        project_id,
        vec![
            (
                None,
                0,
                4_000,
                "Every great structure begins as an empty horizon and a patient blueprint."
                    .to_string(),
                false,
            ),
            (
                None,
                4_000,
                7_500,
                "At dawn the skyline changes one steel girder at a time.".to_string(),
                false,
            ),
        ],
    )
    .await
    .unwrap();
    assert_eq!(seeded_cues.len(), 2);

    // 5. Split cue 0 into two consecutive cues
    let split_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/projects/{project_id}/subtitles/{}/split",
                    seeded_cues[0].id
                ))
                .header(header::COOKIE, &user.cookie)
                .header("X-PACE-CSRF", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(split_res.status(), StatusCode::OK);
    let after_split: Vec<SubtitleCueDto> =
        serde_json::from_slice(&split_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(after_split.len(), 3);
    assert_eq!(after_split[0].start_ms, 0);
    assert_eq!(after_split[0].end_ms, after_split[1].start_ms);
    assert_eq!(after_split[1].end_ms, 4_000);

    // 6. Merge the two split cues back into one
    let merge_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/subtitles/merge"))
                .header(header::COOKIE, &user.cookie)
                .header("X-PACE-CSRF", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "first_cue_id": after_split[0].id,
                        "second_cue_id": after_split[1].id
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(merge_res.status(), StatusCode::OK);
    let after_merge: Vec<SubtitleCueDto> =
        serde_json::from_slice(&merge_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(after_merge.len(), 2);
    assert_eq!(after_merge[0].start_ms, 0);
    assert_eq!(after_merge[0].end_ms, 4_000);

    // 7. Update subtitle burn-in style
    let style_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{project_id}/subtitles/style"))
                .header(header::COOKIE, &user.cookie)
                .header("X-PACE-CSRF", &user.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "enabled": true,
                        "font": "mono",
                        "size_pct": 115,
                        "position": "top",
                        "text_color": "#FACC15",
                        "outline_width_px": 3,
                        "bg_opacity_pct": 80
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(style_res.status(), StatusCode::OK);
    let updated_style: SubtitleStyleDto =
        serde_json::from_slice(&style_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(updated_style.font, "mono");
    assert_eq!(updated_style.size_pct, 115);
    assert_eq!(updated_style.position, "top");
    assert_eq!(updated_style.text_color, "#FACC15");

    // 8. GET /subtitles returns wrapped lines, rule checks, and updated style
    let subs_get = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/subtitles"))
                .header(header::COOKIE, &user.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(subs_get.status(), StatusCode::OK);
    let subs_state: SubtitlesStateResponse =
        serde_json::from_slice(&subs_get.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(subs_state.max_chars_per_line, 42);
    assert_eq!(subs_state.style.font, "mono");
    assert_eq!(subs_state.cues.len(), 2);
    assert!(subs_state.cues[0].lines.len() <= 2);

    // 9. GET /timeline includes both subtitle_style and music_settings
    let tl_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/timeline"))
                .header(header::COOKIE, &user.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tl_res.status(), StatusCode::OK);
    let tl_state: TimelineStateResponse =
        serde_json::from_slice(&tl_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(tl_state.subtitle_style.font, "mono");
    assert_eq!(tl_state.music_settings.volume_db, -15.5);

    // Clean up test user
    let _ = pace_db::sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user.user_id)
        .execute(&pool)
        .await;
}
