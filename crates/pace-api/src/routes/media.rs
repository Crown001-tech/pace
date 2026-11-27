use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pace_db::{CreateMediaAsset, DbError, MediaAsset};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthContext,
    error::{ApiError, ProblemDetails},
};

const MAX_IMAGE_BYTES: i64 = 20 * 1024 * 1024;
const MAX_IMAGES_PER_PROJECT: i64 = 400;

fn is_allowed_image_mime(mime: &str) -> bool {
    matches!(
        mime.trim().to_ascii_lowercase().as_str(),
        "image/jpeg" | "image/jpg" | "image/png" | "image/webp"
    )
}

fn validate_project_media_key(key: &str, user_id: Uuid, project_id: Uuid) -> bool {
    let expected_prefix = format!("u/{user_id}/p/{project_id}/media/");
    key.starts_with(&expected_prefix) && !key.contains("..") && !key.contains('\\')
}

async fn cleanup_unregistered_keys(state: &AppState, req: &RegisterMediaRequest) {
    let _ = state.storage.delete_prefix(&req.storage_key).await;
    if let Some(ref k) = req.preview_key {
        let _ = state.storage.delete_prefix(k).await;
    }
    if let Some(ref k) = req.thumb_key {
        let _ = state.storage.delete_prefix(k).await;
    }
}

fn build_media_response(state: &AppState, asset: MediaAsset) -> MediaResponse {
    let ttl = Duration::from_secs(3600);
    let download_url = state
        .storage
        .presign_get(&asset.storage_key, ttl, None)
        .map(|u| u.to_string())
        .unwrap_or_default();

    let thumb_url = asset
        .thumb_key
        .as_ref()
        .and_then(|k| state.storage.presign_get(k, ttl, None).map(|u| u.to_string()).ok());

    let preview_url = asset
        .preview_key
        .as_ref()
        .and_then(|k| state.storage.presign_get(k, ttl, None).map(|u| u.to_string()).ok());

    MediaResponse {
        id: asset.id,
        project_id: asset.project_id,
        filename: asset.original_filename,
        mime: asset.mime,
        bytes: asset.bytes,
        sha256: asset.sha256,
        width: asset.width,
        height: asset.height,
        download_url,
        thumb_url,
        preview_url,
        source: asset.source,
        prompt: asset.prompt,
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterMediaRequest {
    pub id: Option<Uuid>,
    pub storage_key: String,
    pub original_filename: String,
    pub mime: String,
    pub bytes: i64,
    pub sha256: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub thumb_key: Option<String>,
    pub preview_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MediaResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub filename: String,
    pub mime: String,
    pub bytes: i64,
    pub sha256: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub download_url: String,
    pub thumb_url: Option<String>,
    #[serde(default)]
    pub preview_url: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub prompt: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/media",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "List of project media assets", body = Vec<MediaResponse>),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn list_media(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let assets = pace_db::find_by_project(&state.db, project_id).await?;
    let responses: Vec<MediaResponse> = assets
        .into_iter()
        .filter(|asset| asset.kind == "image")
        .map(|asset| build_media_response(&state, asset))
        .collect();

    Ok((StatusCode::OK, Json(responses)))
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/media",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = RegisterMediaRequest,
    responses(
        (status = 201, description = "Media asset registered", body = MediaResponse),
        (status = 400, description = "Invalid media metadata or missing uploaded object", body = ProblemDetails),
        (status = 409, description = "Duplicate media asset", body = ProblemDetails),
        (status = 413, description = "Storage quota exceeded", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn register_media(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<RegisterMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    // 1. Validate MIME, sha256, and key prefixes owned by this user & project
    if !is_allowed_image_mime(&req.mime) {
        return Err(ApiError::BadRequest(
            "Unsupported image mime type. Allowed: image/jpeg, image/png, image/webp".to_string(),
        ));
    }

    let sha256 = req.sha256.trim().to_string();
    if sha256.is_empty() || sha256.len() > 128 {
        return Err(ApiError::BadRequest("Invalid sha256 digest".to_string()));
    }

    if !validate_project_media_key(&req.storage_key, auth.user.id, project_id) {
        return Err(ApiError::BadRequest("Invalid storage key prefix for project".to_string()));
    }

    if let Some(ref thumb_key) = req.thumb_key {
        if !validate_project_media_key(thumb_key, auth.user.id, project_id) {
            return Err(ApiError::BadRequest("Invalid thumb key prefix for project".to_string()));
        }
    }
    if let Some(ref preview_key) = req.preview_key {
        if !validate_project_media_key(preview_key, auth.user.id, project_id) {
            return Err(ApiError::BadRequest("Invalid preview key prefix for project".to_string()));
        }
    }

    // 2. Verify object actually exists in object storage and read authoritative byte size
    let head_info = state
        .storage
        .head(&req.storage_key)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Failed verifying object in storage: {e}")))?
        .ok_or_else(|| {
            ApiError::BadRequest("Uploaded media object not found in storage".to_string())
        })?;

    let authoritative_bytes = i64::try_from(head_info.size_bytes)
        .map_err(|_| ApiError::BadRequest("Invalid object size".to_string()))?;

    if authoritative_bytes <= 0 || authoritative_bytes > MAX_IMAGE_BYTES {
        cleanup_unregistered_keys(&state, &req).await;
        return Err(ApiError::BadRequest(format!(
            "Uploaded media size must be between 1 byte and {} MB",
            MAX_IMAGE_BYTES / (1024 * 1024)
        )));
    }

    // Verify optional derivative objects exist; drop dangling keys if absent
    let verified_preview_key = match req.preview_key.as_ref() {
        Some(key) if state.storage.head(key).await.ok().flatten().is_some() => Some(key.clone()),
        _ => None,
    };
    let verified_thumb_key = match req.thumb_key.as_ref() {
        Some(key) if state.storage.head(key).await.ok().flatten().is_some() => Some(key.clone()),
        _ => None,
    };

    // 3. Enforce per-project image cap
    let current_count = pace_db::count_by_project_and_kind(&state.db, project_id, "image").await?;
    if current_count >= MAX_IMAGES_PER_PROJECT {
        cleanup_unregistered_keys(&state, &req).await;
        return Err(ApiError::BadRequest(format!(
            "Project has reached the maximum of {MAX_IMAGES_PER_PROJECT} images"
        )));
    }

    // 4. Check for duplicate sha256 in project (MED-04)
    if let Some(existing) = pace_db::find_by_sha256(&state.db, project_id, &sha256, "image").await? {
        if existing.storage_key != req.storage_key {
            cleanup_unregistered_keys(&state, &req).await;
        }
        return Err(ApiError::Conflict(
            "An identical media file already exists in this project".to_string(),
        ));
    }

    let asset = match pace_db::create_media(
        &state.db,
        CreateMediaAsset {
            project_id,
            owner_id: auth.user.id,
            kind: "image".to_string(),
            storage_key: req.storage_key.clone(),
            preview_key: verified_preview_key,
            thumb_key: verified_thumb_key,
            original_filename: req.original_filename.clone(),
            mime: req.mime.clone(),
            bytes: authoritative_bytes,
            sha256,
            width: req.width,
            height: req.height,
            duration_ms: None,
            source: Some("upload".to_string()),
            prompt: None,
        },
    )
    .await
    {
        Ok(asset) => asset,
        Err(DbError::QuotaExceeded {
            current_bytes,
            max_bytes,
        }) => {
            cleanup_unregistered_keys(&state, &req).await;
            return Err(ApiError::QuotaExceeded {
                current_bytes,
                max_bytes,
            });
        }
        Err(err) => return Err(err.into()),
    };

    Ok((
        StatusCode::CREATED,
        Json(build_media_response(&state, asset)),
    ))
}

#[utoipa::path(
    delete,
    path = "/api/v1/projects/{id}/media/{media_id}",
    tag = "Media",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("media_id" = Uuid, Path, description = "Media Asset UUID")
    ),
    responses(
        (status = 204, description = "Media asset deleted"),
        (status = 404, description = "Not found", body = ProblemDetails)
    )
)]
pub async fn delete_media(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, media_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let asset = pace_db::delete_media(&state.db, project_id, media_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Media asset not found".to_string()))?;

    // Clean up original + derivatives from S3/MinIO storage
    let media_dir = asset
        .storage_key
        .rsplit_once('/')
        .map(|(dir, _)| format!("{dir}/"));

    if let Some(prefix) = media_dir {
        if prefix.starts_with(&format!("u/{}/p/{}/media/", asset.owner_id, asset.project_id)) {
            if let Err(err) = state.storage.delete_prefix(&prefix).await {
                tracing::warn!(error = %err, prefix = %prefix, "Failed deleting media folder prefix from storage");
            }
            return Ok(StatusCode::NO_CONTENT);
        }
    }

    let _ = state.storage.delete_prefix(&asset.storage_key).await;
    if let Some(ref preview) = asset.preview_key {
        let _ = state.storage.delete_prefix(preview).await;
    }
    if let Some(ref thumb) = asset.thumb_key {
        let _ = state.storage.delete_prefix(thumb).await;
    }

    Ok(StatusCode::NO_CONTENT)
}
