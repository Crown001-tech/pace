use crate::models::{CreateExport, ExportRecord, RenderSegmentRecord};
use crate::pool::DbError;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn has_active_export(pool: &PgPool, project_id: Uuid) -> Result<bool, DbError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM exports WHERE project_id = $1 AND status IN ('queued', 'running'))",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn create_export(pool: &PgPool, input: CreateExport) -> Result<ExportRecord, DbError> {
    let row = sqlx::query_as::<_, ExportRecord>(
        r#"
        INSERT INTO exports (
            id, project_id, user_id, job_id, spec_hash, resolution,
            width, height, fps, duration_ms, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'queued')
        RETURNING id, project_id, user_id, job_id, spec_hash, resolution, width, height, fps,
                  duration_ms, status, storage_key, size_bytes, validation, error_code,
                  error_message, created_at, updated_at, finished_at
        "#,
    )
    .bind(input.id)
    .bind(input.project_id)
    .bind(input.user_id)
    .bind(input.job_id)
    .bind(input.spec_hash)
    .bind(input.resolution)
    .bind(input.width)
    .bind(input.height)
    .bind(input.fps)
    .bind(input.duration_ms)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn list_exports_by_project(
    pool: &PgPool,
    project_id: Uuid,
    owner_id: Uuid,
) -> Result<Vec<ExportRecord>, DbError> {
    let rows = sqlx::query_as::<_, ExportRecord>(
        r#"
        SELECT id, project_id, user_id, job_id, spec_hash, resolution, width, height, fps,
               duration_ms, status, storage_key, size_bytes, validation, error_code,
               error_message, created_at, updated_at, finished_at
        FROM exports
        WHERE project_id = $1 AND user_id = $2
        ORDER BY created_at DESC
        LIMIT 20
        "#,
    )
    .bind(project_id)
    .bind(owner_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn find_export_by_id_and_project(
    pool: &PgPool,
    export_id: Uuid,
    project_id: Uuid,
    owner_id: Uuid,
) -> Result<Option<ExportRecord>, DbError> {
    let row = sqlx::query_as::<_, ExportRecord>(
        r#"
        SELECT id, project_id, user_id, job_id, spec_hash, resolution, width, height, fps,
               duration_ms, status, storage_key, size_bytes, validation, error_code,
               error_message, created_at, updated_at, finished_at
        FROM exports
        WHERE id = $1 AND project_id = $2 AND user_id = $3
        "#,
    )
    .bind(export_id)
    .bind(project_id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn mark_export_running(
    pool: &PgPool,
    export_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE exports
        SET status = 'running',
            error_code = NULL,
            error_message = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(export_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_export_succeeded(
    pool: &PgPool,
    export_id: Uuid,
    storage_key: &str,
    size_bytes: i64,
    validation: serde_json::Value,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE exports
        SET status = 'succeeded',
            storage_key = $2,
            size_bytes = $3,
            validation = $4,
            error_code = NULL,
            error_message = NULL,
            finished_at = now(),
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(export_id)
    .bind(storage_key)
    .bind(size_bytes)
    .bind(validation)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_export_failed(
    pool: &PgPool,
    export_id: Uuid,
    error_code: &str,
    error_message: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE exports
        SET status = 'failed',
            error_code = $2,
            error_message = $3,
            finished_at = now(),
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(export_id)
    .bind(error_code)
    .bind(error_message)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn reset_export_for_retry(
    pool: &PgPool,
    export_id: Uuid,
    new_job_id: Uuid,
) -> Result<ExportRecord, DbError> {
    let row = sqlx::query_as::<_, ExportRecord>(
        r#"
        UPDATE exports
        SET status = 'queued',
            job_id = $2,
            error_code = NULL,
            error_message = NULL,
            finished_at = NULL,
            updated_at = now()
        WHERE id = $1
        RETURNING id, project_id, user_id, job_id, spec_hash, resolution, width, height, fps,
                  duration_ms, status, storage_key, size_bytes, validation, error_code,
                  error_message, created_at, updated_at, finished_at
        "#,
    )
    .bind(export_id)
    .bind(new_job_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn attach_export_job(
    pool: &PgPool,
    export_id: Uuid,
    job_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query("UPDATE exports SET job_id = $2, updated_at = now() WHERE id = $1")
        .bind(export_id)
        .bind(job_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn upsert_render_segment(
    pool: &PgPool,
    cache_key: &str,
    project_id: Uuid,
    storage_key: &str,
    frame_count: i32,
    size_bytes: i64,
) -> Result<RenderSegmentRecord, DbError> {
    let row = sqlx::query_as::<_, RenderSegmentRecord>(
        r#"
        INSERT INTO render_segments (cache_key, project_id, storage_key, frame_count, size_bytes)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (cache_key)
        DO UPDATE SET
            storage_key = EXCLUDED.storage_key,
            frame_count = EXCLUDED.frame_count,
            size_bytes = EXCLUDED.size_bytes
        RETURNING cache_key, project_id, storage_key, frame_count, size_bytes, created_at
        "#,
    )
    .bind(cache_key)
    .bind(project_id)
    .bind(storage_key)
    .bind(frame_count)
    .bind(size_bytes)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn find_render_segment(
    pool: &PgPool,
    cache_key: &str,
) -> Result<Option<RenderSegmentRecord>, DbError> {
    let row = sqlx::query_as::<_, RenderSegmentRecord>(
        r#"
        SELECT cache_key, project_id, storage_key, frame_count, size_bytes, created_at
        FROM render_segments
        WHERE cache_key = $1
        "#,
    )
    .bind(cache_key)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}
