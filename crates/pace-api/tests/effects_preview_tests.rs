use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use pace_api::{
    app::{create_app, AppState},
    config::AppConfig,
    routes::{
        assignments::AssignmentResponse,
        effects::{RenderSpecResponse, RerollEffectsResponse},
        timeline::TimelineStateResponse,
    },
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
    let email = format!("efx_api_{}@example.com", uuid::Uuid::now_v7());
    let password = "SecretPassword123!";
    let password_hash = pace_api::auth::passwords::hash_password(password).unwrap();

    let user = create_user(
        pool,
        CreateUser {
            email: email.clone(),
            name: "Effects API User".to_string(),
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

async fn seed_project_with_scenes(
    app: &axum::Router,
    pool: &pace_db::DbPool,
    session: &TestSession,
) -> (uuid::Uuid, Vec<uuid::Uuid>) {
    let create_proj = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/projects")
                .header(header::COOKIE, &session.cookie)
                .header("X-PACE-CSRF", &session.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "M7 Effects Test Project",
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

    let save_script = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/projects/{project_id}/script"))
                .header(header::COOKIE, &session.cookie)
                .header("X-PACE-CSRF", &session.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "sections": [
                            { "position": 0, "text": "Opening wide shot across the city skyline." },
                            { "position": 1, "text": "Closer study of architectural steelwork." }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save_script.status(), StatusCode::OK);
    let script_body: Value = serde_json::from_slice(
        &save_script.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let section_ids: Vec<uuid::Uuid> = script_body["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| uuid::Uuid::parse_str(s["id"].as_str().unwrap()).unwrap())
        .collect();

    for idx in 0..2 {
        let key = format!("users/{}/projects/{}/img-{idx}.jpg", session.user_id, project_id);
        let preview_key = format!("users/{}/projects/{}/preview-{idx}.webp", session.user_id, project_id);
        create_media(
            pool,
            CreateMediaAsset {
                project_id,
                owner_id: session.user_id,
                kind: "image".to_string(),
                storage_key: key,
                preview_key: Some(preview_key),
                thumb_key: None,
                original_filename: format!("scene-{idx}.jpg"),
                mime: "image/jpeg".to_string(),
                bytes: 4096,
                sha256: format!("{idx:064x}"),
                width: Some(2400),
                height: Some(1600),
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
                .header(header::COOKIE, &session.cookie)
                .header("X-PACE-CSRF", &session.csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(auto_assign.status(), StatusCode::OK);

    // Seed explicit boundaries (0ms, 5000ms, 10000ms) for cut/transition testing
    for (idx, time_ms) in [(0_i32, 0_i32), (1, 5000), (2, 10000)] {
        pace_db::sqlx::query(
            r#"
            INSERT INTO boundaries (project_id, index, time_ms, source, confidence, flags)
            VALUES ($1, $2, $3, 'auto', 0.99, '{}')
            "#,
        )
        .bind(project_id)
        .bind(idx)
        .bind(time_ms)
        .execute(pool)
        .await
        .unwrap();
    }

    (project_id, section_ids)
}

#[tokio::test]
async fn render_spec_is_deterministic_and_returns_signed_assets() {
    let (app, pool) = setup_test_app().await;
    let user = create_authenticated_user(&app, &pool).await;
    let (project_id, _) = seed_project_with_scenes(&app, &pool, &user).await;

    let res1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/render-spec"))
                .header(header::COOKIE, &user.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    let body1: RenderSpecResponse =
        serde_json::from_slice(&res1.into_body().collect().await.unwrap().to_bytes()).unwrap();

    let res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/render-spec"))
                .header(header::COOKIE, &user.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let body2: RenderSpecResponse =
        serde_json::from_slice(&res2.into_body().collect().await.unwrap().to_bytes()).unwrap();

    assert_eq!(body1.spec_hash.len(), 64);
    assert_eq!(body1.spec_hash, body2.spec_hash);
    assert_eq!(body1.total_frames, 300);
    assert_eq!(body1.scene_assets.len(), 2);
    assert!(body1.scene_assets[0].preview_url.is_some());
    assert!(body1.scene_assets[1].preview_url.is_some());
}

#[tokio::test]
async fn scene_effect_override_and_reroll_behave_accurately_and_preserve_tenant_isolation() {
    let (app, pool) = setup_test_app().await;
    let owner = create_authenticated_user(&app, &pool).await;
    let other_user = create_authenticated_user(&app, &pool).await;
    let (project_id, section_ids) = seed_project_with_scenes(&app, &pool, &owner).await;

    // Cross-tenant isolation check -> 404
    let unauthorized_spec = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/render-spec"))
                .header(header::COOKIE, &other_user.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized_spec.status(), StatusCode::NOT_FOUND);

    // Capture initial spec hash
    let initial_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/render-spec"))
                .header(header::COOKIE, &owner.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(initial_res.status(), StatusCode::OK);
    let initial_spec: RenderSpecResponse =
        serde_json::from_slice(&initial_res.into_body().collect().await.unwrap().to_bytes()).unwrap();

    // Override Scene 0 manually
    let patch_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!(
                    "/api/v1/projects/{project_id}/assignments/{}/effect",
                    section_ids[0]
                ))
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "manual_preset": "zoom_in_pan_right",
                        "intensity": "strong",
                        "easing": "steady",
                        "manual_shake": true,
                        "fit_mode": "fit_blur",
                        "focal_x": 0.25,
                        "focal_y": 0.75
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch_res.status(), StatusCode::OK);
    let updated_assign: AssignmentResponse =
        serde_json::from_slice(&patch_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(updated_assign.effect_overridden);
    assert_eq!(
        updated_assign.effect.manual_preset.as_deref(),
        Some("zoom_in_pan_right")
    );
    assert_eq!(updated_assign.effect.intensity, "strong");
    assert_eq!(updated_assign.effect.easing, "steady");
    assert_eq!(updated_assign.effect.manual_shake, Some(true));
    assert_eq!(updated_assign.fit_mode, "fit_blur");

    // Re-roll project seed -> Scene 0 override remains untouched, seed changes
    let reroll_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/projects/{project_id}/effects/reroll"))
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reroll_res.status(), StatusCode::OK);
    let rerolled: RerollEffectsResponse =
        serde_json::from_slice(&reroll_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_ne!(rerolled.effect_seed, initial_spec.effect_seed);
    assert_ne!(rerolled.spec_hash, initial_spec.spec_hash);

    // Update interior boundary transition to crossfade
    let patch_trans = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/v1/projects/{project_id}/boundaries/1"))
                .header(header::COOKIE, &owner.cookie)
                .header("X-PACE-CSRF", &owner.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "time_ms": 5000,
                        "transition": { "kind": "crossfade", "duration_ms": 800 }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch_trans.status(), StatusCode::OK);

    // Timeline reflects resolved preset, override flag, and new seed
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
    let timeline: TimelineStateResponse =
        serde_json::from_slice(&tl_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(timeline.effect_seed, rerolled.effect_seed);
    assert!(timeline.scenes[0].effect_overridden);
    assert_eq!(timeline.scenes[0].resolved_preset, "zoom_in_pan_right");
    assert!(timeline.scenes[0].resolved_has_shake);
    assert!(!timeline.scenes[1].effect_overridden);

    // After transition change, render-spec compiles transition window and updates hash
    let after_spec_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/projects/{project_id}/render-spec"))
                .header(header::COOKIE, &owner.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after_spec_res.status(), StatusCode::OK);
    let after_spec: RenderSpecResponse =
        serde_json::from_slice(&after_spec_res.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_ne!(after_spec.spec_hash, rerolled.spec_hash);
    assert_eq!(after_spec.spec.transitions.len(), 1);
}
