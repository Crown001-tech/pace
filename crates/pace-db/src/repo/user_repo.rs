use crate::models::{CreateUser, User};
use crate::pool::DbError;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn create_user(pool: &PgPool, input: CreateUser) -> Result<User, DbError> {
    let quota = input.storage_quota_bytes.unwrap_or(2 * 1024 * 1024 * 1024);
    let user = sqlx::query_as!(
        User,
        r#"
        INSERT INTO users (email, name, password_hash, role, must_change_password, storage_quota_bytes)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, email as "email!", name, password_hash, role, must_change_password, is_active,
                  storage_quota_bytes, storage_used_bytes, daily_1080p_limit, created_at, updated_at
        "#,
        input.email,
        input.name,
        input.password_hash,
        input.role,
        input.must_change_password,
        quota
    )
    .fetch_one(pool)
    .await?;

    Ok(user)
}

pub async fn find_user_by_email(pool: &PgPool, email: &str) -> Result<Option<User>, DbError> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT id, email as "email!", name, password_hash, role, must_change_password, is_active,
               storage_quota_bytes, storage_used_bytes, daily_1080p_limit, created_at, updated_at
        FROM users
        WHERE email = ($1::text)::citext
        "#,
        email
    )
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

pub async fn find_user_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, DbError> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT id, email as "email!", name, password_hash, role, must_change_password, is_active,
               storage_quota_bytes, storage_used_bytes, daily_1080p_limit, created_at, updated_at
        FROM users
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

pub async fn update_user_password(
    pool: &PgPool,
    user_id: Uuid,
    new_password_hash: &str,
) -> Result<(), DbError> {
    let rows_affected = sqlx::query!(
        r#"
        UPDATE users
        SET password_hash = $2, must_change_password = false
        WHERE id = $1
        "#,
        user_id,
        new_password_hash
    )
    .execute(pool)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(DbError::NotFound);
    }

    Ok(())
}

pub async fn reset_user_password(
    pool: &PgPool,
    user_id: Uuid,
    temporary_password_hash: &str,
) -> Result<(), DbError> {
    let rows_affected = sqlx::query!(
        r#"
        UPDATE users
        SET password_hash = $2, must_change_password = true
        WHERE id = $1
        "#,
        user_id,
        temporary_password_hash
    )
    .execute(pool)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(DbError::NotFound);
    }

    Ok(())
}

pub async fn set_user_active(
    pool: &PgPool,
    user_id: Uuid,
    is_active: bool,
) -> Result<(), DbError> {
    let rows_affected = sqlx::query!(
        r#"
        UPDATE users
        SET is_active = $2
        WHERE id = $1
        "#,
        user_id,
        is_active
    )
    .execute(pool)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(DbError::NotFound);
    }

    Ok(())
}

pub async fn list_users(pool: &PgPool) -> Result<Vec<User>, DbError> {
    let users = sqlx::query_as!(
        User,
        r#"
        SELECT id, email as "email!", name, password_hash, role, must_change_password, is_active,
               storage_quota_bytes, storage_used_bytes, daily_1080p_limit, created_at, updated_at
        FROM users
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(users)
}
