use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pace_db::SilenceGap;
use pace_engine::stickers::StickerPlacementSpec;
use pace_storage::StorageKeys;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthContext,
    error::{ApiError, ProblemDetails},
    routes::effects::{
        compile_project_spec_from_records, motion_preset_to_str, validate_boundary_transition,
        SceneEffectDto,
    },
    routes::music::{
        extract_include_tilt_in_auto, extract_music_settings, extract_stickers,
        extract_subtitle_style, MusicSettingsDto, SubtitleStyleDto,
    },
    routes::narration::{BoundaryDto, SilenceGapDto, SubtitleCueDto},
};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TimelineSceneDto {
    pub index: i32,
    pub section_id: Uuid,
    pub text: String,
    pub start_ms: i32,
    pub end_ms: i32,
    pub duration_ms: i32,
    pub media_id: Option<Uuid>,
    pub filename: Option<String>,
    pub thumb_url: Option<String>,
    pub preview_url: Option<String>,
    pub fit_mode: String,
    pub focal_x: f32,
    pub focal_y: f32,
    pub effect: SceneEffectDto,
    pub effect_overridden: bool,
    pub resolved_preset: String,
    pub resolved_has_shake: bool,
    pub flags: Vec<String>,
    pub confidence: Option<f32>,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TimelineStateResponse {
    pub project_id: Uuid,
    pub aspect_ratio: String,
    pub fps: i32,
    pub effect_seed: i64,
    pub duration_ms: i32,
    pub audio_url: Option<String>,
    pub audio_filename: Option<String>,
    pub has_peaks: bool,
    pub silences: Vec<SilenceGapDto>,
    pub boundaries: Vec<BoundaryDto>,
    pub flagged_boundary_indices: Vec<i32>,
    pub scenes: Vec<TimelineSceneDto>,
    pub subtitle_cues: Vec<SubtitleCueDto>,
    pub subtitle_style: SubtitleStyleDto,
    pub music_url: Option<String>,
    pub music_filename: Option<String>,
    pub music_settings: MusicSettingsDto,
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    pub stickers: Vec<StickerPlacementSpec>,
    #[serde(default)]
    pub include_tilt_in_auto: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchBoundaryItem {
    pub index: i32,
    pub time_ms: i32,
    pub transition: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchUpdateBoundariesRequest {
    pub boundaries: Vec<BatchBoundaryItem>,
}

/// GET /api/v1/projects/{id}/timeline
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/timeline",
    tag = "Timeline",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Complete timeline state with scenes, boundaries, flags, and audio metadata", body = TimelineStateResponse),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn get_timeline_state(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let sections = pace_db::get_sections_by_project(&state.db, project_id).await?;
    let media_assets = pace_db::find_by_project(&state.db, project_id).await?;
    let assignments = pace_db::get_assignments_by_project(&state.db, project_id).await?;
    let narration_media = pace_db::find_narration_media_by_project(&state.db, project_id).await?;
    let music_media = pace_db::find_music_media_by_project(&state.db, project_id).await?;
    let narration = pace_db::get_narration(&state.db, project_id).await?;
    let db_boundaries = pace_db::list_boundaries(&state.db, project_id).await?;
    let subtitle_cues = pace_db::list_subtitle_cues(&state.db, project_id).await?;

    let ttl = Duration::from_secs(3600);
    let playback_key = if let Some(ref nar) = narration {
        Some(StorageKeys::audio_opus(&auth.user.id, &project_id, &nar.media_id))
    } else {
        narration_media.as_ref().map(|m| m.storage_key.clone())
    };
    let audio_url = playback_key
        .as_deref()
        .and_then(|k| state.storage.presign_get(k, ttl, None).ok())
        .map(|u| u.to_string());
    let music_url = music_media
        .as_ref()
        .and_then(|m| state.storage.presign_get(&m.storage_key, ttl, None).ok())
        .map(|u| u.to_string());
    let subtitle_style = SubtitleStyleDto::from(extract_subtitle_style(&project.settings));
    let music_settings = MusicSettingsDto::from(extract_music_settings(&project.settings));

    let silences: Vec<SilenceGapDto> = narration
        .as_ref()
        .and_then(|n| serde_json::from_value::<Vec<SilenceGap>>(n.silences.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|s| SilenceGapDto {
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            duration_ms: s.duration_ms,
        })
        .collect();

    let total_duration_ms = narration
        .as_ref()
        .map(|n| n.duration_ms)
        .or_else(|| db_boundaries.last().map(|b| b.time_ms))
        .unwrap_or_else(|| (sections.len().max(1) as i32) * 12_000);

    let compiled_spec = compile_project_spec_from_records(
        &project,
        &sections,
        &media_assets,
        &assignments,
        &db_boundaries,
        narration.as_ref().map(|n| n.duration_ms),
    )
    .ok()
    .flatten();

    let boundaries: Vec<BoundaryDto> = db_boundaries
        .into_iter()
        .map(|b| BoundaryDto {
            id: b.id,
            index: b.index,
            time_ms: b.time_ms,
            source: b.source,
            confidence: b.confidence,
            flags: b.flags,
            transition: b.transition,
        })
        .collect();

    let media_map: HashMap<Uuid, _> = media_assets
        .into_iter()
        .filter(|m| m.kind == "image")
        .map(|m| (m.id, m))
        .collect();
    let assign_map: HashMap<Uuid, _> = assignments
        .into_iter()
        .map(|a| (a.section_id, a))
        .collect();

    let scene_count = sections.len().max(1);
    let mut scenes = Vec::with_capacity(sections.len());
    for (idx, sec) in sections.into_iter().enumerate() {
        let b_start = boundaries.get(idx);
        let b_end = boundaries.get(idx + 1);
        let fallback_slice = total_duration_ms / (scene_count as i32);
        let start_ms = b_start.map(|b| b.time_ms).unwrap_or((idx as i32) * fallback_slice);
        let end_ms = b_end
            .map(|b| b.time_ms)
            .unwrap_or(((idx + 1) as i32) * fallback_slice);

        let assign = assign_map.get(&sec.id);
        let media = assign.and_then(|a| media_map.get(&a.media_id));
        let thumb_url = media.and_then(|m| {
            m.thumb_key
                .as_deref()
                .or(Some(m.storage_key.as_str()))
                .and_then(|k| state.storage.presign_get(k, ttl, None).ok())
                .map(|u| u.to_string())
        });
        let preview_url = media.and_then(|m| {
            m.preview_key
                .as_deref()
                .or(Some(m.storage_key.as_str()))
                .and_then(|k| state.storage.presign_get(k, ttl, None).ok())
                .map(|u| u.to_string())
        });

        let compiled_scene = compiled_spec.as_ref().and_then(|s| s.scenes.get(idx));
        let resolved_preset = compiled_scene
            .map(|cs| motion_preset_to_str(cs.preset))
            .unwrap_or_else(|| "still".to_string());
        let resolved_has_shake = compiled_scene.map(|cs| cs.has_shake).unwrap_or(false);

        scenes.push(TimelineSceneDto {
            index: idx as i32,
            section_id: sec.id,
            text: sec.text,
            start_ms,
            end_ms,
            duration_ms: (end_ms - start_ms).max(0),
            media_id: media.map(|m| m.id),
            filename: media.map(|m| m.original_filename.clone()),
            thumb_url,
            preview_url,
            fit_mode: assign
                .map(|a| a.fit_mode.clone())
                .unwrap_or_else(|| "fill".to_string()),
            focal_x: assign.map(|a| a.focal_x).unwrap_or(0.5),
            focal_y: assign.map(|a| a.focal_y).unwrap_or(0.5),
            effect: assign
                .map(|a| SceneEffectDto::from_json(&a.effect))
                .unwrap_or_default(),
            effect_overridden: assign.map(|a| a.effect_overridden).unwrap_or(false),
            resolved_preset,
            resolved_has_shake,
            flags: b_start.map(|b| b.flags.clone()).unwrap_or_default(),
            confidence: b_start.and_then(|b| b.confidence),
            source: b_start
                .map(|b| b.source.clone())
                .unwrap_or_else(|| "auto".to_string()),
        });
    }

    let flagged_boundary_indices: Vec<i32> = boundaries
        .iter()
        .filter(|b| !b.flags.is_empty())
        .map(|b| b.index)
        .collect();

    let stickers = extract_stickers(&project.settings);
    let include_tilt_in_auto = extract_include_tilt_in_auto(&project.settings);

    Ok((
        StatusCode::OK,
        Json(TimelineStateResponse {
            project_id,
            aspect_ratio: project.aspect_ratio,
            fps: project.fps,
            effect_seed: project.effect_seed,
            duration_ms: total_duration_ms,
            audio_url,
            audio_filename: narration_media.map(|m| m.original_filename),
            has_peaks: narration.as_ref().and_then(|n| n.peaks_key.as_ref()).is_some(),
            silences,
            boundaries,
            flagged_boundary_indices,
            scenes,
            subtitle_cues: subtitle_cues
                .into_iter()
                .map(|c| SubtitleCueDto {
                    id: c.id,
                    position: c.position,
                    start_ms: c.start_ms,
                    end_ms: c.end_ms,
                    text: c.text,
                    edited: c.edited,
                })
                .collect(),
            subtitle_style,
            music_url,
            music_filename: music_media.map(|m| m.original_filename),
            music_settings,
            stickers,
            include_tilt_in_auto,
        }),
    ))
}

/// PATCH /api/v1/projects/{id}/boundaries
#[utoipa::path(
    patch,
    path = "/api/v1/projects/{id}/boundaries",
    tag = "Timeline",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = BatchUpdateBoundariesRequest,
    responses(
        (status = 200, description = "Batch boundary updates committed", body = Vec<BoundaryDto>),
        (status = 400, description = "Invalid boundary ordering or minimum duration violation", body = ProblemDetails),
        (status = 404, description = "Project or boundary not found", body = ProblemDetails)
    )
)]
pub async fn batch_patch_boundaries(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<BatchUpdateBoundariesRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let mut items = Vec::with_capacity(req.boundaries.len());
    for b in req.boundaries {
        let transition = validate_boundary_transition(b.transition)?;
        items.push((b.index, b.time_ms, transition));
    }

    let updated = pace_db::batch_update_boundaries(&state.db, project_id, items).await?;
    let dtos: Vec<BoundaryDto> = updated
        .into_iter()
        .map(|b| BoundaryDto {
            id: b.id,
            index: b.index,
            time_ms: b.time_ms,
            source: b.source,
            confidence: b.confidence,
            flags: b.flags,
            transition: b.transition,
        })
        .collect();

    Ok((StatusCode::OK, Json(dtos)))
}
