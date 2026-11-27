use crate::models::{CreateProject, Project, UpdateProject};
use crate::pool::DbError;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn create_project(pool: &PgPool, input: CreateProject) -> Result<Project, DbError> {
    let fps = input.fps.unwrap_or(30);
    let language = input.language.unwrap_or_else(|| "en".to_string());
    let effect_seed = input.effect_seed.unwrap_or(42);
    let settings = input.settings.unwrap_or_else(|| serde_json::json!({}));

    let project = sqlx::query_as!(
        Project,
        r#"
        INSERT INTO projects (owner_id, name, aspect_ratio, fps, language, effect_seed, settings)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, owner_id, name, aspect_ratio, fps, language, effect_seed,
                  settings, version, status, created_at, updated_at, deleted_at
        "#,
        input.owner_id,
        input.name,
        input.aspect_ratio,
        fps,
        language,
        effect_seed,
        settings
    )
    .fetch_one(pool)
    .await?;

    Ok(project)
}

/// Finds a project by ID with strict owner verification (`AUTH-05`).
///
/// Returns `None` if the project does not exist OR if it belongs to a different user.
pub async fn find_project_by_id_and_owner(
    pool: &PgPool,
    project_id: Uuid,
    owner_id: Uuid,
) -> Result<Option<Project>, DbError> {
    let project = sqlx::query_as!(
        Project,
        r#"
        SELECT id, owner_id, name, aspect_ratio, fps, language, effect_seed,
               settings, version, status, created_at, updated_at, deleted_at
        FROM projects
        WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
        "#,
        project_id,
        owner_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(project)
}

/// Lists all active projects owned by a user (`PRJ-03`, `AUTH-05`).
pub async fn list_projects_by_owner(
    pool: &PgPool,
    owner_id: Uuid,
) -> Result<Vec<Project>, DbError> {
    let projects = sqlx::query_as!(
        Project,
        r#"
        SELECT id, owner_id, name, aspect_ratio, fps, language, effect_seed,
               settings, version, status, created_at, updated_at, deleted_at
        FROM projects
        WHERE owner_id = $1 AND deleted_at IS NULL
        ORDER BY updated_at DESC
        "#,
        owner_id
    )
    .fetch_all(pool)
    .await?;

    Ok(projects)
}

/// Updates a project with optimistic version concurrency checking (`PRJ-05`).
///
/// Increments `version` by 1. Returns `DbError::VersionConflict` if the database
/// version does not match `expected_version`.
pub async fn update_project(
    pool: &PgPool,
    project_id: Uuid,
    owner_id: Uuid,
    expected_version: i32,
    update: UpdateProject,
) -> Result<Project, DbError> {
    // 1. Fetch current project to check version and ownership
    let current = find_project_by_id_and_owner(pool, project_id, owner_id)
        .await?
        .ok_or(DbError::NotFound)?;

    if current.version != expected_version {
        return Err(DbError::VersionConflict {
            expected: expected_version,
            actual: current.version,
        });
    }

    let name = update.name.unwrap_or(current.name);
    let aspect_ratio = update.aspect_ratio.unwrap_or(current.aspect_ratio);
    let settings = update.settings.unwrap_or(current.settings);
    let status = update.status.unwrap_or(current.status);

    let updated = sqlx::query_as!(
        Project,
        r#"
        UPDATE projects
        SET name = $3,
            aspect_ratio = $4,
            settings = $5,
            status = $6,
            version = version + 1
        WHERE id = $1 AND owner_id = $2 AND version = $7
        RETURNING id, owner_id, name, aspect_ratio, fps, language, effect_seed,
                  settings, version, status, created_at, updated_at, deleted_at
        "#,
        project_id,
        owner_id,
        name,
        aspect_ratio,
        settings,
        status,
        expected_version
    )
    .fetch_optional(pool)
    .await?;

    match updated {
        Some(p) => Ok(p),
        None => {
            // Concurrent update won race between check and update
            let fresh = find_project_by_id_and_owner(pool, project_id, owner_id)
                .await?
                .ok_or(DbError::NotFound)?;
            Err(DbError::VersionConflict {
                expected: expected_version,
                actual: fresh.version,
            })
        }
    }
}

/// Soft-deletes a project (`PRJ-06`, `AUTH-05`).
pub async fn delete_project(
    pool: &PgPool,
    project_id: Uuid,
    owner_id: Uuid,
) -> Result<(), DbError> {
    let rows = sqlx::query!(
        r#"
        UPDATE projects
        SET deleted_at = now(), status = 'deleted'
        WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
        "#,
        project_id,
        owner_id
    )
    .execute(pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(DbError::NotFound);
    }

    Ok(())
}

/// Atomically advances a project's `effect_seed` and clears un-overridden scene effects
/// so non-overridden scenes receive fresh automatic presets while manual edits stay pinned.
pub async fn reroll_project_effects<F>(
    pool: &PgPool,
    project_id: Uuid,
    owner_id: Uuid,
    compute_next_seed: F,
) -> Result<i64, DbError>
where
    F: FnOnce(i64) -> i64,
{
    let mut tx = pool.begin().await?;

    let current_seed: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT effect_seed
        FROM projects
        WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
        FOR UPDATE
        "#,
    )
    .bind(project_id)
    .bind(owner_id)
    .fetch_optional(&mut *tx)
    .await?;

    let seed = current_seed.ok_or(DbError::NotFound)?;
    let next_seed = compute_next_seed(seed);

    sqlx::query(
        r#"
        UPDATE projects
        SET effect_seed = $2,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(project_id)
    .bind(next_seed)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE scene_media
        SET effect = '{}'::jsonb,
            updated_at = now()
        WHERE section_id IN (SELECT id FROM sections WHERE project_id = $1)
          AND effect_overridden = false
        "#,
    )
    .bind(project_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(next_seed)
}
