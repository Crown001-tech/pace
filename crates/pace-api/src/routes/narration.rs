use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pace_db::{CreateMediaAsset, SilenceGap, TranscriptWord};
use pace_jobs::{
    cancel_project_jobs, enqueue_job, find_latest_project_job, AudioJobPayload, EnqueueJob,
    JobQueue,
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
    routes::jobs::JobStatusDto,
};

const MAX_AUDIO_BYTES: i64 = 250 * 1024 * 1024; // 250 MB raw audio upload cap

fn allowed_audio_extension(content_type: &str) -> Option<&'static str> {
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

fn validate_project_audio_key(key: &str, user_id: Uuid, project_id: Uuid) -> bool {
    let expected_prefix = StorageKeys::project_audio_prefix(&user_id, &project_id);
    key.starts_with(&expected_prefix) && !key.contains("..") && !key.contains('\\')
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PresignNarrationRequest {
    pub filename: String,
    pub content_type: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PresignNarrationResponse {
    pub media_id: Uuid,
    pub storage_key: String,
    pub upload_url: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterNarrationRequest {
    pub media_id: Option<Uuid>,
    pub storage_key: String,
    pub original_filename: String,
    pub mime: String,
    pub bytes: i64,
    pub sha256: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterNarrationResponse {
    pub media_id: Uuid,
    pub job: JobStatusDto,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RealignNarrationRequest {
    #[serde(default = "default_true")]
    pub keep_manual_boundaries: bool,
    #[serde(default)]
    pub force_local_transcription: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SilenceGapDto {
    pub start_ms: i32,
    pub end_ms: i32,
    pub duration_ms: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TranscriptWordDto {
    pub word: String,
    pub start_ms: i32,
    pub end_ms: i32,
    pub probability: f32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TranscriptSummaryDto {
    pub engine: String,
    pub model: String,
    pub language: String,
    pub word_count: usize,
    pub words: Vec<TranscriptWordDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BoundaryDto {
    pub id: Uuid,
    pub index: i32,
    pub time_ms: i32,
    pub source: String,
    pub confidence: Option<f32>,
    pub flags: Vec<String>,
    pub transition: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SubtitleCueDto {
    pub id: Uuid,
    pub position: i32,
    pub start_ms: i32,
    pub end_ms: i32,
    pub text: String,
    pub edited: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NarrationStateResponse {
    pub project_id: Uuid,
    pub media_id: Option<Uuid>,
    pub filename: Option<String>,
    pub bytes: Option<i64>,
    pub audio_url: Option<String>,
    pub duration_ms: Option<i32>,
    pub loudness_lufs: Option<f32>,
    pub true_peak_db: Option<f32>,
    pub has_peaks: bool,
    pub silences: Vec<SilenceGapDto>,
    pub transcript: Option<TranscriptSummaryDto>,
    pub boundaries: Vec<BoundaryDto>,
    pub subtitle_cues: Vec<SubtitleCueDto>,
    pub active_job: Option<JobStatusDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NarrationPeaksResponse {
    pub media_id: Uuid,
    pub duration_ms: i32,
    pub peaks: Vec<[f32; 2]>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBoundaryRequest {
    pub time_ms: i32,
    pub transition: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateSubtitleCueRequest {
    pub text: String,
    pub start_ms: Option<i32>,
    pub end_ms: Option<i32>,
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/narration/presign",
    tag = "Narration",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = PresignNarrationRequest,
    responses(
        (status = 200, description = "Pre-signed PUT URL for narration audio", body = PresignNarrationResponse),
        (status = 400, description = "Unsupported audio format or invalid size", body = ProblemDetails),
        (status = 403, description = "Storage quota exceeded", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn presign_narration(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<PresignNarrationRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let ext = allowed_audio_extension(&req.content_type).ok_or_else(|| {
        ApiError::BadRequest(
            "Unsupported audio format. Allowed: MP3, WAV, M4A, AAC, OGG, FLAC, WebM".to_string(),
        )
    })?;

    if req.bytes == 0 || req.bytes as i64 > MAX_AUDIO_BYTES {
        return Err(ApiError::BadRequest(format!(
            "Audio file size must be between 1 byte and {} MB",
            MAX_AUDIO_BYTES / (1024 * 1024)
        )));
    }

    let user_db = pace_db::find_user_by_id(&state.db, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;
    let previous_narration = pace_db::find_narration_media_by_project(&state.db, project_id).await?;
    let reclaimable = previous_narration.map(|a| a.bytes).unwrap_or(0);
    let remaining_quota =
        (user_db.storage_quota_bytes - (user_db.storage_used_bytes - reclaimable)).max(0) as u64;

    if req.bytes > remaining_quota {
        return Err(ApiError::QuotaExceeded {
            current_bytes: user_db.storage_used_bytes,
            max_bytes: user_db.storage_quota_bytes,
        });
    }

    let media_id = Uuid::new_v4();
    let storage_key = StorageKeys::audio_original(&auth.user.id, &project_id, &media_id, ext);
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
        Json(PresignNarrationResponse {
            media_id,
            storage_key,
            upload_url: upload_url.to_string(),
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/narration",
    tag = "Narration",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = RegisterNarrationRequest,
    responses(
        (status = 202, description = "Narration registered and audio processing job queued", body = RegisterNarrationResponse),
        (status = 400, description = "Invalid audio metadata or missing uploaded object", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn register_narration(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<RegisterNarrationRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    if allowed_audio_extension(&req.mime).is_none() {
        return Err(ApiError::BadRequest("Unsupported audio MIME type".to_string()));
    }
    if req.bytes <= 0 || req.bytes > MAX_AUDIO_BYTES {
        return Err(ApiError::BadRequest("Invalid audio byte size".to_string()));
    }
    if !validate_project_audio_key(&req.storage_key, auth.user.id, project_id) {
        return Err(ApiError::BadRequest("Invalid audio storage key".to_string()));
    }

    let obj = state
        .storage
        .head(&req.storage_key)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let Some(obj_info) = obj else {
        return Err(ApiError::BadRequest(
            "Uploaded narration object was not found in storage".to_string(),
        ));
    };
    if obj_info.size_bytes as i64 > MAX_AUDIO_BYTES {
        let _ = state.storage.delete_prefix(&req.storage_key).await;
        return Err(ApiError::BadRequest("Uploaded audio exceeds size limit".to_string()));
    }

    // Cancel any running audio job and remove prior narration artifacts safely.
    let _ = cancel_project_jobs(&state.db, project_id, JobQueue::Audio).await;
    let deleted_old = pace_db::delete_project_narration(&state.db, project_id, auth.user.id).await?;
    for old_asset in deleted_old {
        if old_asset.storage_key != req.storage_key {
            let folder_prefix = format!(
                "u/{}/p/{}/audio/{}/",
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

    let asset = match pace_db::create_media_with_id(
        &state.db,
        media_id,
        CreateMediaAsset {
            project_id,
            owner_id: auth.user.id,
            kind: "narration".to_string(),
            storage_key: req.storage_key.clone(),
            preview_key: None,
            thumb_key: None,
            original_filename: req.original_filename.clone(),
            mime: clean_mime.clone(),
            bytes: actual_bytes,
            sha256: req.sha256.clone(),
            width: None,
            height: None,
            duration_ms: None,
            source: Some("upload".to_string()),
            prompt: None,
        },
    )
    .await
    {
        Ok(asset) => asset,
        Err(err) => {
            let _ = state.storage.delete_prefix(&req.storage_key).await;
            return Err(err.into());
        }
    };

    let payload = AudioJobPayload {
        project_id,
        user_id: auth.user.id,
        media_id: asset.id,
        storage_key: req.storage_key,
        opus_key: StorageKeys::audio_opus(&auth.user.id, &project_id, &asset.id),
        asr_key: StorageKeys::audio_asr(&auth.user.id, &project_id, &asset.id),
        peaks_key: StorageKeys::audio_peaks(&auth.user.id, &project_id, &asset.id),
        mime: clean_mime,
        original_filename: req.original_filename,
        keep_manual_boundaries: false,
        force_local_transcription: false,
    };

    let job = enqueue_job(
        &state.db,
        EnqueueJob {
            queue: JobQueue::Audio,
            job_type: "audio_process".to_string(),
            project_id: Some(project_id),
            user_id: Some(auth.user.id),
            priority: 10,
            payload: serde_json::to_value(payload).map_err(|e| ApiError::Internal(e.to_string()))?,
            max_attempts: 3,
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(RegisterNarrationResponse {
            media_id: asset.id,
            job: JobStatusDto::from(job),
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/narration",
    tag = "Narration",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Current project narration, alignment boundaries, subtitles, and job status", body = NarrationStateResponse),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn get_narration_state(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let media = pace_db::find_narration_media_by_project(&state.db, project_id).await?;
    let narration = pace_db::get_narration(&state.db, project_id).await?;
    let transcript = pace_db::get_latest_transcript(&state.db, project_id).await?;
    let boundaries = pace_db::list_boundaries(&state.db, project_id).await?;
    let subtitle_cues = pace_db::list_subtitle_cues(&state.db, project_id).await?;
    let mut active_job =
        find_latest_project_job(&state.db, project_id, auth.user.id, JobQueue::Audio).await?;
    if let Some(ref mut j) = active_job {
        if j.status == "succeeded" {
            j.error_code = None;
            j.error_message = None;
        }
    }

    let ttl = Duration::from_secs(3600);
    let playback_key = if let Some(ref nar) = narration {
        Some(StorageKeys::audio_opus(&auth.user.id, &project_id, &nar.media_id))
    } else {
        media.as_ref().map(|m| m.storage_key.clone())
    };

    let audio_url = playback_key
        .as_deref()
        .and_then(|k| state.storage.presign_get(k, ttl, None).ok())
        .map(|u| u.to_string());

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

    let transcript_dto = transcript.map(|t| {
        let words: Vec<TranscriptWordDto> =
            serde_json::from_value::<Vec<TranscriptWord>>(t.words.clone())
                .unwrap_or_default()
                .into_iter()
                .map(|w| TranscriptWordDto {
                    word: w.word,
                    start_ms: w.start_ms,
                    end_ms: w.end_ms,
                    probability: w.probability,
                })
                .collect();
        TranscriptSummaryDto {
            engine: t.engine,
            model: t.model,
            language: t.language,
            word_count: words.len(),
            words,
        }
    });

    Ok((
        StatusCode::OK,
        Json(NarrationStateResponse {
            project_id,
            media_id: media.as_ref().map(|m| m.id),
            filename: media.as_ref().map(|m| m.original_filename.clone()),
            bytes: media.as_ref().map(|m| m.bytes),
            audio_url,
            duration_ms: narration.as_ref().map(|n| n.duration_ms),
            loudness_lufs: narration.as_ref().and_then(|n| n.loudness_lufs),
            true_peak_db: narration.as_ref().and_then(|n| n.true_peak_db),
            has_peaks: narration
                .as_ref()
                .and_then(|n| n.peaks_key.as_ref())
                .is_some(),
            silences,
            transcript: transcript_dto,
            boundaries: boundaries
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
                .collect(),
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
            active_job: active_job.map(JobStatusDto::from),
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/narration/peaks",
    tag = "Narration",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Waveform peak pairs for timeline/narration display", body = NarrationPeaksResponse),
        (status = 404, description = "Narration peaks not available", body = ProblemDetails)
    )
)]
pub async fn get_narration_peaks(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let narration = pace_db::get_narration(&state.db, project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Narration not found".to_string()))?;

    let peaks_key = narration
        .peaks_key
        .ok_or_else(|| ApiError::NotFound("Narration waveform peaks not ready".to_string()))?;

    let tmp_path = std::env::temp_dir().join(format!("pace_peaks_{}.json", Uuid::now_v7()));
    state
        .storage
        .download(&peaks_key, &tmp_path)
        .await
        .map_err(|e| ApiError::NotFound(format!("Peaks file unavailable: {e}")))?;

    let raw = tokio::fs::read(&tmp_path).await;
    let _ = tokio::fs::remove_file(&tmp_path).await;
    let bytes = raw.map_err(|e| ApiError::Internal(e.to_string()))?;
    let peaks: Vec<[f32; 2]> =
        serde_json::from_slice(&bytes).map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok((
        StatusCode::OK,
        Json(NarrationPeaksResponse {
            media_id: narration.media_id,
            duration_ms: narration.duration_ms,
            peaks,
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/narration/realign",
    tag = "Narration",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = RealignNarrationRequest,
    responses(
        (status = 202, description = "Re-alignment job queued", body = RegisterNarrationResponse),
        (status = 400, description = "No narration uploaded yet", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn realign_narration(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<RealignNarrationRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let media = pace_db::find_narration_media_by_project(&state.db, project_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("Upload narration audio before re-aligning".to_string()))?;

    let transcript = pace_db::get_latest_transcript(&state.db, project_id).await?;
    let job_type = if transcript.is_some() && !req.force_local_transcription {
        "realign"
    } else {
        "audio_process"
    };

    let payload = AudioJobPayload {
        project_id,
        user_id: auth.user.id,
        media_id: media.id,
        storage_key: media.storage_key.clone(),
        opus_key: StorageKeys::audio_opus(&auth.user.id, &project_id, &media.id),
        asr_key: StorageKeys::audio_asr(&auth.user.id, &project_id, &media.id),
        peaks_key: StorageKeys::audio_peaks(&auth.user.id, &project_id, &media.id),
        mime: media.mime.clone(),
        original_filename: media.original_filename.clone(),
        keep_manual_boundaries: req.keep_manual_boundaries,
        force_local_transcription: req.force_local_transcription,
    };

    let _ = cancel_project_jobs(&state.db, project_id, JobQueue::Audio).await;
    let job = enqueue_job(
        &state.db,
        EnqueueJob {
            queue: JobQueue::Audio,
            job_type: job_type.to_string(),
            project_id: Some(project_id),
            user_id: Some(auth.user.id),
            priority: 10,
            payload: serde_json::to_value(payload).map_err(|e| ApiError::Internal(e.to_string()))?,
            max_attempts: 3,
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(RegisterNarrationResponse {
            media_id: media.id,
            job: JobStatusDto::from(job),
        }),
    ))
}

#[utoipa::path(
    delete,
    path = "/api/v1/projects/{id}/narration",
    tag = "Narration",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 204, description = "Narration and alignment artifacts deleted"),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn delete_narration(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let _ = cancel_project_jobs(&state.db, project_id, JobQueue::Audio).await;
    let _ = pace_db::delete_project_narration(&state.db, project_id, auth.user.id).await?;
    let prefix = StorageKeys::project_audio_prefix(&auth.user.id, &project_id);
    let _ = state.storage.delete_prefix(&prefix).await;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    patch,
    path = "/api/v1/projects/{id}/boundaries/{index}",
    tag = "Narration",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("index" = i32, Path, description = "Zero-based boundary index")
    ),
    request_body = UpdateBoundaryRequest,
    responses(
        (status = 200, description = "Manual boundary update persisted", body = BoundaryDto),
        (status = 400, description = "Boundary overlaps neighbor or is immutable", body = ProblemDetails),
        (status = 404, description = "Project or boundary not found", body = ProblemDetails)
    )
)]
pub async fn patch_boundary(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, index)): Path<(Uuid, i32)>,
    Json(req): Json<UpdateBoundaryRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let validated_transition =
        crate::routes::effects::validate_boundary_transition(req.transition)?;

    let updated =
        pace_db::update_boundary(&state.db, project_id, index, req.time_ms, validated_transition)
            .await?;

    Ok((
        StatusCode::OK,
        Json(BoundaryDto {
            id: updated.id,
            index: updated.index,
            time_ms: updated.time_ms,
            source: updated.source,
            confidence: updated.confidence,
            flags: updated.flags,
            transition: updated.transition,
        }),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/projects/{id}/subtitles/{cue_id}",
    tag = "Narration",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("cue_id" = Uuid, Path, description = "Subtitle cue UUID")
    ),
    request_body = UpdateSubtitleCueRequest,
    responses(
        (status = 200, description = "Subtitle cue updated", body = SubtitleCueDto),
        (status = 400, description = "Invalid subtitle text or timings", body = ProblemDetails),
        (status = 404, description = "Project or subtitle cue not found", body = ProblemDetails)
    )
)]
pub async fn patch_subtitle_cue(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, cue_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateSubtitleCueRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let normalized_text =
        pace_engine::subtitles::wrap_subtitle_lines(&req.text, &project.aspect_ratio).join("\n");

    let updated = pace_db::update_subtitle_cue(
        &state.db,
        project_id,
        cue_id,
        &normalized_text,
        req.start_ms,
        req.end_ms,
    )
    .await?
    .ok_or_else(|| ApiError::NotFound("Subtitle cue not found".to_string()))?;

    Ok((
        StatusCode::OK,
        Json(SubtitleCueDto {
            id: updated.id,
            position: updated.position,
            start_ms: updated.start_ms,
            end_ms: updated.end_ms,
            text: updated.text,
            edited: updated.edited,
        }),
    ))
}
