use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("Project version conflict: expected version {expected}, found {actual}")]
    VersionConflict { expected: i32, actual: i32 },

    #[error("Account locked due to 10 failed login attempts in the past 15 minutes")]
    AccountLocked,

    #[error("Record not found")]
    NotFound,
}

/// Creates a tuned PostgreSQL connection pool for PACE.
pub async fn create_pool(database_url: &str) -> Result<PgPool, DbError> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(60))
        .connect(database_url)
        .await?;

    Ok(pool)
}

/// Runs embedded database migrations.
pub async fn run_migrations(pool: &PgPool) -> Result<(), DbError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
