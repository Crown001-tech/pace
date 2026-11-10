use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub pace_env: String,
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub session_secret: String,
    pub cors_origin: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        // Load .env file if available, ignore error if missing (e.g. in production/Docker)
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

        let session_secret = env::var("SESSION_SECRET").unwrap_or_else(|_| {
            "local-development-session-secret-change-in-production-min-32-chars".to_string()
        });

        if pace_env == "production" && session_secret.len() < 32 {
            return Err("SESSION_SECRET must be at least 32 characters in production".to_string());
        }

        let cors_origin = env::var("CORS_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".to_string());

        Ok(Self {
            pace_env,
            host,
            port,
            database_url,
            session_secret,
            cors_origin,
        })
    }
}

