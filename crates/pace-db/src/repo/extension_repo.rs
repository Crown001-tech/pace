use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    models::{ExtensionPairingCode, ExtensionToken},
    DbError,
};

pub async fn create_extension_pairing_code(
    pool: &PgPool,
    user_id: Uuid,
    code_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<ExtensionPairingCode, DbError> {
    let row = sqlx::query_as::<_, ExtensionPairingCode>(
        r#"
        INSERT INTO extension_pairing_codes (user_id, code_hash, expires_at)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(code_hash)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Atomically consumes a valid, unexpired, unused pairing code.
pub async fn consume_extension_pairing_code(
    pool: &PgPool,
    code_hash: &str,
) -> Result<Option<ExtensionPairingCode>, DbError> {
    let row = sqlx::query_as::<_, ExtensionPairingCode>(
        r#"
        UPDATE extension_pairing_codes
        SET used_at = now()
        WHERE code_hash = $1
          AND used_at IS NULL
          AND expires_at > now()
        RETURNING *
        "#,
    )
    .bind(code_hash)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn create_extension_token(
    pool: &PgPool,
    user_id: Uuid,
    device_name: &str,
    token_hash: &str,
) -> Result<ExtensionToken, DbError> {
    let row = sqlx::query_as::<_, ExtensionToken>(
        r#"
        INSERT INTO extension_tokens (user_id, device_name, token_hash)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(device_name)
    .bind(token_hash)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn find_active_extension_token_by_hash(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<ExtensionToken>, DbError> {
    let row = sqlx::query_as::<_, ExtensionToken>(
        r#"
        SELECT * FROM extension_tokens
        WHERE token_hash = $1
          AND revoked_at IS NULL
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn list_active_extension_tokens(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<ExtensionToken>, DbError> {
    let rows = sqlx::query_as::<_, ExtensionToken>(
        r#"
        SELECT * FROM extension_tokens
        WHERE user_id = $1
          AND revoked_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn revoke_extension_token(
    pool: &PgPool,
    token_id: Uuid,
    user_id: Uuid,
) -> Result<bool, DbError> {
    let res = sqlx::query(
        r#"
        UPDATE extension_tokens
        SET revoked_at = now()
        WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL
        "#,
    )
    .bind(token_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(res.rows_affected() > 0)
}

pub async fn touch_extension_token_project(
    pool: &PgPool,
    token_id: Uuid,
    project_id: Option<Uuid>,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE extension_tokens
        SET last_used_at = now(),
            last_project_id = COALESCE($2, last_project_id)
        WHERE id = $1
        "#,
    )
    .bind(token_id)
    .bind(project_id)
    .execute(pool)
    .await?;

    Ok(())
}
