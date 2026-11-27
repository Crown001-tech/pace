use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
};
use http_body_util::BodyExt;
use pace_api::{
    app::{create_app, AppState},
    config::AppConfig,
    error::ApiError,
};
use serde_json::Value;
use tower::ServiceExt;

#[test]
fn test_api_error_into_response() {
    let err = ApiError::NotFound("Project not found".into());
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
}

#[tokio::test]
async fn test_healthz_endpoint_with_database() {
    let config = AppConfig {
        pace_env: "test".to_string(),
        host: "127.0.0.1".to_string(),
        port: 8080,
        database_url: std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://pace:pace@localhost:5432/pace".to_string()),
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

    // If database is available, test both healthz and readyz
    if let Ok(pool) = pace_db::create_pool(&config.database_url).await {
        let app = create_app(AppState::new(pool, config.clone()));

        // 1. Test /healthz
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key("x-request-id"));

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");

        // 2. Test /readyz
        let ready_resp = app
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(ready_resp.status(), StatusCode::OK);
        let ready_body = ready_resp.into_body().collect().await.unwrap().to_bytes();
        let ready_json: Value = serde_json::from_slice(&ready_body).unwrap();
        assert_eq!(ready_json["status"], "ok");
        assert_eq!(ready_json["database"], "connected");
    }
}
