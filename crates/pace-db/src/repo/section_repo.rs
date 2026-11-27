use crate::models::{CreateSection, ScriptRecord, Section, UpsertSection};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

pub async fn find_section_by_id_and_project(
    pool: &PgPool,
    section_id: Uuid,
    project_id: Uuid,
) -> Result<Option<Section>, sqlx::Error> {
    sqlx::query_as::<_, Section>(
        r#"
        SELECT id, project_id, position, text, needs_resync, created_at, updated_at
        FROM sections
        WHERE id = $1 AND project_id = $2
        "#,
    )
    .bind(section_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_sections_by_project(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Vec<Section>, sqlx::Error> {
    sqlx::query_as::<_, Section>(
        r#"
        SELECT id, project_id, position, text, needs_resync, created_at, updated_at
        FROM sections
        WHERE project_id = $1
        ORDER BY position ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}

pub async fn save_script(
    pool: &PgPool,
    project_id: Uuid,
    raw_text: &str,
) -> Result<ScriptRecord, sqlx::Error> {
    sqlx::query_as::<_, ScriptRecord>(
        r#"
        INSERT INTO scripts (project_id, raw_text)
        VALUES ($1, $2)
        ON CONFLICT (project_id)
        DO UPDATE SET raw_text = EXCLUDED.raw_text, updated_at = now()
        RETURNING project_id, raw_text, created_at, updated_at
        "#,
    )
    .bind(project_id)
    .bind(raw_text)
    .fetch_one(pool)
    .await
}

pub async fn get_script(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Option<ScriptRecord>, sqlx::Error> {
    sqlx::query_as::<_, ScriptRecord>(
        r#"
        SELECT project_id, raw_text, created_at, updated_at
        FROM scripts
        WHERE project_id = $1
        "#,
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

pub async fn create_section(
    pool: &PgPool,
    req: CreateSection,
) -> Result<Section, sqlx::Error> {
    sqlx::query_as::<_, Section>(
        r#"
        INSERT INTO sections (project_id, position, text)
        VALUES ($1, $2, $3)
        RETURNING id, project_id, position, text, needs_resync, created_at, updated_at
        "#,
    )
    .bind(req.project_id)
    .bind(req.position)
    .bind(req.text)
    .fetch_one(pool)
    .await
}

pub async fn upsert_script_and_sections(
    pool: &PgPool,
    project_id: Uuid,
    raw_text: &str,
    items: &[UpsertSection],
) -> Result<(ScriptRecord, Vec<Section>), sqlx::Error> {
    let mut tx = pool.begin().await?;

    let script = sqlx::query_as::<_, ScriptRecord>(
        r#"
        INSERT INTO scripts (project_id, raw_text)
        VALUES ($1, $2)
        ON CONFLICT (project_id)
        DO UPDATE SET raw_text = EXCLUDED.raw_text, updated_at = now()
        RETURNING project_id, raw_text, created_at, updated_at
        "#,
    )
    .bind(project_id)
    .bind(raw_text)
    .fetch_one(&mut *tx)
    .await?;

    let existing_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM sections WHERE project_id = $1 FOR UPDATE",
    )
    .bind(project_id)
    .fetch_all(&mut *tx)
    .await?;
    let existing_set: HashSet<Uuid> = existing_ids.into_iter().collect();

    let mut seen_ids = HashSet::new();
    let mut kept_ids = Vec::new();
    for item in items {
        if let Some(id) = item.id {
            if existing_set.contains(&id) && seen_ids.insert(id) {
                kept_ids.push(id);
            }
        }
    }

    if kept_ids.is_empty() {
        sqlx::query("DELETE FROM sections WHERE project_id = $1")
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("DELETE FROM sections WHERE project_id = $1 AND NOT (id = ANY($2))")
            .bind(project_id)
            .bind(&kept_ids)
            .execute(&mut *tx)
            .await?;
    }

    // Reassign IDs cleanly while preserving scene_media foreign keys on kept sections.
    // `uq_sections_project_position` is DEFERRABLE INITIALLY DEFERRED so in-place position swaps commit cleanly.
    let mut updated_ids = HashSet::new();
    let mut results = Vec::with_capacity(items.len());

    for (idx, item) in items.iter().enumerate() {
        let reuse_id = item
            .id
            .filter(|id| existing_set.contains(id) && updated_ids.insert(*id));

        let sec = if let Some(sec_id) = reuse_id {
            sqlx::query_as::<_, Section>(
                r#"
                UPDATE sections
                SET position = $1,
                    text = $2,
                    needs_resync = (needs_resync OR (text IS DISTINCT FROM $2)),
                    updated_at = now()
                WHERE id = $3 AND project_id = $4
                RETURNING id, project_id, position, text, needs_resync, created_at, updated_at
                "#,
            )
            .bind(idx as i32)
            .bind(&item.text)
            .bind(sec_id)
            .bind(project_id)
            .fetch_one(&mut *tx)
            .await?
        } else {
            sqlx::query_as::<_, Section>(
                r#"
                INSERT INTO sections (project_id, position, text)
                VALUES ($1, $2, $3)
                RETURNING id, project_id, position, text, needs_resync, created_at, updated_at
                "#,
            )
            .bind(project_id)
            .bind(idx as i32)
            .bind(&item.text)
            .fetch_one(&mut *tx)
            .await?
        };

        results.push(sec);
    }

    tx.commit().await?;
    Ok((script, results))
}

pub async fn replace_sections(
    pool: &PgPool,
    project_id: Uuid,
    texts: &[String],
) -> Result<Vec<Section>, sqlx::Error> {
    let items: Vec<UpsertSection> = texts
        .iter()
        .map(|text| UpsertSection {
            id: None,
            text: text.clone(),
        })
        .collect();
    let raw_text = texts.join("\n\n");
    let (_, sections) = upsert_script_and_sections(pool, project_id, &raw_text, &items).await?;
    Ok(sections)
}

