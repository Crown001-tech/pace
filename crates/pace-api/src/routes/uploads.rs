use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
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
};

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024; // 20 MB per image (MED-01)
const MAX_IMAGES_PER_PROJECT: i64 = 400; // MED-01

fn allowed_image_extension(content_type: &str) -> Option<&'static str> {
    match content_type.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PresignUploadRequest {
    pub filename: String,
    pub content_type: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PresignUploadResponse {
    pub media_id: Uuid,
    pub storage_key: String,
    pub upload_url: String,
    #[serde(default)]
    pub preview_key: String,
    #[serde(default)]
    pub preview_upload_url: String,
    #[serde(default)]
    pub thumb_key: String,
    #[serde(default)]
    pub thumb_upload_url: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/uploads/presign",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = PresignUploadRequest,
    responses(
        (status = 200, description = "Pre-signed PUT URLs generated", body = PresignUploadResponse),
        (status = 400, description = "Unsupported file type, invalid size, or project image cap reached", body = ProblemDetails),
        (status = 403, description = "Storage quota exceeded", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn presign_upload(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<PresignUploadRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // 1. Verify project exists and belongs to authenticated owner (AUTH-05)
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    // 2. Validate image MIME and file size (MED-01)
    let ext = allowed_image_extension(&req.content_type).ok_or_else(|| {
        ApiError::BadRequest("Unsupported image content type. Allowed: image/jpeg, image/png, image/webp".to_string())
    })?;

    if req.bytes == 0 {
        return Err(ApiError::BadRequest("File size must be greater than 0 bytes".to_string()));
    }
    if req.bytes > MAX_IMAGE_BYTES {
        return Err(ApiError::BadRequest(format!(
            "Image exceeds maximum allowed size of {} MB",
            MAX_IMAGE_BYTES / (1024 * 1024)
        )));
    }

    // 3. Enforce per-project image limit (MED-01)
    let current_count = pace_db::count_by_project_and_kind(&state.db, project_id, "image").await?;
    if current_count >= MAX_IMAGES_PER_PROJECT {
        return Err(ApiError::BadRequest(format!(
            "Project has reached the maximum of {MAX_IMAGES_PER_PROJECT} images"
        )));
    }

    // 4. Check storage quota (STO-01)
    let user_db = pace_db::find_user_by_id(&state.db, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    let remaining_quota = (user_db.storage_quota_bytes - user_db.storage_used_bytes).max(0) as u64;
    if req.bytes > remaining_quota {
        return Err(ApiError::Forbidden(
            "Storage limit exceeded. Delete unused media or upgrade quota.".to_string(),
        ));
    }

    // 5. Generate storage keys according to TDD 9.1
    let media_id = Uuid::new_v4();
    let storage_key = StorageKeys::media_original(&auth.user.id, &project_id, &media_id, ext);
    let preview_key = StorageKeys::media_preview(&auth.user.id, &project_id, &media_id);
    let thumb_key = StorageKeys::media_thumb(&auth.user.id, &project_id, &media_id);

    // 6. Generate 60-minute pre-signed PUT URLs (TDD 9.2)
    let ttl = Duration::from_secs(3600);
    let upload_url = state
        .storage
        .presign_put(&storage_key, req.content_type.trim(), req.bytes, ttl)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let preview_upload_url = state
        .storage
        .presign_put(&preview_key, "image/webp", MAX_IMAGE_BYTES, ttl)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let thumb_upload_url = state
        .storage
        .presign_put(&thumb_key, "image/webp", MAX_IMAGE_BYTES, ttl)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok((
        StatusCode::OK,
        Json(PresignUploadResponse {
            media_id,
            storage_key,
            upload_url: upload_url.to_string(),
            preview_key,
            preview_upload_url: preview_upload_url.to_string(),
            thumb_key,
            thumb_upload_url: thumb_upload_url.to_string(),
        }),
    ))
}
