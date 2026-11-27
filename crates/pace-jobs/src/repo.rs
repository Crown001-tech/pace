use crate::models::{EnqueueJob, JobQueue, JobRecord};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),
}

/// Inserts a new job into the PostgreSQL queue.
pub async fn enqueue_job(pool: &PgPool, req: EnqueueJob) -> Result<JobRecord, JobError> {
    let job = sqlx::query_as::<_, JobRecord>(
        r#"
        INSERT INTO jobs (queue, type, project_id, user_id, priority, payload, max_attempts)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, queue, type, project_id, user_id, status, priority, payload, result,
                  progress, stage, attempts, max_attempts, error_code, error_message,
                  locked_by, heartbeat_at, run_after, started_at, finished_at, created_at, updated_at
        "#,
    )
    .bind(req.queue.as_str())
    .bind(req.job_type)
    .bind(req.project_id)
    .bind(req.user_id)
    .bind(req.priority)
    .bind(req.payload)
    .bind(req.max_attempts.max(1))
    .fetch_one(pool)
    .await?;

    Ok(job)
}

/// Atomically claims the next runnable job from `queue` using `FOR UPDATE SKIP LOCKED`.
/// Also reclaims stale running jobs whose heartbeat stopped for over 120 seconds (TDD 8.2).
pub async fn claim_next_job(
    pool: &PgPool,
    queue: JobQueue,
    worker_id: &str,
) -> Result<Option<JobRecord>, JobError> {
    let job = sqlx::query_as::<_, JobRecord>(
        r#"
        UPDATE jobs
        SET status = 'running',
            locked_by = $2,
            started_at = COALESCE(started_at, now()),
            heartbeat_at = now(),
            attempts = attempts + 1,
            updated_at = now()
        WHERE id = (
            SELECT id
            FROM jobs
            WHERE queue = $1
              AND (
                    (status = 'queued' AND run_after <= now())
                 OR (status = 'running' AND heartbeat_at < now() - interval '120 seconds')
              )
            ORDER BY priority DESC, created_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        RETURNING id, queue, type, project_id, user_id, status, priority, payload, result,
                  progress, stage, attempts, max_attempts, error_code, error_message,
                  locked_by, heartbeat_at, run_after, started_at, finished_at, created_at, updated_at
        "#,
    )
    .bind(queue.as_str())
    .bind(worker_id)
    .fetch_optional(pool)
    .await?;

    Ok(job)
}

/// Updates heartbeat timestamp, progress (0..100), and stage while a job is running.
pub async fn heartbeat_job(
    pool: &PgPool,
    job_id: Uuid,
    worker_id: &str,
    progress: i32,
    stage: Option<&str>,
) -> Result<bool, JobError> {
    let clamped_progress = progress.clamp(0, 100);
    let res = sqlx::query(
        r#"
        UPDATE jobs
        SET heartbeat_at = now(),
            progress = $3,
            stage = COALESCE($4, stage),
            updated_at = now()
        WHERE id = $1 AND locked_by = $2 AND status = 'running'
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .bind(clamped_progress)
    .bind(stage)
    .execute(pool)
    .await?;

    Ok(res.rows_affected() > 0)
}

/// Marks a running job as succeeded and records its JSON result.
pub async fn complete_job(
    pool: &PgPool,
    job_id: Uuid,
    worker_id: &str,
    result: Option<serde_json::Value>,
) -> Result<Option<JobRecord>, JobError> {
    let job = sqlx::query_as::<_, JobRecord>(
        r#"
        UPDATE jobs
        SET status = 'succeeded',
            progress = 100,
            stage = 'completed',
            result = $3,
            finished_at = now(),
            heartbeat_at = now(),
            updated_at = now()
        WHERE id = $1 AND locked_by = $2 AND status = 'running'
        RETURNING id, queue, type, project_id, user_id, status, priority, payload, result,
                  progress, stage, attempts, max_attempts, error_code, error_message,
                  locked_by, heartbeat_at, run_after, started_at, finished_at, created_at, updated_at
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .bind(result)
    .fetch_optional(pool)
    .await?;

    Ok(job)
}

/// Fails a job permanently or re-queues it with TDD 8.1 backoff (30s, 2m, 10m).
pub async fn fail_job(
    pool: &PgPool,
    job_id: Uuid,
    worker_id: &str,
    error_code: &str,
    error_message: &str,
    permanent: bool,
) -> Result<Option<JobRecord>, JobError> {
    let existing = sqlx::query_as::<_, JobRecord>(
        r#"
        SELECT id, queue, type, project_id, user_id, status, priority, payload, result,
               progress, stage, attempts, max_attempts, error_code, error_message,
               locked_by, heartbeat_at, run_after, started_at, finished_at, created_at, updated_at
        FROM jobs
        WHERE id = $1 AND locked_by = $2 AND status = 'running'
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .fetch_optional(pool)
    .await?;

    let Some(current) = existing else {
        return Ok(None);
    };

    let should_fail_permanently = permanent || current.attempts >= current.max_attempts;
    if should_fail_permanently {
        let updated = sqlx::query_as::<_, JobRecord>(
            r#"
            UPDATE jobs
            SET status = 'failed',
                stage = 'failed',
                error_code = $3,
                error_message = $4,
                finished_at = now(),
                updated_at = now()
            WHERE id = $1 AND locked_by = $2
            RETURNING id, queue, type, project_id, user_id, status, priority, payload, result,
                      progress, stage, attempts, max_attempts, error_code, error_message,
                      locked_by, heartbeat_at, run_after, started_at, finished_at, created_at, updated_at
            "#,
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(error_code)
        .bind(error_message)
        .fetch_optional(pool)
        .await?;
        Ok(updated)
    } else {
        let backoff_secs = match current.attempts {
            1 => 30,
            2 => 120,
            _ => 600,
        };
        let next_run = Utc::now() + Duration::seconds(backoff_secs);
        let updated = sqlx::query_as::<_, JobRecord>(
            r#"
            UPDATE jobs
            SET status = 'queued',
                stage = 'retrying',
                error_code = $3,
                error_message = $4,
                locked_by = NULL,
                heartbeat_at = NULL,
                run_after = $5,
                updated_at = now()
            WHERE id = $1 AND locked_by = $2
            RETURNING id, queue, type, project_id, user_id, status, priority, payload, result,
                      progress, stage, attempts, max_attempts, error_code, error_message,
                      locked_by, heartbeat_at, run_after, started_at, finished_at, created_at, updated_at
            "#,
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(error_code)
        .bind(error_message)
        .bind(next_run)
        .fetch_optional(pool)
        .await?;
        Ok(updated)
    }
}

/// Cancels any queued or running jobs for a project on a specific queue.
pub async fn cancel_project_jobs(
    pool: &PgPool,
    project_id: Uuid,
    queue: JobQueue,
) -> Result<u64, JobError> {
    let res = sqlx::query(
        r#"
        UPDATE jobs
        SET status = 'cancelled',
            stage = 'cancelled',
            finished_at = now(),
            updated_at = now()
        WHERE project_id = $1 AND queue = $2 AND status IN ('queued', 'running')
        "#,
    )
    .bind(project_id)
    .bind(queue.as_str())
    .execute(pool)
    .await?;

    Ok(res.rows_affected())
}

/// Looks up a single job strictly scoped to `project_id` and `user_id`.
pub async fn find_job_by_id_and_project(
    pool: &PgPool,
    job_id: Uuid,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<Option<JobRecord>, JobError> {
    let job = sqlx::query_as::<_, JobRecord>(
        r#"
        SELECT id, queue, type, project_id, user_id, status, priority, payload, result,
               progress, stage, attempts, max_attempts, error_code, error_message,
               locked_by, heartbeat_at, run_after, started_at, finished_at, created_at, updated_at
        FROM jobs
        WHERE id = $1 AND project_id = $2 AND user_id = $3
        "#,
    )
    .bind(job_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(job)
}

/// Returns the most recent job for a project in a given queue.
pub async fn find_latest_project_job(
    pool: &PgPool,
    project_id: Uuid,
    user_id: Uuid,
    queue: JobQueue,
) -> Result<Option<JobRecord>, JobError> {
    let job = sqlx::query_as::<_, JobRecord>(
        r#"
        SELECT id, queue, type, project_id, user_id, status, priority, payload, result,
               progress, stage, attempts, max_attempts, error_code, error_message,
               locked_by, heartbeat_at, run_after, started_at, finished_at, created_at, updated_at
        FROM jobs
        WHERE project_id = $1 AND user_id = $2 AND queue = $3
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(project_id)
    .bind(user_id)
    .bind(queue.as_str())
    .fetch_optional(pool)
    .await?;

    Ok(job)
}
