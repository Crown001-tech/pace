use std::env;

#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub database_url: String,
    pub s3_endpoint: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub poll_interval_ms: u64,
}

impl RenderConfig {
    pub fn from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://pace:pace@localhost:5432/pace".to_string()),
            s3_endpoint: env::var("S3_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:9000".to_string()),
            s3_bucket: env::var("S3_BUCKET").unwrap_or_else(|_| "pace".to_string()),
            s3_region: env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            s3_access_key: env::var("S3_ACCESS_KEY").unwrap_or_else(|_| "pace".to_string()),
            s3_secret_key: env::var("S3_SECRET_KEY")
                .unwrap_or_else(|_| "pace-secret".to_string()),
            poll_interval_ms: env::var("RENDER_POLL_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1500),
        })
    }
}
