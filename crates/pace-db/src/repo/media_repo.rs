use crate::models::{CreateMediaAsset, MediaAsset};
use crate::pool::DbError;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn find_by_project(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Vec<MediaAsset>, sqlx::Error> {
    sqlx::query_as::<_, MediaAsset>(
        r#"
        SELECT id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
               original_filename, mime, bytes, sha256, width, height, duration_ms,
               source, prompt, status, probe, created_at, updated_at
        FROM media_assets
        WHERE project_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}

pub async fn find_by_sha256(
    pool: &PgPool,
    project_id: Uuid,
    sha256: &str,
    kind: &str,
) -> Result<Option<MediaAsset>, sqlx::Error> {
    sqlx::query_as::<_, MediaAsset>(
        r#"
        SELECT id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
               original_filename, mime, bytes, sha256, width, height, duration_ms,
               source, prompt, status, probe, created_at, updated_at
        FROM media_assets
        WHERE project_id = $1 AND sha256 = $2 AND kind = $3
        "#,
    )
    .bind(project_id)
    .bind(sha256)
    .bind(kind)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(
    pool: &PgPool,
    media_id: Uuid,
) -> Result<Option<MediaAsset>, sqlx::Error> {
    sqlx::query_as::<_, MediaAsset>(
        r#"
        SELECT id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
               original_filename, mime, bytes, sha256, width, height, duration_ms,
               source, prompt, status, probe, created_at, updated_at
        FROM media_assets
        WHERE id = $1
        "#,
    )
    .bind(media_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id_and_project(
    pool: &PgPool,
    media_id: Uuid,
    project_id: Uuid,
) -> Result<Option<MediaAsset>, sqlx::Error> {
    sqlx::query_as::<_, MediaAsset>(
        r#"
        SELECT id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
               original_filename, mime, bytes, sha256, width, height, duration_ms,
               source, prompt, status, probe, created_at, updated_at
        FROM media_assets
        WHERE id = $1 AND project_id = $2
        "#,
    )
    .bind(media_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

pub async fn count_by_project_and_kind(
    pool: &PgPool,
    project_id: Uuid,
    kind: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM media_assets WHERE project_id = $1 AND kind = $2")
        .bind(project_id)
        .bind(kind)
        .fetch_one(pool)
        .await
}

pub async fn create_media(
    pool: &PgPool,
    req: CreateMediaAsset,
) -> Result<MediaAsset, DbError> {
    create_media_with_id(pool, Uuid::now_v7(), req).await
}

pub async fn create_media_with_id(
    pool: &PgPool,
    media_id: Uuid,
    req: CreateMediaAsset,
) -> Result<MediaAsset, DbError> {
    let mut tx = pool.begin().await?;

    // Lock owner row to enforce quota atomically under concurrent uploads (STO-01).
    let (used_bytes, quota_bytes): (i64, i64) = sqlx::query_as(
        "SELECT storage_used_bytes, storage_quota_bytes FROM users WHERE id = $1 FOR UPDATE",
    )
    .bind(req.owner_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DbError::NotFound)?;

    if used_bytes.saturating_add(req.bytes) > quota_bytes {
        tx.rollback().await?;
        return Err(DbError::QuotaExceeded {
            current_bytes: used_bytes,
            max_bytes: quota_bytes,
        });
    }

    let asset = sqlx::query_as::<_, MediaAsset>(
        r#"
        INSERT INTO media_assets (
            id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
            original_filename, mime, bytes, sha256, width, height, duration_ms,
            source, prompt, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, 'ready')
        RETURNING id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
                  original_filename, mime, bytes, sha256, width, height, duration_ms,
                  source, prompt, status, probe, created_at, updated_at
        "#,
    )
    .bind(media_id)
    .bind(req.project_id)
    .bind(req.owner_id)
    .bind(req.kind)
    .bind(req.storage_key)
    .bind(req.preview_key)
    .bind(req.thumb_key)
    .bind(req.original_filename)
    .bind(req.mime)
    .bind(req.bytes)
    .bind(req.sha256)
    .bind(req.width)
    .bind(req.height)
    .bind(req.duration_ms)
    .bind(req.source.unwrap_or_else(|| "upload".to_string()))
    .bind(req.prompt)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE users
        SET storage_used_bytes = storage_used_bytes + $1
        WHERE id = $2
        "#,
    )
    .bind(req.bytes)
    .bind(req.owner_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(asset)
}

pub async fn delete_media(
    pool: &PgPool,
    project_id: Uuid,
    media_id: Uuid,
) -> Result<Option<MediaAsset>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let maybe_asset = sqlx::query_as::<_, MediaAsset>(
        r#"
        SELECT id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
               original_filename, mime, bytes, sha256, width, height, duration_ms,
               source, prompt, status, probe, created_at, updated_at
        FROM media_assets
        WHERE id = $1 AND project_id = $2
        FOR UPDATE
        "#,
    )
    .bind(media_id)
    .bind(project_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(asset) = maybe_asset {
        sqlx::query("DELETE FROM media_assets WHERE id = $1 AND project_id = $2")
            .bind(media_id)
            .bind(project_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            r#"
            UPDATE users
            SET storage_used_bytes = GREATEST(0, storage_used_bytes - $1)
            WHERE id = $2
            "#,
        )
        .bind(asset.bytes)
        .bind(asset.owner_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Some(asset))
    } else {
        tx.rollback().await?;
        Ok(None)
    }
}
