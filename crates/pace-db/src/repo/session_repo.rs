use crate::models::{CreateSession, Session};
use crate::pool::DbError;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn create_session(pool: &PgPool, input: CreateSession) -> Result<Session, DbError> {
    let session = sqlx::query_as!(
        Session,
        r#"
        INSERT INTO sessions (user_id, token_hash, expires_at, user_agent)
        VALUES ($1, $2, $3, $4)
        RETURNING id, user_id, token_hash, created_at, last_seen_at, expires_at, user_agent
        "#,
        input.user_id,
        input.token_hash,
        input.expires_at,
        input.user_agent
    )
    .fetch_one(pool)
    .await?;

    Ok(session)
}

pub async fn find_active_session_by_token_hash(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<Session>, DbError> {
    let now = Utc::now();
    let session = sqlx::query_as!(
        Session,
        r#"
        SELECT id, user_id, token_hash, created_at, last_seen_at, expires_at, user_agent
        FROM sessions
        WHERE token_hash = $1 AND expires_at > $2
        "#,
        token_hash,
        now
    )
    .fetch_optional(pool)
    .await?;

    Ok(session)
}

/// Extends an active session's rolling 30-day expiry. Never resurrects an already-expired session.
pub async fn touch_session(pool: &PgPool, session_id: Uuid) -> Result<(), DbError> {
    sqlx::query!(
        r#"
        UPDATE sessions
        SET last_seen_at = now(),
            expires_at = now() + interval '30 days'
        WHERE id = $1 AND expires_at > now()
        "#,
        session_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_session(pool: &PgPool, session_id: Uuid) -> Result<(), DbError> {
    sqlx::query!(
        r#"
        DELETE FROM sessions
        WHERE id = $1
        "#,
        session_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Removes expired sessions to prevent unbounded session table growth.
pub async fn cleanup_expired_sessions(pool: &PgPool) -> Result<u64, DbError> {
    let count = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?
        .rows_affected();

    Ok(count)
}

pub async fn delete_all_user_sessions(pool: &PgPool, user_id: Uuid) -> Result<u64, DbError> {
    let count = sqlx::query!(
        r#"
        DELETE FROM sessions
        WHERE user_id = $1
        "#,
        user_id
    )
    .execute(pool)
    .await?
    .rows_affected();

    Ok(count)
}

/// Terminates every active session for `user_id` except `keep_session_id` (used on password change).
pub async fn delete_other_user_sessions(
    pool: &PgPool,
    user_id: Uuid,
    keep_session_id: Uuid,
) -> Result<u64, DbError> {
    let count = sqlx::query!(
        r#"
        DELETE FROM sessions
        WHERE user_id = $1 AND id <> $2
        "#,
        user_id,
        keep_session_id
    )
    .execute(pool)
    .await?
    .rows_affected();

    Ok(count)
}
