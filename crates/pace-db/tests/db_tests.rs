use pace_db::models::{CreateProject, CreateSession, CreateUser, UpdateProject};
use pace_db::pool::{create_pool, run_migrations, DbError};
use pace_db::repo::*;
use std::env;
use uuid::Uuid;

async fn setup_test_pool() -> sqlx::PgPool {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pace:pace@localhost:5432/pace".to_string());
    let pool = create_pool(&database_url).await.expect("Failed to connect to test Postgres");
    run_migrations(&pool).await.expect("Failed to run migrations");
    pool
}

#[tokio::test]
async fn test_migrations_and_case_insensitive_user_lookup() {
    let pool = setup_test_pool().await;
    let unique_email = format!("creator_{}@example.com", Uuid::now_v7());

    let user = create_user(
        &pool,
        CreateUser {
            email: unique_email.clone(),
            name: "Test Creator".to_string(),
            password_hash: "argon2id$test_hash".to_string(),
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: None,
        },
    )
    .await
    .expect("create user ok");

    assert_eq!(user.name, "Test Creator");
    assert_eq!(user.storage_quota_bytes, 2 * 1024 * 1024 * 1024); // 2 GB default

    // Case-insensitive lookup (AUTH-01)
    let uppercase_email = unique_email.to_uppercase();
    let found = find_user_by_email(&pool, &uppercase_email)
        .await
        .expect("query ok")
        .expect("user found");

    assert_eq!(found.id, user.id);
}

#[tokio::test]
async fn test_login_attempts_and_10_failure_lockout() {
    let pool = setup_test_pool().await;
    let email = format!("locked_{}@example.com", Uuid::now_v7());

    // 0 failures -> not locked
    assert!(check_lockout(&pool, &email).await.is_ok());

    // Record 9 failures -> still not locked
    for _ in 0..9 {
        record_login_attempt(&pool, &email, false).await.expect("record ok");
    }
    assert!(check_lockout(&pool, &email).await.is_ok());

    // 10th failure -> locked!
    record_login_attempt(&pool, &email, false).await.expect("record ok");
    let lockout_err = check_lockout(&pool, &email).await;
    assert!(matches!(lockout_err, Err(DbError::AccountLocked)));
}

#[tokio::test]
async fn test_session_lifecycle_and_invalidation() {
    let pool = setup_test_pool().await;
    let email = format!("sess_{}@example.com", Uuid::now_v7());

    let user = create_user(
        &pool,
        CreateUser {
            email,
            name: "Session User".to_string(),
            password_hash: "hash".to_string(),
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: None,
        },
    )
    .await
    .expect("user ok");

    let token_hash = format!("token_hash_{}", Uuid::now_v7());
    let session = create_session(
        &pool,
        CreateSession {
            user_id: user.id,
            token_hash: token_hash.clone(),
            expires_at: chrono::Utc::now() + chrono::Duration::days(30),
            user_agent: Some("PaceTest/1.0".to_string()),
        },
    )
    .await
    .expect("session ok");

    // Lookup by hash
    let found = find_active_session_by_token_hash(&pool, &token_hash)
        .await
        .expect("query ok")
        .expect("session found");
    assert_eq!(found.id, session.id);

    // Invalidate all user sessions (on password change / logout all)
    let deleted = delete_all_user_sessions(&pool, user.id).await.expect("delete ok");
    assert_eq!(deleted, 1);

    let not_found = find_active_session_by_token_hash(&pool, &token_hash)
        .await
        .expect("query ok");
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_project_version_concurrency_control() {
    let pool = setup_test_pool().await;
    let email = format!("prj_owner_{}@example.com", Uuid::now_v7());

    let user = create_user(
        &pool,
        CreateUser {
            email,
            name: "Project Owner".to_string(),
            password_hash: "hash".to_string(),
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: None,
        },
    )
    .await
    .expect("user ok");

    let project = create_project(
        &pool,
        CreateProject {
            owner_id: user.id,
            name: "Initial Video".to_string(),
            aspect_ratio: "16:9".to_string(),
            fps: None,
            language: None,
            effect_seed: None,
            settings: None,
        },
    )
    .await
    .expect("create project ok");

    assert_eq!(project.version, 1);

    // Update with correct version (1) -> succeeds and bumps to version 2
    let updated = update_project(
        &pool,
        project.id,
        user.id,
        1,
        UpdateProject {
            name: Some("Renamed Video".to_string()),
            aspect_ratio: None,
            settings: None,
            status: None,
        },
    )
    .await
    .expect("update ok");

    assert_eq!(updated.name, "Renamed Video");
    assert_eq!(updated.version, 2);

    // Stale update with version 1 -> VersionConflict!
    let stale_err = update_project(
        &pool,
        project.id,
        user.id,
        1,
        UpdateProject {
            name: Some("Conflicting Rename".to_string()),
            aspect_ratio: None,
            settings: None,
            status: None,
        },
    )
    .await;

    assert!(matches!(
        stale_err,
        Err(DbError::VersionConflict {
            expected: 1,
            actual: 2
        })
    ));
}

#[tokio::test]
async fn test_strict_ownership_isolation() {
    let pool = setup_test_pool().await;

    // Create User A
    let user_a = create_user(
        &pool,
        CreateUser {
            email: format!("user_a_{}@example.com", Uuid::now_v7()),
            name: "User A".to_string(),
            password_hash: "hash_a".to_string(),
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: None,
        },
    )
    .await
    .expect("user a ok");

    // Create User B
    let user_b = create_user(
        &pool,
        CreateUser {
            email: format!("user_b_{}@example.com", Uuid::now_v7()),
            name: "User B".to_string(),
            password_hash: "hash_b".to_string(),
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: None,
        },
    )
    .await
    .expect("user b ok");

    // User A creates a secret project
    let project_a = create_project(
        &pool,
        CreateProject {
            owner_id: user_a.id,
            name: "User A Secret Project".to_string(),
            aspect_ratio: "9:16".to_string(),
            fps: None,
            language: None,
            effect_seed: None,
            settings: None,
        },
    )
    .await
    .expect("project a ok");

    // 1. User B tries to view User A's project -> returns None (AUTH-05)
    let view_attempt = find_project_by_id_and_owner(&pool, project_a.id, user_b.id)
        .await
        .expect("query ok");
    assert!(view_attempt.is_none(), "User B must not see User A's project");

    // 2. User B tries to update User A's project -> returns NotFound
    let update_attempt = update_project(
        &pool,
        project_a.id,
        user_b.id,
        1,
        UpdateProject {
            name: Some("Hacked by User B".to_string()),
            aspect_ratio: None,
            settings: None,
            status: None,
        },
    )
    .await;
    assert!(matches!(update_attempt, Err(DbError::NotFound)));

    // 3. User B tries to delete User A's project -> returns NotFound
    let delete_attempt = delete_project(&pool, project_a.id, user_b.id).await;
    assert!(matches!(delete_attempt, Err(DbError::NotFound)));

    // 4. User B lists their projects -> project_a is not in list
    let user_b_projects = list_projects_by_owner(&pool, user_b.id)
        .await
        .expect("list ok");
    assert!(user_b_projects.is_empty());

    // 5. User A lists their projects -> project_a IS in list
    let user_a_projects = list_projects_by_owner(&pool, user_a.id)
        .await
        .expect("list ok");
    assert_eq!(user_a_projects.len(), 1);
    assert_eq!(user_a_projects[0].id, project_a.id);
}
