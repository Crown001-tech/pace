use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pace_db::{Boundary as DbBoundary, MediaAsset, Project, SceneMedia, Section};
use pace_engine::{
    fit::{FitMode, SizeF},
    motion::{Easing, FocalPoint, Intensity, MotionPreset, SceneVisualEffect},
    spec::{compile_spec, CompileSpecInput, RenderSpec, SceneEffectSettings, SceneSpecInput},
    timing::{Boundary, BoundaryTransition, TransitionKind, MIN_SCENE_DURATION_MS},
};
use pace_storage::StorageKeys;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthContext,
    error::{ApiError, ProblemDetails},
    routes::music::{extract_include_tilt_in_auto, extract_stickers},
};

/// Validates framing `fit_mode` string (`"fill"` | `"fit_blur"`).
pub fn validate_fit_mode(mode: &str) -> Result<&str, ApiError> {
    match mode {
        "fill" | "fit_blur" => Ok(mode),
        _ => Err(ApiError::BadRequest(
            "Invalid fit_mode. Allowed values: 'fill', 'fit_blur'".to_string(),
        )),
    }
}

/// Validates normalized focal coordinate (`0.0..=1.0`).
pub fn validate_focal_coord(value: f32, axis: &str) -> Result<f32, ApiError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(ApiError::BadRequest(format!(
            "focal_{axis} must be a finite number between 0.0 and 1.0"
        )));
    }
    Ok(value)
}

pub fn parse_motion_preset(raw: &str) -> Result<MotionPreset, ApiError> {
    serde_json::from_value(serde_json::Value::String(raw.to_string())).map_err(|_| {
        ApiError::BadRequest(format!("Invalid motion preset '{raw}'"))
    })
}

pub fn motion_preset_to_str(preset: MotionPreset) -> String {
    serde_json::to_value(preset)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "still".to_string())
}

pub fn parse_intensity(raw: &str) -> Result<Intensity, ApiError> {
    serde_json::from_value(serde_json::Value::String(raw.to_string())).map_err(|_| {
        ApiError::BadRequest("Invalid intensity. Allowed values: 'subtle', 'normal', 'strong'".to_string())
    })
}

pub fn intensity_to_str(intensity: Intensity) -> String {
    serde_json::to_value(intensity)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "normal".to_string())
}

pub fn parse_easing(raw: &str) -> Result<Easing, ApiError> {
    serde_json::from_value(serde_json::Value::String(raw.to_string())).map_err(|_| {
        ApiError::BadRequest(
            "Invalid easing. Allowed values: 'steady', 'ease_in_out', 'punch_out', 'ramp_in', 'dynamic_curve'"
                .to_string(),
        )
    })
}

pub fn easing_to_str(easing: Easing) -> String {
    serde_json::to_value(easing)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "ease_in_out".to_string())
}

pub fn parse_visual_effect(raw: &str) -> Result<SceneVisualEffect, ApiError> {
    serde_json::from_value(serde_json::Value::String(raw.to_string())).map_err(|_| {
        ApiError::BadRequest(
            "Invalid visual_effect. Allowed values: 'none', 'handheld', 'camera_shake', 'zoom_lens'"
                .to_string(),
        )
    })
}

pub fn visual_effect_to_str(effect: SceneVisualEffect) -> String {
    serde_json::to_value(effect)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "none".to_string())
}

/// Validates and normalizes boundary transition JSON (`cut` / `crossfade` / `fade_black` / etc.).
pub fn validate_boundary_transition(
    value: Option<serde_json::Value>,
) -> Result<Option<serde_json::Value>, ApiError> {
    let Some(val) = value else {
        return Ok(None);
    };
    if val.is_null() {
        return Ok(Some(serde_json::json!({
            "kind": "cut",
            "duration_ms": 0
        })));
    }

    let mut parsed: BoundaryTransition = serde_json::from_value(val).map_err(|_| {
        ApiError::BadRequest(
            "Invalid transition object. Expected { kind: 'cut' | 'crossfade' | 'fade_black' | 'shutter_flash' | 'zoom_through' | 'whip_pan_left' | 'whip_pan_right' | 'slide_left' | 'slide_right' | 'slide_up' | 'slide_down', duration_ms: number }".to_string(),
        )
    })?;

    match parsed.kind {
        TransitionKind::Cut => {
            parsed.duration_ms = 0;
        }
        TransitionKind::Crossfade
        | TransitionKind::FadeBlack
        | TransitionKind::ShutterFlash
        | TransitionKind::ZoomThrough
        | TransitionKind::WhipPanLeft
        | TransitionKind::WhipPanRight
        | TransitionKind::SlideLeft
        | TransitionKind::SlideRight
        | TransitionKind::SlideUp
        | TransitionKind::SlideDown => {
            if parsed.duration_ms < 100 || parsed.duration_ms > 3000 {
                return Err(ApiError::BadRequest(
                    "Transition duration_ms must be between 100 and 3000".to_string(),
                ));
            }
        }
    }

    serde_json::to_value(parsed)
        .map(Some)
        .map_err(|e| ApiError::Internal(e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SceneEffectDto {
    pub manual_preset: Option<String>,
    pub intensity: String,
    pub easing: String,
    pub manual_shake: Option<bool>,
    #[serde(default = "default_visual_effect_str")]
    pub visual_effect: String,
}

fn default_visual_effect_str() -> String {
    "none".to_string()
}

impl Default for SceneEffectDto {
    fn default() -> Self {
        Self {
            manual_preset: None,
            intensity: "normal".to_string(),
            easing: "ease_in_out".to_string(),
            manual_shake: None,
            visual_effect: "none".to_string(),
        }
    }
}

impl SceneEffectDto {
    pub fn from_json(value: &serde_json::Value) -> Self {
        let parsed: SceneEffectSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        Self {
            manual_preset: parsed.manual_preset.map(motion_preset_to_str),
            intensity: intensity_to_str(parsed.intensity),
            easing: easing_to_str(parsed.easing),
            manual_shake: parsed.manual_shake,
            visual_effect: visual_effect_to_str(parsed.visual_effect),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateSceneEffectRequest {
    pub manual_preset: Option<String>,
    pub clear_manual_preset: Option<bool>,
    pub intensity: Option<String>,
    pub easing: Option<String>,
    pub manual_shake: Option<bool>,
    pub clear_manual_shake: Option<bool>,
    pub visual_effect: Option<String>,
    pub fit_mode: Option<String>,
    pub focal_x: Option<f32>,
    pub focal_y: Option<f32>,
    pub reset_to_auto: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RenderSpecAssetDto {
    pub scene_index: usize,
    pub section_id: Uuid,
    pub media_id: Option<Uuid>,
    pub filename: Option<String>,
    pub preview_url: Option<String>,
    pub thumb_url: Option<String>,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RenderSpecResponse {
    pub project_id: Uuid,
    pub spec_hash: String,
    pub effect_seed: i64,
    pub fps: u32,
    pub frame_width: f32,
    pub frame_height: f32,
    pub total_frames: u32,
    pub duration_ms: u64,
    pub audio_url: Option<String>,
    pub scene_assets: Vec<RenderSpecAssetDto>,
    #[schema(value_type = Object)]
    pub spec: RenderSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RerollEffectsResponse {
    pub project_id: Uuid,
    pub effect_seed: i64,
    pub spec_hash: String,
}

pub fn frame_size_for_aspect(aspect_ratio: &str) -> SizeF {
    if aspect_ratio == "9:16" {
        SizeF::new(1080.0, 1920.0)
    } else {
        SizeF::new(1920.0, 1080.0)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn compile_project_spec_from_records(
    project: &Project,
    sections: &[Section],
    media_assets: &[MediaAsset],
    assignments: &[SceneMedia],
    db_boundaries: &[DbBoundary],
    narration_duration_ms: Option<i32>,
) -> Result<Option<RenderSpec>, ApiError> {
    if sections.is_empty() {
        return Ok(None);
    }

    let frame_size = frame_size_for_aspect(&project.aspect_ratio);
    let fps = project.fps.max(1) as u32;
    let scene_count = sections.len();

    let (boundaries, narration_ms) = if db_boundaries.len() == scene_count + 1 {
        let last_ms = db_boundaries
            .last()
            .map(|b| b.time_ms.max(0) as u64)
            .unwrap_or(0);
        let mut engine_bounds = Vec::with_capacity(db_boundaries.len());
        for (idx, b) in db_boundaries.iter().enumerate() {
            let transition = if idx == 0 || idx == scene_count {
                None
            } else {
                serde_json::from_value::<BoundaryTransition>(b.transition.clone()).ok()
            };
            let next_ms = db_boundaries
                .get(idx + 1)
                .map(|nb| nb.time_ms.max(0) as u64)
                .unwrap_or(last_ms);
            let cur_ms = if idx == 0 { 0 } else { b.time_ms.max(0) as u64 };
            let allow_zero = idx < scene_count && next_ms.saturating_sub(cur_ms) < MIN_SCENE_DURATION_MS;
            engine_bounds.push(Boundary {
                time_ms: cur_ms,
                allow_zero_length: allow_zero,
                transition,
            });
        }
        (engine_bounds, last_ms)
    } else {
        let total_ms = narration_duration_ms
            .filter(|&d| (d as usize) >= scene_count * 500)
            .map(|d| d as u64)
            .unwrap_or_else(|| (scene_count as u64) * 12_000);
        let slice_ms = total_ms / (scene_count as u64);
        let mut engine_bounds = Vec::with_capacity(scene_count + 1);
        for idx in 0..scene_count {
            engine_bounds.push(Boundary::new((idx as u64) * slice_ms));
        }
        engine_bounds.push(Boundary::new(total_ms));
        (engine_bounds, total_ms)
    };

    let media_map: HashMap<Uuid, &MediaAsset> = media_assets.iter().map(|m| (m.id, m)).collect();
    let assign_map: HashMap<Uuid, &SceneMedia> =
        assignments.iter().map(|a| (a.section_id, a)).collect();

    let scenes: Vec<SceneSpecInput> = sections
        .iter()
        .enumerate()
        .map(|(idx, sec)| {
            let assign = assign_map.get(&sec.id).copied();
            let media = assign.and_then(|a| media_map.get(&a.media_id).copied());
            let fit_mode = match assign.map(|a| a.fit_mode.as_str()) {
                Some("fit_blur") => FitMode::FitBlur,
                _ => FitMode::Fill,
            };
            let focal = FocalPoint {
                x: assign.map(|a| a.focal_x).unwrap_or(0.5).clamp(0.0, 1.0),
                y: assign.map(|a| a.focal_y).unwrap_or(0.5).clamp(0.0, 1.0),
            };
            let effect: SceneEffectSettings = assign
                .and_then(|a| serde_json::from_value(a.effect.clone()).ok())
                .unwrap_or_default();
            let image_size = match (media.and_then(|m| m.width), media.and_then(|m| m.height)) {
                (Some(w), Some(h)) if w > 0 && h > 0 => SizeF::new(w as f32, h as f32),
                _ => frame_size,
            };

            SceneSpecInput {
                scene_index: idx,
                section_id: sec.id.to_string(),
                media_id: media.map(|m| m.id.to_string()),
                image_size,
                fit_mode,
                focal,
                effect,
                effect_overridden: assign.map(|a| a.effect_overridden).unwrap_or(false),
            }
        })
        .collect();

    let input = CompileSpecInput {
        fps,
        frame_size,
        narration_ms,
        effect_seed: project.effect_seed.max(0) as u64,
        include_tilt_in_auto: extract_include_tilt_in_auto(&project.settings),
        boundaries,
        scenes,
        stickers: extract_stickers(&project.settings),
    };

    let compiled = compile_spec(&input).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Some(compiled))
}

#[allow(clippy::too_many_arguments)]
pub fn build_render_spec_response(
    state: &AppState,
    user_id: Uuid,
    project: &Project,
    sections: &[Section],
    media_assets: &[MediaAsset],
    assignments: &[SceneMedia],
    narration_media: Option<&MediaAsset>,
    narration: Option<&pace_db::Narration>,
    _db_boundaries: &[DbBoundary],
    spec: RenderSpec,
) -> RenderSpecResponse {
    let ttl = Duration::from_secs(3600);
    let playback_key = if let Some(nar) = narration {
        Some(StorageKeys::audio_opus(&user_id, &project.id, &nar.media_id))
    } else {
        narration_media.map(|m| m.storage_key.clone())
    };
    let audio_url = playback_key
        .as_deref()
        .and_then(|k| state.storage.presign_get(k, ttl, None).ok())
        .map(|u| u.to_string());

    let media_map: HashMap<Uuid, &MediaAsset> = media_assets.iter().map(|m| (m.id, m)).collect();
    let assign_map: HashMap<Uuid, &SceneMedia> =
        assignments.iter().map(|a| (a.section_id, a)).collect();

    let scene_assets = sections
        .iter()
        .enumerate()
        .map(|(idx, sec)| {
            let assign = assign_map.get(&sec.id).copied();
            let media = assign.and_then(|a| media_map.get(&a.media_id).copied());
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
            let compiled = &spec.scenes[idx];
            RenderSpecAssetDto {
                scene_index: idx,
                section_id: sec.id,
                media_id: media.map(|m| m.id),
                filename: media.map(|m| m.original_filename.clone()),
                preview_url,
                thumb_url,
                width: compiled.image_size.w,
                height: compiled.image_size.h,
            }
        })
        .collect();

    RenderSpecResponse {
        project_id: project.id,
        spec_hash: spec.spec_hash.clone(),
        effect_seed: project.effect_seed,
        fps: spec.fps,
        frame_width: spec.frame_size.w,
        frame_height: spec.frame_size.h,
        total_frames: spec.total_frames,
        duration_ms: spec.narration_ms,
        audio_url,
        scene_assets,
        spec,
    }
}

/// GET /api/v1/projects/{id}/render-spec
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/render-spec",
    tag = "Effects",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Compiled deterministic RenderSpec with signed preview asset URLs", body = RenderSpecResponse),
        (status = 400, description = "Project missing sections or invalid timing", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn get_render_spec(
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
    let narration = pace_db::get_narration(&state.db, project_id).await?;
    let db_boundaries = pace_db::list_boundaries(&state.db, project_id).await?;

    let spec = compile_project_spec_from_records(
        &project,
        &sections,
        &media_assets,
        &assignments,
        &db_boundaries,
        narration.as_ref().map(|n| n.duration_ms),
    )?
    .ok_or_else(|| {
        ApiError::BadRequest(
            "Project must have at least one script section to compile render spec".to_string(),
        )
    })?;

    let response = build_render_spec_response(
        &state,
        auth.user.id,
        &project,
        &sections,
        &media_assets,
        &assignments,
        narration_media.as_ref(),
        narration.as_ref(),
        &db_boundaries,
        spec,
    );

    Ok((StatusCode::OK, Json(response)))
}

/// PATCH /api/v1/projects/{id}/assignments/{section_id}/effect
#[utoipa::path(
    patch,
    path = "/api/v1/projects/{id}/assignments/{section_id}/effect",
    tag = "Effects",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("section_id" = Uuid, Path, description = "Section UUID")
    ),
    request_body = UpdateSceneEffectRequest,
    responses(
        (status = 200, description = "Updated scene motion effect and framing", body = crate::routes::assignments::AssignmentResponse),
        (status = 400, description = "Invalid effect parameter or unassigned section", body = ProblemDetails),
        (status = 404, description = "Project or section not found", body = ProblemDetails)
    )
)]
pub async fn update_scene_effect(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, section_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateSceneEffectRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let existing = pace_db::get_assignment_by_section(&state.db, project_id, section_id)
        .await?
        .ok_or_else(|| {
            ApiError::BadRequest(
                "Assign an image to this scene before customizing its framing or motion effects"
                    .to_string(),
            )
        })?;

    let fit_mode = match req.fit_mode.as_deref() {
        Some(mode) => Some(validate_fit_mode(mode)?),
        None => None,
    };
    let focal_x = match req.focal_x {
        Some(x) => Some(validate_focal_coord(x, "x")?),
        None => None,
    };
    let focal_y = match req.focal_y {
        Some(y) => Some(validate_focal_coord(y, "y")?),
        None => None,
    };

    let reset = req.reset_to_auto == Some(true);
    let mut settings: SceneEffectSettings = if reset {
        SceneEffectSettings::default()
    } else {
        serde_json::from_value(existing.effect.clone()).unwrap_or_default()
    };

    if !reset {
        if req.clear_manual_preset == Some(true) {
            settings.manual_preset = None;
        } else if let Some(ref raw_preset) = req.manual_preset {
            settings.manual_preset = Some(parse_motion_preset(raw_preset)?);
        }

        if let Some(ref raw_intensity) = req.intensity {
            settings.intensity = parse_intensity(raw_intensity)?;
        }

        if let Some(ref raw_easing) = req.easing {
            settings.easing = parse_easing(raw_easing)?;
        }

        if req.clear_manual_shake == Some(true) {
            settings.manual_shake = None;
        } else if let Some(shake) = req.manual_shake {
            settings.manual_shake = Some(shake);
        }

        if let Some(ref raw_visual) = req.visual_effect {
            settings.visual_effect = parse_visual_effect(raw_visual)?;
        }
    }

    let has_effect_override = if reset {
        false
    } else {
        settings.manual_preset.is_some()
            || settings.manual_shake.is_some()
            || settings.intensity != Intensity::Normal
            || settings.easing != Easing::EaseInOut
            || settings.visual_effect != SceneVisualEffect::None
    };

    let effect_json =
        serde_json::to_value(&settings).map_err(|e| ApiError::Internal(e.to_string()))?;

    let updated = pace_db::update_scene_effect(
        &state.db,
        project_id,
        section_id,
        fit_mode,
        focal_x,
        focal_y,
        Some(effect_json),
        Some(has_effect_override),
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(crate::routes::assignments::AssignmentResponse::from(updated)),
    ))
}

/// POST /api/v1/projects/{id}/effects/reroll
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/effects/reroll",
    tag = "Effects",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Re-rolled automatic presets for non-overridden scenes", body = RerollEffectsResponse),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn reroll_effects(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let new_seed = pace_db::reroll_project_effects(
        &state.db,
        project_id,
        auth.user.id,
        |seed| {
            let next = pace_engine::effects::reroll_seed(seed.max(0) as u64);
            (next & (i64::MAX as u64)) as i64
        },
    )
    .await?;

    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let sections = pace_db::get_sections_by_project(&state.db, project_id).await?;
    let media_assets = pace_db::find_by_project(&state.db, project_id).await?;
    let assignments = pace_db::get_assignments_by_project(&state.db, project_id).await?;
    let narration = pace_db::get_narration(&state.db, project_id).await?;
    let db_boundaries = pace_db::list_boundaries(&state.db, project_id).await?;

    let spec_hash = compile_project_spec_from_records(
        &project,
        &sections,
        &media_assets,
        &assignments,
        &db_boundaries,
        narration.as_ref().map(|n| n.duration_ms),
    )?
    .map(|s| s.spec_hash)
    .unwrap_or_default();

    Ok((
        StatusCode::OK,
        Json(RerollEffectsResponse {
            project_id,
            effect_seed: new_seed,
            spec_hash,
        }),
    ))
}
