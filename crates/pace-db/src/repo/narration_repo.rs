use crate::models::{Boundary, MediaAsset, Narration, SubtitleCue, Transcript};
use crate::pool::DbError;
use sqlx::PgPool;
use uuid::Uuid;

const MIN_BOUNDARY_GAP_MS: i32 = 500;

pub async fn find_narration_media_by_project(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Option<MediaAsset>, DbError> {
    let media = sqlx::query_as::<_, MediaAsset>(
        r#"
        SELECT id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
               original_filename, mime, bytes, sha256, width, height, duration_ms,
               source, prompt, status, probe, created_at, updated_at
        FROM media_assets
        WHERE project_id = $1 AND kind = 'narration'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(media)
}

pub async fn get_narration(pool: &PgPool, project_id: Uuid) -> Result<Option<Narration>, DbError> {
    let row = sqlx::query_as::<_, Narration>(
        r#"
        SELECT project_id, media_id, duration_ms, loudness_lufs, true_peak_db,
               peaks_key, silences, created_at, updated_at
        FROM narrations
        WHERE project_id = $1
        "#,
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_latest_transcript(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Option<Transcript>, DbError> {
    let row = sqlx::query_as::<_, Transcript>(
        r#"
        SELECT id, project_id, narration_media_id, engine, model, language, words,
               created_at, updated_at
        FROM transcripts
        WHERE project_id = $1
        ORDER BY updated_at DESC, created_at DESC
        LIMIT 1
        "#,
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn list_boundaries(pool: &PgPool, project_id: Uuid) -> Result<Vec<Boundary>, DbError> {
    let rows = sqlx::query_as::<_, Boundary>(
        r#"
        SELECT id, project_id, index, time_ms, source, confidence, flags,
               transition, created_at, updated_at
        FROM boundaries
        WHERE project_id = $1
        ORDER BY index ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Updates an intermediate boundary cut time (`source = 'manual'`) while enforcing monotonic order
/// and locking boundary 0 and the final end boundary.
pub async fn update_boundary(
    pool: &PgPool,
    project_id: Uuid,
    index: i32,
    new_time_ms: i32,
    transition: Option<serde_json::Value>,
) -> Result<Boundary, DbError> {
    let mut tx = pool.begin().await?;

    let all = sqlx::query_as::<_, Boundary>(
        r#"
        SELECT id, project_id, index, time_ms, source, confidence, flags,
               transition, created_at, updated_at
        FROM boundaries
        WHERE project_id = $1
        ORDER BY index ASC
        FOR UPDATE
        "#,
    )
    .bind(project_id)
    .fetch_all(&mut *tx)
    .await?;

    if all.is_empty() {
        return Err(DbError::NotFound);
    }

    let pos = all
        .iter()
        .position(|b| b.index == index)
        .ok_or(DbError::NotFound)?;

    if pos == 0 || pos + 1 == all.len() {
        return Err(DbError::Validation(
            "Start and end project boundaries cannot be moved manually".to_string(),
        ));
    }

    let prev_ms = all[pos - 1].time_ms;
    let next_ms = all[pos + 1].time_ms;

    if new_time_ms < prev_ms + MIN_BOUNDARY_GAP_MS || new_time_ms > next_ms - MIN_BOUNDARY_GAP_MS {
        return Err(DbError::Validation(format!(
            "Boundary time {new_time_ms}ms must stay between {}ms and {}ms",
            prev_ms + MIN_BOUNDARY_GAP_MS,
            next_ms - MIN_BOUNDARY_GAP_MS
        )));
    }

    let updated = sqlx::query_as::<_, Boundary>(
        r#"
        UPDATE boundaries
        SET time_ms = $3,
            source = 'manual',
            flags = '{}',
            transition = COALESCE($4, transition),
            updated_at = now()
        WHERE project_id = $1 AND index = $2
        RETURNING id, project_id, index, time_ms, source, confidence, flags,
                  transition, created_at, updated_at
        "#,
    )
    .bind(project_id)
    .bind(index)
    .bind(new_time_ms)
    .bind(transition)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(updated)
}

/// Atomically updates multiple interior boundaries inside one transaction while enforcing
/// strict ordering, locked start/end boundaries, and the 0.5s minimum scene length invariant.
pub async fn batch_update_boundaries(
    pool: &PgPool,
    project_id: Uuid,
    updates: Vec<(i32, i32, Option<serde_json::Value>)>,
) -> Result<Vec<Boundary>, DbError> {
    if updates.is_empty() {
        return Err(DbError::Validation(
            "At least one boundary update is required".to_string(),
        ));
    }

    let mut tx = pool.begin().await?;

    let mut all = sqlx::query_as::<_, Boundary>(
        r#"
        SELECT id, project_id, index, time_ms, source, confidence, flags,
               transition, created_at, updated_at
        FROM boundaries
        WHERE project_id = $1
        ORDER BY index ASC
        FOR UPDATE
        "#,
    )
    .bind(project_id)
    .fetch_all(&mut *tx)
    .await?;

    if all.len() < 2 {
        return Err(DbError::NotFound);
    }

    let last_pos = all.len() - 1;
    let mut staged: Vec<(i32, i32, Option<serde_json::Value>)> = Vec::with_capacity(updates.len());

    for (idx, new_ms, transition) in updates {
        let pos = all
            .iter()
            .position(|b| b.index == idx)
            .ok_or(DbError::NotFound)?;
        if pos == 0 || pos == last_pos {
            return Err(DbError::Validation(
                "Start and end project boundaries cannot be moved manually".to_string(),
            ));
        }
        all[pos].time_ms = new_ms;
        staged.push((idx, new_ms, transition));
    }

    for pair in all.windows(2) {
        if pair[1].time_ms - pair[0].time_ms < MIN_BOUNDARY_GAP_MS {
            return Err(DbError::Validation(format!(
                "Scene between boundary {} and {} violates minimum 500ms duration",
                pair[0].index, pair[1].index
            )));
        }
    }

    for (idx, new_ms, transition) in staged {
        sqlx::query(
            r#"
            UPDATE boundaries
            SET time_ms = $3,
                source = 'manual',
                flags = '{}',
                transition = COALESCE($4, transition),
                updated_at = now()
            WHERE project_id = $1 AND index = $2
            "#,
        )
        .bind(project_id)
        .bind(idx)
        .bind(new_ms)
        .bind(transition)
        .execute(&mut *tx)
        .await?;
    }

    let refreshed = sqlx::query_as::<_, Boundary>(
        r#"
        SELECT id, project_id, index, time_ms, source, confidence, flags,
               transition, created_at, updated_at
        FROM boundaries
        WHERE project_id = $1
        ORDER BY index ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(refreshed)
}

pub async fn list_subtitle_cues(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Vec<SubtitleCue>, DbError> {
    let rows = sqlx::query_as::<_, SubtitleCue>(
        r#"
        SELECT id, project_id, position, start_ms, end_ms, text, edited,
               created_at, updated_at
        FROM subtitle_cues
        WHERE project_id = $1
        ORDER BY position ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_subtitle_cue(
    pool: &PgPool,
    project_id: Uuid,
    cue_id: Uuid,
    text: &str,
    start_ms: Option<i32>,
    end_ms: Option<i32>,
) -> Result<Option<SubtitleCue>, DbError> {
    if text.trim().is_empty() {
        return Err(DbError::Validation(
            "Subtitle cue text cannot be empty".to_string(),
        ));
    }
    if let (Some(s), Some(e)) = (start_ms, end_ms) {
        if e <= s {
            return Err(DbError::Validation(
                "Subtitle end time must be after start time".to_string(),
            ));
        }
    }

    let row = sqlx::query_as::<_, SubtitleCue>(
        r#"
        UPDATE subtitle_cues
        SET text = $3,
            start_ms = COALESCE($4, start_ms),
            end_ms = COALESCE($5, end_ms),
            edited = true,
            updated_at = now()
        WHERE id = $1 AND project_id = $2
        RETURNING id, project_id, position, start_ms, end_ms, text, edited,
                  created_at, updated_at
        "#,
    )
    .bind(cue_id)
    .bind(project_id)
    .bind(text.trim())
    .bind(start_ms)
    .bind(end_ms)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Replaces the ordered list of subtitle cues for a project atomically (used by split/merge).
pub async fn replace_project_subtitle_cues(
    pool: &PgPool,
    project_id: Uuid,
    cues: Vec<(Option<Uuid>, i32, i32, String, bool)>,
) -> Result<Vec<SubtitleCue>, DbError> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM subtitle_cues WHERE project_id = $1")
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

    let mut out = Vec::with_capacity(cues.len());
    for (idx, (id_opt, start_ms, end_ms, text, edited)) in cues.into_iter().enumerate() {
        let cue_id = id_opt.unwrap_or_else(Uuid::now_v7);
        let row = sqlx::query_as::<_, SubtitleCue>(
            r#"
            INSERT INTO subtitle_cues (id, project_id, position, start_ms, end_ms, text, edited)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, project_id, position, start_ms, end_ms, text, edited,
                      created_at, updated_at
            "#,
        )
        .bind(cue_id)
        .bind(project_id)
        .bind(idx as i32)
        .bind(start_ms)
        .bind(end_ms)
        .bind(text)
        .bind(edited)
        .fetch_one(&mut *tx)
        .await?;
        out.push(row);
    }

    tx.commit().await?;
    Ok(out)
}

/// Finds the active background music `media_assets` record (`kind = 'music'`) for a project.
pub async fn find_music_media_by_project(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Option<MediaAsset>, DbError> {
    let row = sqlx::query_as::<_, MediaAsset>(
        r#"
        SELECT id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
               original_filename, mime, bytes, sha256, width, height, duration_ms,
               source, prompt, status, probe, created_at, updated_at
        FROM media_assets
        WHERE project_id = $1 AND kind = 'music'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Deletes all background music `media_assets` for a project and reclaims user storage quota.
pub async fn delete_project_music(
    pool: &PgPool,
    project_id: Uuid,
    owner_id: Uuid,
) -> Result<Vec<MediaAsset>, DbError> {
    let mut tx = pool.begin().await?;

    let deleted_assets = sqlx::query_as::<_, MediaAsset>(
        r#"
        DELETE FROM media_assets
        WHERE project_id = $1 AND owner_id = $2 AND kind = 'music'
        RETURNING id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
                  original_filename, mime, bytes, sha256, width, height, duration_ms,
                  source, prompt, status, probe, created_at, updated_at
        "#,
    )
    .bind(project_id)
    .bind(owner_id)
    .fetch_all(&mut *tx)
    .await?;

    let freed_bytes: i64 = deleted_assets.iter().map(|a| a.bytes).sum();
    if freed_bytes > 0 {
        sqlx::query(
            r#"
            UPDATE users
            SET storage_used_bytes = GREATEST(0, storage_used_bytes - $2),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(owner_id)
        .bind(freed_bytes)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(deleted_assets)
}

/// Merges a JSON key/value into `projects.settings` without requiring optimistic version headers.
pub async fn patch_project_settings_key(
    pool: &PgPool,
    project_id: Uuid,
    owner_id: Uuid,
    key: &str,
    value: serde_json::Value,
) -> Result<serde_json::Value, DbError> {
    let updated: Option<serde_json::Value> = sqlx::query_scalar(
        r#"
        UPDATE projects
        SET settings = jsonb_set(COALESCE(settings, '{}'::jsonb), ARRAY[$3], $4, true),
            updated_at = now()
        WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
        RETURNING settings
        "#,
    )
    .bind(project_id)
    .bind(owner_id)
    .bind(key)
    .bind(value)
    .fetch_optional(pool)
    .await?;

    updated.ok_or(DbError::NotFound)
}

/// Removes all narration records, alignment boundaries, subtitle cues, transcripts,
/// and narration `media_assets` for a project, reclaiming user storage quota cleanly.
pub async fn delete_project_narration(
    pool: &PgPool,
    project_id: Uuid,
    owner_id: Uuid,
) -> Result<Vec<MediaAsset>, DbError> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM subtitle_cues WHERE project_id = $1")
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM boundaries WHERE project_id = $1")
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM transcripts WHERE project_id = $1")
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM narrations WHERE project_id = $1")
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

    let deleted_assets = sqlx::query_as::<_, MediaAsset>(
        r#"
        DELETE FROM media_assets
        WHERE project_id = $1 AND owner_id = $2 AND kind = 'narration'
        RETURNING id, project_id, owner_id, kind, storage_key, preview_key, thumb_key,
                  original_filename, mime, bytes, sha256, width, height, duration_ms,
                  source, prompt, status, probe, created_at, updated_at
        "#,
    )
    .bind(project_id)
    .bind(owner_id)
    .fetch_all(&mut *tx)
    .await?;

    let freed_bytes: i64 = deleted_assets.iter().map(|a| a.bytes).sum();
    if freed_bytes > 0 {
        sqlx::query(
            r#"
            UPDATE users
            SET storage_used_bytes = GREATEST(0, storage_used_bytes - $2),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(owner_id)
        .bind(freed_bytes)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(deleted_assets)
}
