use pace_db::models::{CreateProject, CreateUser};
use pace_db::pool::{create_pool, run_migrations};
use pace_db::repo::{create_project, create_user};
use pace_jobs::{
    claim_next_job, complete_job, enqueue_job, fail_job, find_job_by_id_and_project, heartbeat_job,
    EnqueueJob, JobQueue,
};
use serde_json::json;
use std::env;
use std::sync::OnceLock;
use tokio::sync::Mutex;
use uuid::Uuid;

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

async fn setup_test_pool() -> (sqlx::PgPool, tokio::sync::MutexGuard<'static, ()>) {
    let guard = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().await;
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pace:pace@localhost:5432/pace".to_string());
    let pool = create_pool(&database_url)
        .await
        .expect("connect to test postgres");
    run_migrations(&pool).await.expect("migrations ok");
    sqlx::query("DELETE FROM jobs WHERE priority >= 10000")
        .execute(&pool)
        .await
        .ok();
    (pool, guard)
}

#[tokio::test]
async fn test_concurrent_workers_never_claim_same_job() {
    let (pool, _guard) = setup_test_pool().await;
    let user = create_user(
        &pool,
        CreateUser {
            email: format!("jobs_{}@example.com", Uuid::now_v7()),
            name: "Jobs User".into(),
            password_hash: "hash".into(),
            role: "creator".into(),
            must_change_password: false,
            storage_quota_bytes: None,
        },
    )
    .await
    .unwrap();
    let project = create_project(
        &pool,
        CreateProject {
            owner_id: user.id,
            name: "Job Queue Project".into(),
            aspect_ratio: "16:9".into(),
            fps: None,
            language: None,
            effect_seed: None,
            settings: None,
        },
    )
    .await
    .unwrap();

    // Clean any pre-existing queued audio jobs so only our high-priority test jobs compete.
    let job = enqueue_job(
        &pool,
        EnqueueJob {
            queue: JobQueue::Audio,
            job_type: "audio_process".into(),
            project_id: Some(project.id),
            user_id: Some(user.id),
            priority: 10_000,
            payload: json!({ "project_id": project.id }),
            max_attempts: 3,
        },
    )
    .await
    .unwrap();

    let (r1, r2) = tokio::join!(
        claim_next_job(&pool, JobQueue::Audio, "worker-a"),
        claim_next_job(&pool, JobQueue::Audio, "worker-b")
    );
    let c1 = r1.unwrap();
    let c2 = r2.unwrap();

    let claimed_ids: Vec<Uuid> = [c1, c2]
        .into_iter()
        .flatten()
        .map(|j| j.id)
        .collect();

    assert!(claimed_ids.contains(&job.id));
    assert_eq!(claimed_ids.iter().filter(|&&id| id == job.id).count(), 1);

    // Clean up test job status
    let fetched = find_job_by_id_and_project(&pool, job.id, project.id, user.id)
        .await
        .unwrap()
        .unwrap();
    let winner = fetched.locked_by.unwrap();
    assert!(heartbeat_job(&pool, job.id, &winner, 50, Some("transcribing"))
        .await
        .unwrap());
    let done = complete_job(&pool, job.id, &winner, Some(json!({"ok": true})))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.status, "succeeded");
    assert_eq!(done.progress, 100);
}

#[tokio::test]
async fn test_stale_heartbeat_reclaim_and_permanent_vs_retryable_fail() {
    let (pool, _guard) = setup_test_pool().await;
    let user = create_user(
        &pool,
        CreateUser {
            email: format!("jobs_stale_{}@example.com", Uuid::now_v7()),
            name: "Stale Jobs User".into(),
            password_hash: "hash".into(),
            role: "creator".into(),
            must_change_password: false,
            storage_quota_bytes: None,
        },
    )
    .await
    .unwrap();
    let project = create_project(
        &pool,
        CreateProject {
            owner_id: user.id,
            name: "Stale Queue Project".into(),
            aspect_ratio: "16:9".into(),
            fps: None,
            language: None,
            effect_seed: None,
            settings: None,
        },
    )
    .await
    .unwrap();

    let job = enqueue_job(
        &pool,
        EnqueueJob {
            queue: JobQueue::Audio,
            job_type: "audio_process".into(),
            project_id: Some(project.id),
            user_id: Some(user.id),
            priority: 20_000,
            payload: json!({ "project_id": project.id }),
            max_attempts: 3,
        },
    )
    .await
    .unwrap();

    let claimed = claim_next_job(&pool, JobQueue::Audio, "dead-worker")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, job.id);
    assert_eq!(claimed.attempts, 1);

    // Backdate heartbeat > 120 seconds to simulate worker crash
    sqlx::query("UPDATE jobs SET heartbeat_at = now() - interval '180 seconds' WHERE id = $1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();

    let reclaimed = claim_next_job(&pool, JobQueue::Audio, "rescue-worker")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reclaimed.id, job.id);
    assert_eq!(reclaimed.attempts, 2);
    assert_eq!(reclaimed.locked_by.as_deref(), Some("rescue-worker"));

    // Retryable fail should re-queue with future run_after
    let retried = fail_job(
        &pool,
        job.id,
        "rescue-worker",
        "GROQ_429",
        "Rate limited",
        false,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(retried.status, "queued");
    assert!(retried.run_after > retried.created_at);

    // Force immediate retry & verify permanent error skips remaining attempt
    sqlx::query("UPDATE jobs SET run_after = now() - interval '1 second' WHERE id = $1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let final_claim = claim_next_job(&pool, JobQueue::Audio, "worker-c")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_claim.id, job.id);

    let failed = fail_job(
        &pool,
        job.id,
        "worker-c",
        "INVALID_AUDIO",
        "Corrupt stream",
        true,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.error_code.as_deref(), Some("INVALID_AUDIO"));
}
