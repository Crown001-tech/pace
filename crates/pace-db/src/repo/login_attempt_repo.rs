use crate::pool::DbError;
use sqlx::PgPool;

/// Records a login attempt (successful or failed).
pub async fn record_login_attempt(
    pool: &PgPool,
    email: &str,
    success: bool,
) -> Result<(), DbError> {
    sqlx::query!(
        r#"
        INSERT INTO login_attempts (email, success)
        VALUES ($1, $2)
        "#,
        email,
        success
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Checks if an email is locked out (`AUTH-01`).
///
/// Rule: 10 failed attempts within the past 15 minutes lock that email for 15 minutes.
pub async fn check_lockout(pool: &PgPool, email: &str) -> Result<(), DbError> {
    let failed_count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)
        FROM login_attempts
        WHERE email = ($1::text)::citext
          AND success = false
          AND attempted_at >= (now() - interval '15 minutes')
        "#,
        email
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(0);

    if failed_count >= 10 {
        return Err(DbError::AccountLocked);
    }

    Ok(())
}
