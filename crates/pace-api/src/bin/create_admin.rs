use pace_api::{auth::passwords::hash_password, config::AppConfig};
use pace_db::{create_pool, create_user, find_user_by_email, run_migrations, CreateUser};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env().unwrap_or(AppConfig {
        pace_env: "development".to_string(),
        host: "127.0.0.1".to_string(),
        port: 8080,
        database_url: env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://pace:pace@localhost:5432/pace".to_string()),
        session_secret: "local-development-session-secret-change-in-production-min-32-chars"
            .to_string(),
        cors_origin: "http://localhost:3000".to_string(),
    });

    let pool = create_pool(&config.database_url).await?;
    run_migrations(&pool).await?;

    let email = env::args()
        .nth(1)
        .unwrap_or_else(|| "admin@pace.local".to_string());
    let password = env::args()
        .nth(2)
        .unwrap_or_else(|| "AdminPassword123!".to_string());
    let name = env::args()
        .nth(3)
        .unwrap_or_else(|| "System Administrator".to_string());

    if let Some(existing) = find_user_by_email(&pool, &email).await? {
        println!(
            "User with email '{}' already exists (ID: {})",
            email, existing.id
        );
        return Ok(());
    }

    let password_hash = hash_password(&password)?;
    let user = create_user(
        &pool,
        CreateUser {
            email: email.clone(),
            name,
            password_hash,
            role: "admin".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(100 * 1024 * 1024 * 1024), // 100 GB for admin
        },
    )
    .await?;

    println!("Admin user created successfully!");
    println!("  ID:    {}", user.id);
    println!("  Email: {}", user.email);
    println!("  Role:  {}", user.role);

    Ok(())
}
