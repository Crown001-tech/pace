use pace_db::models::{
    CreateMediaAsset, CreateProject, CreateSession, CreateUser, UpdateProject, UpsertSection,
};
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

#[tokio::test]
async fn test_upsert_sections_preserves_ids_and_assignments() {
    let pool = setup_test_pool().await;
    let user = create_user(
        &pool,
        CreateUser {
            email: format!("script_{}@example.com", Uuid::now_v7()),
            name: "Script User".to_string(),
            password_hash: "hash".to_string(),
            role: "creator".to_string(),
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
            name: "Script Test".to_string(),
            aspect_ratio: "16:9".to_string(),
            fps: None,
            language: None,
            effect_seed: None,
            settings: None,
        },
    )
    .await
    .unwrap();

    let (_, initial_sections) = upsert_script_and_sections(
        &pool,
        project.id,
        "First scene.\n\nSecond scene.",
        &[
            UpsertSection {
                id: None,
                text: "First scene.".to_string(),
            },
            UpsertSection {
                id: None,
                text: "Second scene.".to_string(),
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(initial_sections.len(), 2);
    let sec_a_id = initial_sections[0].id;
    let sec_b_id = initial_sections[1].id;

    let asset = create_media(
        &pool,
        CreateMediaAsset {
            project_id: project.id,
            owner_id: user.id,
            kind: "image".to_string(),
            storage_key: format!("u/{}/p/{}/media/a/original.png", user.id, project.id),
            preview_key: None,
            thumb_key: None,
            original_filename: "a.png".to_string(),
            mime: "image/png".to_string(),
            bytes: 1234,
            sha256: "sha_a".to_string(),
            width: Some(1920),
            height: Some(1080),
            duration_ms: None,
            source: Some("upload".to_string()),
            prompt: None,
        },
    )
    .await
    .unwrap();

    assign_media(&pool, project.id, sec_a_id, asset.id, "fit_blur", 0.25, 0.75)
        .await
        .unwrap();

    // Insert a new section at position 0 and swap sec_b to 1, edited sec_a to 2
    let (script, updated) = upsert_script_and_sections(
        &pool,
        project.id,
        "Intro\n\nSecond scene.\n\nFirst scene edited.",
        &[
            UpsertSection {
                id: None,
                text: "Intro".to_string(),
            },
            UpsertSection {
                id: Some(sec_b_id),
                text: "Second scene.".to_string(),
            },
            UpsertSection {
                id: Some(sec_a_id),
                text: "First scene edited.".to_string(),
            },
        ],
    )
    .await
    .unwrap();

    assert_eq!(script.raw_text, "Intro\n\nSecond scene.\n\nFirst scene edited.");
    assert_eq!(updated.len(), 3);
    assert_eq!(updated[1].id, sec_b_id);
    assert!(!updated[1].needs_resync);
    assert_eq!(updated[2].id, sec_a_id);
    assert!(updated[2].needs_resync);

    // Scene assignment on sec_a_id MUST still be present!
    let assignments = get_assignments_by_project(&pool, project.id).await.unwrap();
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].section_id, sec_a_id);
    assert_eq!(assignments[0].media_id, asset.id);
    assert_eq!(assignments[0].fit_mode, "fit_blur");
}

#[tokio::test]
async fn test_media_and_assignment_project_isolation() {
    let pool = setup_test_pool().await;
    let user = create_user(
        &pool,
        CreateUser {
            email: format!("iso_{}@example.com", Uuid::now_v7()),
            name: "Iso User".to_string(),
            password_hash: "hash".to_string(),
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: None,
        },
    )
    .await
    .unwrap();

    let project_a = create_project(
        &pool,
        CreateProject {
            owner_id: user.id,
            name: "Proj A".to_string(),
            aspect_ratio: "16:9".to_string(),
            fps: None,
            language: None,
            effect_seed: None,
            settings: None,
        },
    )
    .await
    .unwrap();
    let project_b = create_project(
        &pool,
        CreateProject {
            owner_id: user.id,
            name: "Proj B".to_string(),
            aspect_ratio: "16:9".to_string(),
            fps: None,
            language: None,
            effect_seed: None,
            settings: None,
        },
    )
    .await
    .unwrap();

    let (_, secs_a) = upsert_script_and_sections(
        &pool,
        project_a.id,
        "A1\n\nA2",
        &[
            UpsertSection { id: None, text: "A1".to_string() },
            UpsertSection { id: None, text: "A2".to_string() },
        ],
    )
    .await
    .unwrap();
    let (_, secs_b) = upsert_script_and_sections(
        &pool,
        project_b.id,
        "B1",
        &[UpsertSection { id: None, text: "B1".to_string() }],
    )
    .await
    .unwrap();

    let media_a = create_media(
        &pool,
        CreateMediaAsset {
            project_id: project_a.id,
            owner_id: user.id,
            kind: "image".to_string(),
            storage_key: format!("u/{}/p/{}/media/m1/original.jpg", user.id, project_a.id),
            preview_key: None,
            thumb_key: None,
            original_filename: "m1.jpg".to_string(),
            mime: "image/jpeg".to_string(),
            bytes: 2048,
            sha256: "sha_m1".to_string(),
            width: Some(1280),
            height: Some(720),
            duration_ms: None,
            source: None,
            prompt: None,
        },
    )
    .await
    .unwrap();

    // Cross-project assign must return NotFound
    let err = assign_media(&pool, project_b.id, secs_b[0].id, media_a.id, "fill", 0.5, 0.5).await;
    assert!(matches!(err, Err(DbError::NotFound)));

    // Cross-project swap must return NotFound
    let swap_err = swap_assignments(&pool, project_a.id, secs_a[0].id, secs_b[0].id).await;
    assert!(matches!(swap_err, Err(DbError::NotFound)));

    // Cross-project delete_media must return None without deleting
    let wrong_del = delete_media(&pool, project_b.id, media_a.id).await.unwrap();
    assert!(wrong_del.is_none());
    assert!(find_by_id_and_project(&pool, media_a.id, project_a.id).await.unwrap().is_some());
}

#[tokio::test]
async fn test_narration_boundaries_subtitles_and_quota_reclaim() {
    let pool = setup_test_pool().await;
    let user = create_user(
        &pool,
        CreateUser {
            email: format!("nar_db_{}@example.com", Uuid::now_v7()),
            name: "Narration DB User".to_string(),
            password_hash: "hash".to_string(),
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(10_000_000),
        },
    )
    .await
    .unwrap();

    let project = create_project(
        &pool,
        CreateProject {
            owner_id: user.id,
            name: "Narration Test Project".to_string(),
            aspect_ratio: "16:9".to_string(),
            fps: None,
            language: None,
            effect_seed: None,
            settings: None,
        },
    )
    .await
    .unwrap();

    let nar_media = create_media(
        &pool,
        CreateMediaAsset {
            project_id: project.id,
            owner_id: user.id,
            kind: "narration".to_string(),
            storage_key: format!("u/{}/p/{}/audio/a1/original.mp3", user.id, project.id),
            preview_key: None,
            thumb_key: None,
            original_filename: "voice.mp3".to_string(),
            mime: "audio/mpeg".to_string(),
            bytes: 123_456,
            sha256: "sha_nar_1".to_string(),
            width: None,
            height: None,
            duration_ms: Some(10_000),
            source: None,
            prompt: None,
        },
    )
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO narrations (project_id, media_id, duration_ms, loudness_lufs, true_peak_db, peaks_key, silences)
        VALUES ($1, $2, 10000, -16.0, -1.2, 'peaks.json', '[]'::jsonb)
        "#,
    )
    .bind(project.id)
    .bind(nar_media.id)
    .execute(&pool)
    .await
    .unwrap();

    for (idx, time_ms) in [(0, 0), (1, 4000), (2, 10000)] {
        sqlx::query(
            r#"
            INSERT INTO boundaries (project_id, index, time_ms, source, confidence, flags)
            VALUES ($1, $2, $3, 'auto', 0.9, '{}')
            "#,
        )
        .bind(project.id)
        .bind(idx)
        .bind(time_ms)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Cannot move start/end boundaries manually
    let start_err = update_boundary(&pool, project.id, 0, 500, None).await;
    assert!(matches!(start_err, Err(DbError::Validation(_))));

    // Cannot cross neighboring boundary
    let overlap_err = update_boundary(&pool, project.id, 1, 9900, None).await;
    assert!(matches!(overlap_err, Err(DbError::Validation(_))));

    // Valid nudge sets source = 'manual'
    let updated = update_boundary(&pool, project.id, 1, 4250, None).await.unwrap();
    assert_eq!(updated.time_ms, 4250);
    assert_eq!(updated.source, "manual");

    let cue_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO subtitle_cues (project_id, position, start_ms, end_ms, text)
        VALUES ($1, 0, 0, 3500, 'First line')
        RETURNING id
        "#,
    )
    .bind(project.id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let edited = update_subtitle_cue(&pool, project.id, cue_id, "Updated line", None, None)
        .await
        .unwrap()
        .unwrap();
    assert!(edited.edited);
    assert_eq!(edited.text, "Updated line");

    // Delete cleans up narration, boundaries, subtitles, and quota
    let deleted = delete_project_narration(&pool, project.id, user.id).await.unwrap();
    assert_eq!(deleted.len(), 1);
    assert!(get_narration(&pool, project.id).await.unwrap().is_none());
    assert!(list_boundaries(&pool, project.id).await.unwrap().is_empty());
    assert!(list_subtitle_cues(&pool, project.id).await.unwrap().is_empty());

    let refreshed_user = find_user_by_id(&pool, user.id).await.unwrap().unwrap();
    assert_eq!(refreshed_user.storage_used_bytes, 0);
}
