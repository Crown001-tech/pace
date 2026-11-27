use std::env;

pub const DEV_DEFAULT_SESSION_SECRET: &str =
    "local-development-session-secret-change-in-production-min-32-chars";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub pace_env: String,
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub session_secret: String,
    pub cors_origin: String,
    pub s3_endpoint: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub pexels_api_key: Option<String>,
    pub pixabay_api_key: Option<String>,
    pub cloudflare_account_id: Option<String>,
    pub cloudflare_api_token: Option<String>,
}

impl AppConfig {
    /// Parses configuration from environment variables and enforces production secret checks.
    pub fn from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        let pace_env = env::var("PACE_ENV").unwrap_or_else(|_| "development".to_string());
        let host = env::var("API_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("API_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .map_err(|e| format!("Invalid API_PORT: {e}"))?;

        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://pace:pace@localhost:5432/pace".to_string()
        });

        let raw_secret = env::var("SESSION_SECRET").ok();
        let session_secret = match raw_secret {
            Some(s) if !s.trim().is_empty() => s,
            _ if pace_env == "production" => {
                return Err(
                    "SESSION_SECRET must be explicitly set when PACE_ENV=production".to_string(),
                );
            }
            _ => DEV_DEFAULT_SESSION_SECRET.to_string(),
        };

        validate_session_secret(&pace_env, &session_secret)?;

        let cors_origin = env::var("CORS_ORIGIN").unwrap_or_else(|_| {
            "http://localhost:3000,http://127.0.0.1:3000".to_string()
        });

        let s3_endpoint =
            env::var("S3_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
        let s3_bucket = env::var("S3_BUCKET").unwrap_or_else(|_| "pace".to_string());
        let s3_region = env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let s3_access_key = env::var("S3_ACCESS_KEY").unwrap_or_else(|_| "pace".to_string());
        let s3_secret_key =
            env::var("S3_SECRET_KEY").unwrap_or_else(|_| "pace-secret".to_string());

        let opt_env = |name: &str| {
            env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let pexels_api_key = opt_env("PEXELS_API_KEY");
        let pixabay_api_key = opt_env("PIXABAY_API_KEY");
        let cloudflare_account_id = opt_env("CLOUDFLARE_ACCOUNT_ID");
        let cloudflare_api_token = opt_env("CLOUDFLARE_API_TOKEN");

        Ok(Self {
            pace_env,
            host,
            port,
            database_url,
            session_secret,
            cors_origin,
            s3_endpoint,
            s3_bucket,
            s3_region,
            s3_access_key,
            s3_secret_key,
            pexels_api_key,
            pixabay_api_key,
            cloudflare_account_id,
            cloudflare_api_token,
        })
    }

    /// Returns trimmed, non-empty allowed origins configured via `CORS_ORIGIN`.
    pub fn allowed_origins(&self) -> Vec<&str> {
        self.cors_origin
            .split(',')
            .map(|s| s.trim().trim_end_matches('/'))
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// Ensures production deployments never boot with short or well-known default secrets.
pub fn validate_session_secret(pace_env: &str, session_secret: &str) -> Result<(), String> {
    if session_secret.trim().len() < 32 {
        return Err("SESSION_SECRET must be at least 32 characters".to_string());
    }

    if pace_env == "production" && session_secret == DEV_DEFAULT_SESSION_SECRET {
        return Err(
            "SESSION_SECRET cannot use the default development value in production".to_string(),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_production_rejects_default_or_short_secret() {
        assert!(validate_session_secret("production", DEV_DEFAULT_SESSION_SECRET).is_err());
        assert!(validate_session_secret("production", "too-short").is_err());
        assert!(validate_session_secret(
            "production",
            "0123456789abcdef0123456789abcdef-prod-key"
        )
        .is_ok());
        assert!(validate_session_secret("development", DEV_DEFAULT_SESSION_SECRET).is_ok());
    }
}
