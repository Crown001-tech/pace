use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pace_db::{CreateMediaAsset, SubtitleCue};
use pace_engine::stickers::StickerPlacementSpec;
use pace_engine::subtitles::{
    max_line_chars_for_aspect, merge_subtitle_cues, split_subtitle_cue, validate_subtitle_cue,
    wrap_subtitle_lines, MusicSettingsSpec, SubtitleCueSpec, SubtitleFont, SubtitlePosition,
    SubtitleStyleSpec,
};
use pace_storage::StorageKeys;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthContext,
    error::{ApiError, ProblemDetails},
    routes::narration::SubtitleCueDto,
};

const MAX_MUSIC_BYTES: i64 = 100 * 1024 * 1024; // 100 MB background music upload cap

fn allowed_music_extension(content_type: &str) -> Option<&'static str> {
    let clean = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match clean.as_str() {
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => Some("m4a"),
        "audio/aac" => Some("aac"),
        "audio/ogg" | "audio/opus" => Some("ogg"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/webm" => Some("webm"),
        _ => None,
    }
}

fn validate_project_music_key(key: &str, user_id: Uuid, project_id: Uuid) -> bool {
    let expected_prefix = StorageKeys::project_music_prefix(&user_id, &project_id);
    key.starts_with(&expected_prefix) && !key.contains("..") && !key.contains('\\')
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MusicSettingsDto {
    /// Background music volume in dB (`-40.0` to `0.0`).
    pub volume_db: f32,
    /// Automatically duck music under narration speech.
    pub ducking_enabled: bool,
    /// Ducking attenuation in dB (`-18.0` to `-4.0`).
    pub duck_db: f32,
    /// Loop music track to fill the entire video duration.
    pub loop_enabled: bool,
    /// Crossfade duration between loops in milliseconds (`0..=5000`).
    pub crossfade_ms: u32,
    /// Fade-in duration at the start of the video in milliseconds (`0..=15000`).
    pub fade_in_ms: u32,
    /// Fade-out duration at the end of the video in milliseconds (`0..=15000`).
    pub fade_out_ms: u32,
}

impl Default for MusicSettingsDto {
    fn default() -> Self {
        Self::from(MusicSettingsSpec::default())
    }
}

impl From<MusicSettingsSpec> for MusicSettingsDto {
    fn from(s: MusicSettingsSpec) -> Self {
        Self {
            volume_db: s.volume_db,
            ducking_enabled: s.ducking_enabled,
            duck_db: s.duck_amount_db,
            loop_enabled: s.loop_enabled,
            crossfade_ms: s.loop_crossfade_ms,
            fade_in_ms: s.fade_in_ms,
            fade_out_ms: s.fade_out_ms,
        }
    }
}

impl From<MusicSettingsDto> for MusicSettingsSpec {
    fn from(d: MusicSettingsDto) -> Self {
        Self {
            enabled: true,
            media_id: None,
            volume_db: d.volume_db.clamp(-40.0, 0.0),
            ducking_enabled: d.ducking_enabled,
            duck_amount_db: d.duck_db.clamp(-18.0, -4.0),
            loop_enabled: d.loop_enabled,
            loop_crossfade_ms: d.crossfade_ms.min(5_000),
            fade_in_ms: d.fade_in_ms.min(15_000),
            fade_out_ms: d.fade_out_ms.min(15_000),
        }
    }
}

pub fn extract_music_settings(settings_json: &serde_json::Value) -> MusicSettingsSpec {
    settings_json
        .get("music")
        .and_then(|v| serde_json::from_value::<MusicSettingsSpec>(v.clone()).ok())
        .unwrap_or_default()
}

pub fn extract_subtitle_style(settings_json: &serde_json::Value) -> SubtitleStyleSpec {
    settings_json
        .get("subtitle_style")
        .and_then(|v| serde_json::from_value::<SubtitleStyleSpec>(v.clone()).ok())
        .unwrap_or_default()
}

pub fn extract_stickers(settings_json: &serde_json::Value) -> Vec<StickerPlacementSpec> {
    settings_json
        .get("stickers")
        .and_then(|v| serde_json::from_value::<Vec<StickerPlacementSpec>>(v.clone()).ok())
        .unwrap_or_default()
}

pub fn extract_include_tilt_in_auto(settings_json: &serde_json::Value) -> bool {
    settings_json
        .get("include_tilt_in_auto")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MusicStateResponse {
    pub project_id: Uuid,
    pub media_id: Option<Uuid>,
    pub filename: Option<String>,
    pub bytes: Option<i64>,
    pub audio_url: Option<String>,
    pub settings: MusicSettingsDto,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PresignMusicRequest {
    pub filename: String,
    pub content_type: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PresignMusicResponse {
    pub media_id: Uuid,
    pub storage_key: String,
    pub upload_url: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterMusicRequest {
    pub media_id: Option<Uuid>,
    pub storage_key: String,
    pub original_filename: String,
    pub mime: String,
    pub bytes: i64,
    pub sha256: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMusicSettingsRequest {
    pub volume_db: Option<f32>,
    pub ducking_enabled: Option<bool>,
    pub duck_db: Option<f32>,
    pub loop_enabled: Option<bool>,
    pub crossfade_ms: Option<u32>,
    pub fade_in_ms: Option<u32>,
    pub fade_out_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubtitleStyleDto {
    pub enabled: bool,
    pub font: String,
    pub size_pct: u16,
    pub position: String,
    pub text_color: String,
    pub outline_width_px: u8,
    pub bg_opacity_pct: u8,
}

impl Default for SubtitleStyleDto {
    fn default() -> Self {
        Self::from(SubtitleStyleSpec::default())
    }
}

impl From<SubtitleStyleSpec> for SubtitleStyleDto {
    fn from(s: SubtitleStyleSpec) -> Self {
        Self {
            enabled: s.enabled,
            font: match s.font_family {
                SubtitleFont::Sans => "sans".to_string(),
                SubtitleFont::Mono => "mono".to_string(),
            },
            size_pct: s.font_size_pct as u16,
            position: match s.position {
                SubtitlePosition::Bottom => "bottom".to_string(),
                SubtitlePosition::Center => "center".to_string(),
                SubtitlePosition::Top => "top".to_string(),
            },
            text_color: s.text_color,
            outline_width_px: s.outline_width_px as u8,
            bg_opacity_pct: s.bg_opacity_pct as u8,
        }
    }
}

impl SubtitleStyleDto {
    pub fn to_spec(&self) -> Result<SubtitleStyleSpec, ApiError> {
        let font_family = match self.font.trim().to_ascii_lowercase().as_str() {
            "sans" => SubtitleFont::Sans,
            "mono" => SubtitleFont::Mono,
            other => {
                return Err(ApiError::BadRequest(format!(
                    "Invalid subtitle font '{other}'. Allowed: 'sans', 'mono'"
                )))
            }
        };
        let position = match self.position.trim().to_ascii_lowercase().as_str() {
            "bottom" => SubtitlePosition::Bottom,
            "center" => SubtitlePosition::Center,
            "top" => SubtitlePosition::Top,
            other => {
                return Err(ApiError::BadRequest(format!(
                    "Invalid subtitle position '{other}'. Allowed: 'bottom', 'center', 'top'"
                )))
            }
        };
        if !(70..=150).contains(&self.size_pct) {
            return Err(ApiError::BadRequest(
                "Subtitle size_pct must be between 70 and 150".to_string(),
            ));
        }
        if self.outline_width_px > 6 {
            return Err(ApiError::BadRequest(
                "Subtitle outline_width_px must be between 0 and 6".to_string(),
            ));
        }
        if self.bg_opacity_pct > 100 {
            return Err(ApiError::BadRequest(
                "Subtitle bg_opacity_pct must be between 0 and 100".to_string(),
            ));
        }
        let color = self.text_color.trim();
        let hex_ok = color.starts_with('#')
            && color.len() == 7
            && color[1..].chars().all(|c| c.is_ascii_hexdigit());
        if !hex_ok {
            return Err(ApiError::BadRequest(
                "Subtitle text_color must be a 7-character hex color like '#FFFFFF'".to_string(),
            ));
        }

        Ok(SubtitleStyleSpec {
            enabled: self.enabled,
            font_family,
            font_size_pct: self.size_pct as u32,
            position,
            text_color: color.to_ascii_uppercase(),
            outline_width_px: self.outline_width_px as u32,
            bg_opacity_pct: self.bg_opacity_pct as u32,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubtitleCueDetailDto {
    pub id: Uuid,
    pub position: i32,
    pub start_ms: i32,
    pub end_ms: i32,
    pub duration_ms: i32,
    pub text: String,
    pub lines: Vec<String>,
    pub max_line_chars: usize,
    pub char_limit: usize,
    pub violations: Vec<String>,
    pub edited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubtitlesStateResponse {
    pub project_id: Uuid,
    pub aspect_ratio: String,
    pub max_chars_per_line: usize,
    pub style: SubtitleStyleDto,
    pub cues: Vec<SubtitleCueDetailDto>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SplitSubtitleCueRequest {
    /// Optional word index (`1..word_count`) or split time to split at; defaults to midpoint.
    pub split_word_index: Option<usize>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MergeSubtitleCuesRequest {
    pub first_cue_id: Uuid,
    pub second_cue_id: Uuid,
}

pub fn hydrate_subtitle_cue_detail(
    cue: &SubtitleCue,
    aspect_ratio: &str,
) -> SubtitleCueDetailDto {
    let spec = SubtitleCueSpec {
        id: cue.id.to_string(),
        position: cue.position.max(0) as usize,
        start_ms: cue.start_ms.max(0) as u64,
        end_ms: cue.end_ms.max(cue.start_ms) as u64,
        text: cue.text.clone(),
    };
    let violations = validate_subtitle_cue(&spec, aspect_ratio);
    let lines = wrap_subtitle_lines(&cue.text, aspect_ratio);
    let max_line_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let char_limit = max_line_chars_for_aspect(aspect_ratio);

    SubtitleCueDetailDto {
        id: cue.id,
        position: cue.position,
        start_ms: cue.start_ms,
        end_ms: cue.end_ms,
        duration_ms: (cue.end_ms - cue.start_ms).max(0),
        text: cue.text.clone(),
        lines,
        max_line_chars,
        char_limit,
        violations,
        edited: cue.edited,
    }
}

/// POST /api/v1/projects/{id}/music/presign
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/music/presign",
    tag = "Music",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = PresignMusicRequest,
    responses(
        (status = 200, description = "Pre-signed PUT URL for background music", body = PresignMusicResponse),
        (status = 400, description = "Unsupported audio format or invalid size", body = ProblemDetails),
        (status = 403, description = "Storage quota exceeded", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn presign_music(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<PresignMusicRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let ext = allowed_music_extension(&req.content_type).ok_or_else(|| {
        ApiError::BadRequest(
            "Unsupported music format. Allowed: MP3, WAV, M4A, AAC, OGG, FLAC, WebM".to_string(),
        )
    })?;

    if req.bytes == 0 || req.bytes as i64 > MAX_MUSIC_BYTES {
        return Err(ApiError::BadRequest(format!(
            "Music file size must be between 1 byte and {} MB",
            MAX_MUSIC_BYTES / (1024 * 1024)
        )));
    }

    let user_db = pace_db::find_user_by_id(&state.db, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;
    let previous_music = pace_db::find_music_media_by_project(&state.db, project_id).await?;
    let reclaimable = previous_music.map(|a| a.bytes).unwrap_or(0);
    let remaining_quota =
        (user_db.storage_quota_bytes - (user_db.storage_used_bytes - reclaimable)).max(0) as u64;

    if req.bytes > remaining_quota {
        return Err(ApiError::QuotaExceeded {
            current_bytes: user_db.storage_used_bytes,
            max_bytes: user_db.storage_quota_bytes,
        });
    }

    let media_id = Uuid::now_v7();
    let storage_key = StorageKeys::music_original(&auth.user.id, &project_id, &media_id, ext);
    let ttl = Duration::from_secs(3600);
    let clean_mime = req
        .content_type
        .split(';')
        .next()
        .unwrap_or(&req.content_type)
        .trim();
    let upload_url = state
        .storage
        .presign_put(&storage_key, clean_mime, req.bytes, ttl)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok((
        StatusCode::OK,
        Json(PresignMusicResponse {
            media_id,
            storage_key,
            upload_url: upload_url.to_string(),
        }),
    ))
}

/// POST /api/v1/projects/{id}/music
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/music",
    tag = "Music",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = RegisterMusicRequest,
    responses(
        (status = 201, description = "Background music registered", body = MusicStateResponse),
        (status = 400, description = "Invalid music metadata or missing uploaded object", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn register_music(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<RegisterMusicRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    if allowed_music_extension(&req.mime).is_none() {
        return Err(ApiError::BadRequest(
            "Unsupported music MIME type".to_string(),
        ));
    }
    if req.bytes <= 0 || req.bytes > MAX_MUSIC_BYTES {
        return Err(ApiError::BadRequest("Invalid music byte size".to_string()));
    }
    if !validate_project_music_key(&req.storage_key, auth.user.id, project_id) {
        return Err(ApiError::BadRequest(
            "Invalid music storage key".to_string(),
        ));
    }

    let obj = state
        .storage
        .head(&req.storage_key)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let Some(obj_info) = obj else {
        return Err(ApiError::BadRequest(
            "Uploaded music object was not found in storage".to_string(),
        ));
    };
    if obj_info.size_bytes as i64 > MAX_MUSIC_BYTES {
        let _ = state.storage.delete_prefix(&req.storage_key).await;
        return Err(ApiError::BadRequest(
            "Uploaded music exceeds size limit".to_string(),
        ));
    }

    let deleted_old = pace_db::delete_project_music(&state.db, project_id, auth.user.id).await?;
    for old_asset in deleted_old {
        if old_asset.storage_key != req.storage_key {
            let folder_prefix = format!(
                "u/{}/p/{}/music/{}/",
                auth.user.id, project_id, old_asset.id
            );
            let _ = state.storage.delete_prefix(&folder_prefix).await;
        }
    }

    let media_id = req.media_id.unwrap_or_else(Uuid::now_v7);
    let actual_bytes = obj_info.size_bytes as i64;
    let clean_mime = req
        .mime
        .split(';')
        .next()
        .unwrap_or(&req.mime)
        .trim()
        .to_string();

    let asset = pace_db::create_media_with_id(
        &state.db,
        media_id,
        CreateMediaAsset {
            project_id,
            owner_id: auth.user.id,
            kind: "music".to_string(),
            storage_key: req.storage_key.clone(),
            preview_key: None,
            thumb_key: None,
            original_filename: req.original_filename.clone(),
            mime: clean_mime,
            bytes: actual_bytes,
            sha256: req.sha256,
            width: None,
            height: None,
            duration_ms: None,
            source: Some("upload".to_string()),
            prompt: None,
        },
    )
    .await?;

    let settings = extract_music_settings(&project.settings);
    let audio_url = state
        .storage
        .presign_get(&asset.storage_key, Duration::from_secs(3600), None)
        .ok()
        .map(|u| u.to_string());

    Ok((
        StatusCode::CREATED,
        Json(MusicStateResponse {
            project_id,
            media_id: Some(asset.id),
            filename: Some(asset.original_filename),
            bytes: Some(asset.bytes),
            audio_url,
            settings: MusicSettingsDto::from(settings),
        }),
    ))
}

/// GET /api/v1/projects/{id}/music
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/music",
    tag = "Music",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Current background music asset and mix/ducking settings", body = MusicStateResponse),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn get_music_state(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let music_media = pace_db::find_music_media_by_project(&state.db, project_id).await?;
    let settings = extract_music_settings(&project.settings);
    let audio_url = music_media
        .as_ref()
        .and_then(|m| {
            state
                .storage
                .presign_get(&m.storage_key, Duration::from_secs(3600), None)
                .ok()
        })
        .map(|u| u.to_string());

    Ok((
        StatusCode::OK,
        Json(MusicStateResponse {
            project_id,
            media_id: music_media.as_ref().map(|m| m.id),
            filename: music_media.as_ref().map(|m| m.original_filename.clone()),
            bytes: music_media.as_ref().map(|m| m.bytes),
            audio_url,
            settings: MusicSettingsDto::from(settings),
        }),
    ))
}

/// PATCH /api/v1/projects/{id}/music
#[utoipa::path(
    patch,
    path = "/api/v1/projects/{id}/music",
    tag = "Music",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = UpdateMusicSettingsRequest,
    responses(
        (status = 200, description = "Updated background music volume, ducking, loop, and fade settings", body = MusicStateResponse),
        (status = 400, description = "Out-of-range volume or ducking parameter", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn update_music_settings(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<UpdateMusicSettingsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let mut current = extract_music_settings(&project.settings);
    if let Some(v) = req.volume_db {
        if !(-40.0..=0.0).contains(&v) {
            return Err(ApiError::BadRequest(
                "Music volume_db must be between -40.0 and 0.0 dB".to_string(),
            ));
        }
        current.volume_db = v;
    }
    if let Some(d) = req.ducking_enabled {
        current.ducking_enabled = d;
    }
    if let Some(duck_db) = req.duck_db {
        if !(-18.0..=-4.0).contains(&duck_db) {
            return Err(ApiError::BadRequest(
                "Music duck_db must be between -18.0 and -4.0 dB".to_string(),
            ));
        }
        current.duck_amount_db = duck_db;
    }
    if let Some(l) = req.loop_enabled {
        current.loop_enabled = l;
    }
    if let Some(cf) = req.crossfade_ms {
        if cf > 5_000 {
            return Err(ApiError::BadRequest(
                "Music crossfade_ms must be between 0 and 5000 ms".to_string(),
            ));
        }
        current.loop_crossfade_ms = cf;
    }
    if let Some(fi) = req.fade_in_ms {
        if fi > 15_000 {
            return Err(ApiError::BadRequest(
                "Music fade_in_ms must be between 0 and 15000 ms".to_string(),
            ));
        }
        current.fade_in_ms = fi;
    }
    if let Some(fo) = req.fade_out_ms {
        if fo > 15_000 {
            return Err(ApiError::BadRequest(
                "Music fade_out_ms must be between 0 and 15000 ms".to_string(),
            ));
        }
        current.fade_out_ms = fo;
    }

    let val = serde_json::to_value(&current).map_err(|e| ApiError::Internal(e.to_string()))?;
    pace_db::patch_project_settings_key(&state.db, project_id, auth.user.id, "music", val).await?;

    let music_media = pace_db::find_music_media_by_project(&state.db, project_id).await?;
    let audio_url = music_media
        .as_ref()
        .and_then(|m| {
            state
                .storage
                .presign_get(&m.storage_key, Duration::from_secs(3600), None)
                .ok()
        })
        .map(|u| u.to_string());

    Ok((
        StatusCode::OK,
        Json(MusicStateResponse {
            project_id,
            media_id: music_media.as_ref().map(|m| m.id),
            filename: music_media.as_ref().map(|m| m.original_filename.clone()),
            bytes: music_media.as_ref().map(|m| m.bytes),
            audio_url,
            settings: MusicSettingsDto::from(current),
        }),
    ))
}

/// DELETE /api/v1/projects/{id}/music
#[utoipa::path(
    delete,
    path = "/api/v1/projects/{id}/music",
    tag = "Music",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 204, description = "Background music track removed"),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn delete_music(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let _ = pace_db::delete_project_music(&state.db, project_id, auth.user.id).await?;
    let prefix = StorageKeys::project_music_prefix(&auth.user.id, &project_id);
    let _ = state.storage.delete_prefix(&prefix).await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/projects/{id}/subtitles
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/subtitles",
    tag = "Subtitles",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Subtitle cues with line wrapping, rule checks, and burn-in style settings", body = SubtitlesStateResponse),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn get_subtitles_state(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let raw_cues = pace_db::list_subtitle_cues(&state.db, project_id).await?;
    let style = extract_subtitle_style(&project.settings);
    let max_chars = max_line_chars_for_aspect(&project.aspect_ratio);
    let cues = raw_cues
        .iter()
        .map(|c| hydrate_subtitle_cue_detail(c, &project.aspect_ratio))
        .collect();

    Ok((
        StatusCode::OK,
        Json(SubtitlesStateResponse {
            project_id,
            aspect_ratio: project.aspect_ratio,
            max_chars_per_line: max_chars,
            style: SubtitleStyleDto::from(style),
            cues,
        }),
    ))
}

/// PUT /api/v1/projects/{id}/subtitles/style
#[utoipa::path(
    put,
    path = "/api/v1/projects/{id}/subtitles/style",
    tag = "Subtitles",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = SubtitleStyleDto,
    responses(
        (status = 200, description = "Updated subtitle burn-in style settings", body = SubtitleStyleDto),
        (status = 400, description = "Invalid font, position, color, or size", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn update_subtitle_style(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<SubtitleStyleDto>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let validated = req.to_spec()?;
    let val = serde_json::to_value(&validated).map_err(|e| ApiError::Internal(e.to_string()))?;
    pace_db::patch_project_settings_key(
        &state.db,
        project_id,
        auth.user.id,
        "subtitle_style",
        val,
    )
    .await?;

    Ok((StatusCode::OK, Json(SubtitleStyleDto::from(validated))))
}

/// POST /api/v1/projects/{id}/subtitles/{cue_id}/split
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/subtitles/{cue_id}/split",
    tag = "Subtitles",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("cue_id" = Uuid, Path, description = "Subtitle cue UUID to split")
    ),
    request_body = SplitSubtitleCueRequest,
    responses(
        (status = 200, description = "Subtitle cue split into two consecutive cues", body = Vec<SubtitleCueDto>),
        (status = 400, description = "Cue has fewer than 2 words or invalid duration", body = ProblemDetails),
        (status = 404, description = "Project or subtitle cue not found", body = ProblemDetails)
    )
)]
pub async fn split_subtitle_cue_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, cue_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SplitSubtitleCueRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let existing = pace_db::list_subtitle_cues(&state.db, project_id).await?;
    let target_pos = existing
        .iter()
        .position(|c| c.id == cue_id)
        .ok_or_else(|| ApiError::NotFound("Subtitle cue not found".to_string()))?;

    let target = &existing[target_pos];
    let spec = SubtitleCueSpec {
        id: target.id.to_string(),
        position: target_pos,
        start_ms: target.start_ms.max(0) as u64,
        end_ms: target.end_ms.max(target.start_ms) as u64,
        text: target.text.clone(),
    };

    let split_ms = req.split_word_index.and_then(|word_idx| {
        let word_count = spec.text.split_whitespace().count();
        if word_count >= 2 && word_idx >= 1 && word_idx < word_count {
            let dur = spec.duration_ms();
            Some(spec.start_ms + (dur * (word_idx as u64)) / (word_count as u64))
        } else {
            None
        }
    });

    let (left, right) = split_subtitle_cue(&spec, split_ms, &project.aspect_ratio)
        .map_err(ApiError::BadRequest)?;
    let left_wrapped = wrap_subtitle_lines(&left.text, &project.aspect_ratio).join("\n");
    let right_wrapped = wrap_subtitle_lines(&right.text, &project.aspect_ratio).join("\n");

    let mut replacement: Vec<(Option<Uuid>, i32, i32, String, bool)> =
        Vec::with_capacity(existing.len() + 1);
    for (idx, c) in existing.iter().enumerate() {
        if idx == target_pos {
            replacement.push((
                Some(c.id),
                left.start_ms as i32,
                left.end_ms as i32,
                left_wrapped.clone(),
                true,
            ));
            replacement.push((
                None,
                right.start_ms as i32,
                right.end_ms as i32,
                right_wrapped.clone(),
                true,
            ));
        } else {
            replacement.push((
                Some(c.id),
                c.start_ms,
                c.end_ms,
                c.text.clone(),
                c.edited,
            ));
        }
    }

    let updated =
        pace_db::replace_project_subtitle_cues(&state.db, project_id, replacement).await?;
    let dtos: Vec<SubtitleCueDto> = updated
        .into_iter()
        .map(|c| SubtitleCueDto {
            id: c.id,
            position: c.position,
            start_ms: c.start_ms,
            end_ms: c.end_ms,
            text: c.text,
            edited: c.edited,
        })
        .collect();

    Ok((StatusCode::OK, Json(dtos)))
}

/// POST /api/v1/projects/{id}/subtitles/merge
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/subtitles/merge",
    tag = "Subtitles",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = MergeSubtitleCuesRequest,
    responses(
        (status = 200, description = "Two adjacent subtitle cues merged", body = Vec<SubtitleCueDto>),
        (status = 400, description = "Subtitle cues are not adjacent", body = ProblemDetails),
        (status = 404, description = "Project or subtitle cue not found", body = ProblemDetails)
    )
)]
pub async fn merge_subtitle_cues_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<MergeSubtitleCuesRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let existing = pace_db::list_subtitle_cues(&state.db, project_id).await?;
    let first_idx = existing
        .iter()
        .position(|c| c.id == req.first_cue_id)
        .ok_or_else(|| ApiError::NotFound("First subtitle cue not found".to_string()))?;
    let second_idx = existing
        .iter()
        .position(|c| c.id == req.second_cue_id)
        .ok_or_else(|| ApiError::NotFound("Second subtitle cue not found".to_string()))?;

    if second_idx != first_idx + 1 {
        return Err(ApiError::BadRequest(
            "Only adjacent subtitle cues in chronological order can be merged".to_string(),
        ));
    }

    let first_rec = &existing[first_idx];
    let second_rec = &existing[second_idx];
    let first_spec = SubtitleCueSpec {
        id: first_rec.id.to_string(),
        position: first_idx,
        start_ms: first_rec.start_ms.max(0) as u64,
        end_ms: first_rec.end_ms.max(first_rec.start_ms) as u64,
        text: first_rec.text.clone(),
    };
    let second_spec = SubtitleCueSpec {
        id: second_rec.id.to_string(),
        position: second_idx,
        start_ms: second_rec.start_ms.max(0) as u64,
        end_ms: second_rec.end_ms.max(second_rec.start_ms) as u64,
        text: second_rec.text.clone(),
    };
    let merged = merge_subtitle_cues(&first_spec, &second_spec, &project.aspect_ratio)
        .map_err(ApiError::BadRequest)?;
    let merged_wrapped = wrap_subtitle_lines(&merged.text, &project.aspect_ratio).join("\n");

    let mut replacement: Vec<(Option<Uuid>, i32, i32, String, bool)> =
        Vec::with_capacity(existing.len().saturating_sub(1));
    let mut idx = 0;
    while idx < existing.len() {
        if idx == first_idx {
            replacement.push((
                Some(existing[idx].id),
                merged.start_ms as i32,
                merged.end_ms as i32,
                merged_wrapped.clone(),
                true,
            ));
            idx += 2;
        } else {
            let c = &existing[idx];
            replacement.push((
                Some(c.id),
                c.start_ms,
                c.end_ms,
                c.text.clone(),
                c.edited,
            ));
            idx += 1;
        }
    }

    let updated =
        pace_db::replace_project_subtitle_cues(&state.db, project_id, replacement).await?;
    let dtos: Vec<SubtitleCueDto> = updated
        .into_iter()
        .map(|c| SubtitleCueDto {
            id: c.id,
            position: c.position,
            start_ms: c.start_ms,
            end_ms: c.end_ms,
            text: c.text,
            edited: c.edited,
        })
        .collect();

    Ok((StatusCode::OK, Json(dtos)))
}
