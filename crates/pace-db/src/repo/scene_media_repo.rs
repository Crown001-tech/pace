use crate::models::SceneMedia;
use crate::pool::DbError;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn get_assignments_by_project(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Vec<SceneMedia>, sqlx::Error> {
    sqlx::query_as::<_, SceneMedia>(
        r#"
        SELECT sm.id, sm.section_id, sm.media_id, sm.position, sm.fit_mode,
               sm.focal_x, sm.focal_y, sm.effect, sm.effect_overridden,
               sm.trim, sm.created_at, sm.updated_at
        FROM scene_media sm
        JOIN sections s ON s.id = sm.section_id
        WHERE s.project_id = $1
        ORDER BY s.position ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}

pub async fn get_assignment_by_section(
    pool: &PgPool,
    project_id: Uuid,
    section_id: Uuid,
) -> Result<Option<SceneMedia>, DbError> {
    let section_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sections WHERE id = $1 AND project_id = $2)",
    )
    .bind(section_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    if !section_exists {
        return Err(DbError::NotFound);
    }

    let row = sqlx::query_as::<_, SceneMedia>(
        r#"
        SELECT sm.id, sm.section_id, sm.media_id, sm.position, sm.fit_mode,
               sm.focal_x, sm.focal_y, sm.effect, sm.effect_overridden,
               sm.trim, sm.created_at, sm.updated_at
        FROM scene_media sm
        WHERE sm.section_id = $1
        "#,
    )
    .bind(section_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_scene_effect(
    pool: &PgPool,
    project_id: Uuid,
    section_id: Uuid,
    fit_mode: Option<&str>,
    focal_x: Option<f32>,
    focal_y: Option<f32>,
    effect: Option<serde_json::Value>,
    effect_overridden: Option<bool>,
) -> Result<SceneMedia, DbError> {
    let section_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sections WHERE id = $1 AND project_id = $2)",
    )
    .bind(section_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    if !section_exists {
        return Err(DbError::NotFound);
    }

    let row = sqlx::query_as::<_, SceneMedia>(
        r#"
        UPDATE scene_media
        SET fit_mode = COALESCE($2, fit_mode),
            focal_x = COALESCE($3, focal_x),
            focal_y = COALESCE($4, focal_y),
            effect = COALESCE($5, effect),
            effect_overridden = COALESCE($6, effect_overridden),
            updated_at = now()
        WHERE section_id = $1
        RETURNING id, section_id, media_id, position, fit_mode,
                  focal_x, focal_y, effect, effect_overridden,
                  trim, created_at, updated_at
        "#,
    )
    .bind(section_id)
    .bind(fit_mode)
    .bind(focal_x)
    .bind(focal_y)
    .bind(effect)
    .bind(effect_overridden)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        DbError::Validation(
            "Assign an image to this scene before customizing its framing or motion effects"
                .to_string(),
        )
    })?;

    Ok(row)
}

pub async fn assign_media(
    pool: &PgPool,
    project_id: Uuid,
    section_id: Uuid,
    media_id: Uuid,
    fit_mode: &str,
    focal_x: f32,
    focal_y: f32,
) -> Result<SceneMedia, DbError> {
    let mut tx = pool.begin().await?;

    let valid_pair: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(SELECT 1 FROM sections WHERE id = $1 AND project_id = $3)
           AND EXISTS(SELECT 1 FROM media_assets WHERE id = $2 AND project_id = $3)
        "#,
    )
    .bind(section_id)
    .bind(media_id)
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await?;

    if !valid_pair {
        tx.rollback().await?;
        return Err(DbError::NotFound);
    }

    let row = sqlx::query_as::<_, SceneMedia>(
        r#"
        INSERT INTO scene_media (section_id, media_id, fit_mode, focal_x, focal_y)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (section_id)
        DO UPDATE SET
            media_id = EXCLUDED.media_id,
            fit_mode = EXCLUDED.fit_mode,
            focal_x = EXCLUDED.focal_x,
            focal_y = EXCLUDED.focal_y,
            updated_at = now()
        RETURNING id, section_id, media_id, position, fit_mode,
                  focal_x, focal_y, effect, effect_overridden,
                  trim, created_at, updated_at
        "#,
    )
    .bind(section_id)
    .bind(media_id)
    .bind(fit_mode)
    .bind(focal_x)
    .bind(focal_y)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(row)
}

pub async fn unassign_section(
    pool: &PgPool,
    project_id: Uuid,
    section_id: Uuid,
) -> Result<bool, DbError> {
    let section_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sections WHERE id = $1 AND project_id = $2)",
    )
    .bind(section_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    if !section_exists {
        return Err(DbError::NotFound);
    }

    let res = sqlx::query("DELETE FROM scene_media WHERE section_id = $1")
        .bind(section_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn swap_assignments(
    pool: &PgPool,
    project_id: Uuid,
    section_id_a: Uuid,
    section_id_b: Uuid,
) -> Result<(), DbError> {
    let mut tx = pool.begin().await?;

    let valid_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sections WHERE project_id = $1 AND id IN ($2, $3)",
    )
    .bind(project_id)
    .bind(section_id_a)
    .bind(section_id_b)
    .fetch_one(&mut *tx)
    .await?;

    let expected_count = if section_id_a == section_id_b { 1 } else { 2 };
    if valid_count != expected_count {
        tx.rollback().await?;
        return Err(DbError::NotFound);
    }

    if section_id_a == section_id_b {
        tx.commit().await?;
        return Ok(());
    }

    let item_a = sqlx::query_as::<_, SceneMedia>(
        "SELECT id, section_id, media_id, position, fit_mode, focal_x, focal_y, effect, effect_overridden, trim, created_at, updated_at FROM scene_media WHERE section_id = $1 FOR UPDATE",
    )
    .bind(section_id_a)
    .fetch_optional(&mut *tx)
    .await?;

    let item_b = sqlx::query_as::<_, SceneMedia>(
        "SELECT id, section_id, media_id, position, fit_mode, focal_x, focal_y, effect, effect_overridden, trim, created_at, updated_at FROM scene_media WHERE section_id = $1 FOR UPDATE",
    )
    .bind(section_id_b)
    .fetch_optional(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM scene_media WHERE section_id IN ($1, $2)")
        .bind(section_id_a)
        .bind(section_id_b)
        .execute(&mut *tx)
        .await?;

    if let Some(a) = item_a {
        sqlx::query(
            r#"
            INSERT INTO scene_media (
                section_id, media_id, position, fit_mode, focal_x, focal_y,
                effect, effect_overridden, trim
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(section_id_b)
        .bind(a.media_id)
        .bind(a.position)
        .bind(a.fit_mode)
        .bind(a.focal_x)
        .bind(a.focal_y)
        .bind(a.effect)
        .bind(a.effect_overridden)
        .bind(a.trim)
        .execute(&mut *tx)
        .await?;
    }

    if let Some(b) = item_b {
        sqlx::query(
            r#"
            INSERT INTO scene_media (
                section_id, media_id, position, fit_mode, focal_x, focal_y,
                effect, effect_overridden, trim
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(section_id_a)
        .bind(b.media_id)
        .bind(b.position)
        .bind(b.fit_mode)
        .bind(b.focal_x)
        .bind(b.focal_y)
        .bind(b.effect)
        .bind(b.effect_overridden)
        .bind(b.trim)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}
