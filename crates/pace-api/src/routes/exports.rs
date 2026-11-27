use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    Json,
};
use chrono::{DateTime, Utc};
use pace_db::ExportRecord;
use pace_engine::fit::SizeF;
use pace_engine::segments::ExportResolution;
use pace_jobs::{enqueue_job, EnqueueJob, JobQueue, RenderJobPayload, RenderSceneAsset};
use pace_storage::StorageKeys;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    time::Duration,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthContext,
    error::{ApiError, ProblemDetails},
    routes::effects::compile_project_spec_from_records,
    routes::music::{extract_music_settings, extract_stickers, extract_subtitle_style},
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct StartExportRequest {
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub job_id: Option<Uuid>,
    pub spec_hash: String,
    pub resolution: String,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub duration_ms: i32,
    pub status: String,
    pub progress: i32,
    pub stage: Option<String>,
    pub size_bytes: Option<i64>,
    pub download_url: Option<String>,
    pub download_filename: Option<String>,
    pub validation: serde_json::Value,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

fn export_filename(project_name: &str, resolution: &str) -> String {
    let safe: String = project_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = safe.trim_matches('-');
    let base = if trimmed.is_empty() { "video" } else { trimmed };
    format!("pace-{base}-{resolution}.mp4")
}

fn build_export_response(
    state: &AppState,
    export: ExportRecord,
    project_name: &str,
    progress: i32,
    stage: Option<String>,
) -> ExportResponse {
    let filename = export_filename(project_name, &export.resolution);
    let download_url = if export.status == "succeeded" {
        export.storage_key.as_deref().and_then(|key| {
            state
                .storage
                .presign_get(key, Duration::from_secs(3600), Some(&filename))
                .ok()
                .map(|u| u.to_string())
        })
    } else {
        None
    };

    ExportResponse {
        id: export.id,
        project_id: export.project_id,
        job_id: export.job_id,
        spec_hash: export.spec_hash,
        resolution: export.resolution,
        width: export.width,
        height: export.height,
        fps: export.fps,
        duration_ms: export.duration_ms,
        status: export.status.clone(),
        progress: if export.status == "succeeded" {
            100
        } else {
            progress
        },
        stage,
        size_bytes: export.size_bytes,
        download_url,
        download_filename: Some(filename),
        validation: export.validation,
        error_code: export.error_code,
        error_message: export.error_message,
        created_at: export.created_at,
        updated_at: export.updated_at,
        finished_at: export.finished_at,
    }
}

async fn hydrate_export_response(
    state: &AppState,
    export: ExportRecord,
    project_name: &str,
    user_id: Uuid,
) -> ExportResponse {
    let (progress, stage) = if let Some(job_id) = export.job_id {
        match pace_jobs::find_job_by_id_and_project(&state.db, job_id, export.project_id, user_id)
            .await
        {
            Ok(Some(job)) => (job.progress, job.stage),
            _ => (0, None),
        }
    } else {
        (0, None)
    };
    build_export_response(state, export, project_name, progress, stage)
}

/// POST /api/v1/projects/{id}/exports
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/exports",
    tag = "Exports",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = StartExportRequest,
    responses(
        (status = 202, description = "MP4 export queued", body = ExportResponse),
        (status = 400, description = "Missing section assignments, empty script, or invalid resolution", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails),
        (status = 409, description = "An export is already queued or rendering for this project", body = ProblemDetails)
    )
)]
pub async fn start_export(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<StartExportRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let resolution = ExportResolution::parse(&req.resolution).ok_or_else(|| {
        ApiError::BadRequest(
            "Invalid resolution. Allowed values: '360p', '480p', '720p', '1080p'".to_string(),
        )
    })?;

    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    if pace_db::has_active_export(&state.db, project_id).await? {
        return Err(ApiError::Conflict(
            "An export is already queued or rendering for this project".to_string(),
        ));
    }

    let sections = pace_db::get_sections_by_project(&state.db, project_id).await?;
    if sections.is_empty() {
        return Err(ApiError::BadRequest(
            "Add at least one script section before exporting".to_string(),
        ));
    }

    let media_assets = pace_db::find_by_project(&state.db, project_id).await?;
    let assignments = pace_db::get_assignments_by_project(&state.db, project_id).await?;
    let narration_media = pace_db::find_narration_media_by_project(&state.db, project_id).await?;
    let narration = pace_db::get_narration(&state.db, project_id).await?;
    let db_boundaries = pace_db::list_boundaries(&state.db, project_id).await?;

    let assigned_sections: HashSet<Uuid> = assignments.iter().map(|a| a.section_id).collect();
    let missing_indices: Vec<usize> = sections
        .iter()
        .enumerate()
        .filter(|(_, s)| !assigned_sections.contains(&s.id))
        .map(|(idx, _)| idx + 1)
        .collect();
    if !missing_indices.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "Assign images to all scenes before exporting (missing scene(s): {:?})",
            missing_indices
        )));
    }

    let (width, height) = resolution.dimensions(&project.aspect_ratio);
    let mut spec = compile_project_spec_from_records(
        &project,
        &sections,
        &media_assets,
        &assignments,
        &db_boundaries,
        narration.as_ref().map(|n| n.duration_ms),
    )?
    .ok_or_else(|| ApiError::BadRequest("Failed to compile project RenderSpec".to_string()))?;
    spec.frame_size = SizeF::new(width as f32, height as f32);
    spec.spec_hash = pace_engine::spec::compute_spec_hash(&spec);

    let media_map: HashMap<Uuid, _> = media_assets
        .into_iter()
        .filter(|m| m.kind == "image")
        .map(|m| (m.id, m))
        .collect();
    let assign_map: HashMap<Uuid, _> = assignments
        .into_iter()
        .map(|a| (a.section_id, a))
        .collect();

    let mut scene_assets = Vec::with_capacity(sections.len());
    for (idx, sec) in sections.iter().enumerate() {
        if let Some(assign) = assign_map.get(&sec.id) {
            if let Some(media) = media_map.get(&assign.media_id) {
                scene_assets.push(RenderSceneAsset {
                    scene_index: idx,
                    media_id: media.id,
                    storage_key: media.storage_key.clone(),
                });
            }
        }
    }

    let audio_key = if let Some(ref nar) = narration {
        Some(StorageKeys::audio_opus(&auth.user.id, &project_id, &nar.media_id))
    } else {
        narration_media.map(|m| m.storage_key)
    };

    let music_media = pace_db::find_music_media_by_project(&state.db, project_id).await?;
    let music_key = music_media.map(|m| m.storage_key);
    let music_settings = if music_key.is_some() {
        Some(extract_music_settings(&project.settings))
    } else {
        None
    };

    let raw_subtitles = pace_db::list_subtitle_cues(&state.db, project_id).await?;
    let subtitles: Vec<pace_engine::subtitles::SubtitleCueSpec> = raw_subtitles
        .into_iter()
        .map(|c| pace_engine::subtitles::SubtitleCueSpec {
            id: c.id.to_string(),
            position: c.position.max(0) as usize,
            start_ms: c.start_ms.max(0) as u64,
            end_ms: c.end_ms.max(c.start_ms) as u64,
            text: c.text,
        })
        .collect();
    let subtitle_style = extract_subtitle_style(&project.settings);
    let stickers = extract_stickers(&project.settings);

    let export_id = Uuid::now_v7();
    let output_key = StorageKeys::export_mp4(
        &auth.user.id,
        &project_id,
        &export_id,
        &project.name,
        resolution.as_str(),
    );

    let spec_json = serde_json::to_value(&spec).map_err(|e| ApiError::Internal(e.to_string()))?;
    let payload = RenderJobPayload {
        export_id,
        project_id,
        user_id: auth.user.id,
        project_name: project.name.clone(),
        aspect_ratio: project.aspect_ratio.clone(),
        resolution: resolution.as_str().to_string(),
        width,
        height,
        fps: spec.fps,
        output_key,
        audio_key,
        music_key,
        music_settings,
        subtitles,
        subtitle_style,
        stickers,
        spec_json,
        scene_assets,
    };

    let created = pace_db::create_export(
        &state.db,
        pace_db::CreateExport {
            id: export_id,
            project_id,
            user_id: auth.user.id,
            job_id: None,
            spec_hash: spec.spec_hash,
            resolution: resolution.as_str().to_string(),
            width: width as i32,
            height: height as i32,
            fps: spec.fps as i32,
            duration_ms: spec.narration_ms as i32,
        },
    )
    .await?;

    let job = enqueue_job(
        &state.db,
        EnqueueJob {
            queue: JobQueue::Render,
            job_type: "video_export".to_string(),
            project_id: Some(project_id),
            user_id: Some(auth.user.id),
            priority: 10,
            payload: serde_json::to_value(&payload)
                .map_err(|e| ApiError::Internal(e.to_string()))?,
            max_attempts: 3,
        },
    )
    .await?;

    pace_db::attach_export_job(&state.db, created.id, job.id).await?;
    let mut updated = created;
    updated.job_id = Some(job.id);

    Ok((
        StatusCode::ACCEPTED,
        Json(build_export_response(
            &state,
            updated,
            &project.name,
            job.progress,
            job.stage,
        )),
    ))
}

/// GET /api/v1/projects/{id}/exports
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/exports",
    tag = "Exports",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Recent project exports", body = Vec<ExportResponse>),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn list_exports(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let rows = pace_db::list_exports_by_project(&state.db, project_id, auth.user.id).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(hydrate_export_response(&state, row, &project.name, auth.user.id).await);
    }

    Ok((StatusCode::OK, Json(out)))
}

/// GET /api/v1/projects/{id}/exports/{export_id}
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/exports/{export_id}",
    tag = "Exports",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("export_id" = Uuid, Path, description = "Export UUID")
    ),
    responses(
        (status = 200, description = "Export details, validation status, and signed download URL", body = ExportResponse),
        (status = 404, description = "Project or export not found", body = ProblemDetails)
    )
)]
pub async fn get_export(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, export_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let export = pace_db::find_export_by_id_and_project(
        &state.db,
        export_id,
        project_id,
        auth.user.id,
    )
    .await?
    .ok_or_else(|| ApiError::NotFound("Export not found".to_string()))?;

    let resp = hydrate_export_response(&state, export, &project.name, auth.user.id).await;
    Ok((StatusCode::OK, Json(resp)))
}

/// POST /api/v1/projects/{id}/exports/{export_id}/retry
#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/exports/{export_id}/retry",
    tag = "Exports",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("export_id" = Uuid, Path, description = "Export UUID")
    ),
    responses(
        (status = 202, description = "Failed or cancelled export re-queued (resuming cached segments)", body = ExportResponse),
        (status = 400, description = "Export cannot be retried in current status", body = ProblemDetails),
        (status = 404, description = "Project or export not found", body = ProblemDetails)
    )
)]
pub async fn retry_export(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, export_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let export = pace_db::find_export_by_id_and_project(
        &state.db,
        export_id,
        project_id,
        auth.user.id,
    )
    .await?
    .ok_or_else(|| ApiError::NotFound("Export not found".to_string()))?;

    if export.status != "failed" && export.status != "cancelled" {
        return Err(ApiError::BadRequest(format!(
            "Cannot retry export with status '{}'",
            export.status
        )));
    }

    let prev_job_id = export.job_id.ok_or_else(|| {
        ApiError::BadRequest("Original render job payload not found for retry".to_string())
    })?;
    let prev_job = pace_jobs::find_job_by_id_and_project(
        &state.db,
        prev_job_id,
        project_id,
        auth.user.id,
    )
    .await?
    .ok_or_else(|| ApiError::NotFound("Previous render job not found".to_string()))?;

    let new_job = enqueue_job(
        &state.db,
        EnqueueJob {
            queue: JobQueue::Render,
            job_type: "video_export".to_string(),
            project_id: Some(project_id),
            user_id: Some(auth.user.id),
            priority: 10,
            payload: prev_job.payload,
            max_attempts: 3,
        },
    )
    .await?;

    let updated = pace_db::reset_export_for_retry(&state.db, export.id, new_job.id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(build_export_response(
            &state,
            updated,
            &project.name,
            new_job.progress,
            new_job.stage,
        )),
    ))
}

/// GET /api/v1/projects/{id}/exports/{export_id}/events
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/exports/{export_id}/events",
    tag = "Exports",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("export_id" = Uuid, Path, description = "Export UUID")
    ),
    responses(
        (status = 200, description = "Server-Sent Events stream of live export progress"),
        (status = 404, description = "Project or export not found", body = ProblemDetails)
    )
)]
pub async fn stream_export_events(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, export_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let _initial = pace_db::find_export_by_id_and_project(
        &state.db,
        export_id,
        project_id,
        auth.user.id,
    )
    .await?
    .ok_or_else(|| ApiError::NotFound("Export not found".to_string()))?;

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(16);
    let user_id = auth.user.id;
    let project_name = project.name.clone();

    tokio::spawn(async move {
        let mut last_snapshot = String::new();
        loop {
            let Ok(Some(export)) = pace_db::find_export_by_id_and_project(
                &state.db,
                export_id,
                project_id,
                user_id,
            )
            .await
            else {
                break;
            };

            let terminal = matches!(
                export.status.as_str(),
                "succeeded" | "failed" | "cancelled"
            );
            let dto = hydrate_export_response(&state, export, &project_name, user_id).await;
            if let Ok(json_str) = serde_json::to_string(&dto) {
                if json_str != last_snapshot {
                    last_snapshot = json_str.clone();
                    let ev = Event::default().event("progress").data(json_str);
                    if tx.send(Ok(ev)).await.is_err() {
                        break;
                    }
                }
            }
            if terminal {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}
