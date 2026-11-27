use std::net::SocketAddr;
use pace_api::{
    app::{create_app, AppState},
    config::AppConfig,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize structured logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pace_api=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. Load configuration from environment
    let config = AppConfig::from_env().map_err(|e| format!("Configuration error: {e}"))?;
    tracing::info!(
        env = %config.pace_env,
        host = %config.host,
        port = %config.port,
        "Starting PACE API server"
    );

    // 3. Connect to PostgreSQL
    let pool = pace_db::create_pool(&config.database_url).await?;
    tracing::info!("Connected to PostgreSQL, checking schema migrations...");
    pace_db::run_migrations(&pool).await?;
    tracing::info!("Database schema migrations up to date");

    // 4. Build Axum application router
    let state = AppState::new(pool, config.clone());
    let app = create_app(state);

    // 5. Bind TCP listener
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("PACE API listening on http://{}", addr);

    // 6. Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("PACE API server stopped cleanly");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C signal handler");
    tracing::info!("Shutdown signal received, shutting down gracefully...");
}
