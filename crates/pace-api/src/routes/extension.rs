use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{Duration as ChronoDuration, Utc};
use pace_db::{CreateMediaAsset, DbError};
use pace_storage::StorageKeys;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{
        extractors::{AuthContext, ExtensionAuthContext},
        sessions::{generate_session_token, hash_session_token},
    },
    error::ApiError,
};

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_VIDEO_BYTES: u64 = 150 * 1024 * 1024;
const MAX_IMAGES_PER_PROJECT: i64 = 400;

fn generate_pairing_code_string() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    let mut part1 = String::with_capacity(4);
    let mut part2 = String::with_capacity(4);
    for _ in 0..4 {
        part1.push(ALPHABET[rng.gen_range(0..ALPHABET.len())] as char);
        part2.push(ALPHABET[rng.gen_range(0..ALPHABET.len())] as char);
    }
    format!("PACE-{part1}-{part2}")
}

fn normalize_pairing_code(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}

fn resolve_media_kind_and_ext(content_type: &str) -> Option<(&'static str, &'static str)> {
    match content_type.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => Some(("image", "jpg")),
        "image/png" => Some(("image", "png")),
        "image/webp" => Some(("image", "webp")),
        "video/mp4" => Some(("video", "mp4")),
        "video/webm" => Some(("video", "webm")),
        _ => None,
    }
}

fn validate_project_media_key(key: &str, user_id: Uuid, project_id: Uuid) -> bool {
    let expected_prefix = format!("u/{user_id}/p/{project_id}/media/");
    key.starts_with(&expected_prefix) && !key.contains("..") && !key.contains('\\')
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PairingCodeResponse {
    pub code: String,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PairExtensionRequest {
    pub code: String,
    #[serde(default)]
    pub device_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PairExtensionResponse {
    pub token: String,
    pub user_email: String,
    pub user_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtensionTokenDto {
    pub id: Uuid,
    pub device_name: String,
    pub last_project_id: Option<Uuid>,
    pub last_used_at: chrono::DateTime<Utc>,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtensionProjectDto {
    pub id: Uuid,
    pub name: String,
    pub aspect_ratio: String,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtensionProjectsResponse {
    pub projects: Vec<ExtensionProjectDto>,
    pub last_project_id: Option<Uuid>,
    pub user_email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtensionPromptItem {
    pub section_id: Uuid,
    pub position: i32,
    pub text: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtensionProjectPromptsResponse {
    pub project_id: Uuid,
    pub project_name: String,
    pub aspect_ratio: String,
    pub prompts: Vec<ExtensionPromptItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ExtensionPresignRequest {
    pub filename: String,
    pub content_type: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtensionPresignResponse {
    pub media_id: Uuid,
    pub kind: String,
    pub storage_key: String,
    pub upload_url: String,
    pub preview_key: String,
    pub preview_upload_url: String,
    pub thumb_key: String,
    pub thumb_upload_url: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ExtensionRegisterMediaRequest {
    pub id: Option<Uuid>,
    pub storage_key: String,
    pub original_filename: String,
    pub mime: String,
    pub sha256: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    pub preview_key: Option<String>,
    pub thumb_key: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtensionMediaRegisteredResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub kind: String,
    pub filename: String,
    pub prompt: Option<String>,
    pub duplicate: bool,
}

/// POST /api/v1/extension/pairing-code
/// Generates a 10-minute one-time pairing code for the Chrome extension (EXT-01).
pub async fn create_pairing_code(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<impl IntoResponse, ApiError> {
    let raw_code = generate_pairing_code_string();
    let code_hash = hash_session_token(&raw_code);
    let expires_at = Utc::now() + ChronoDuration::minutes(10);

    pace_db::create_extension_pairing_code(&state.db, auth.user.id, &code_hash, expires_at)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(PairingCodeResponse {
            code: raw_code,
            expires_at,
        }),
    ))
}

/// POST /api/v1/extension/pair
/// Exchanges a one-time pairing code for a scoped `pace_ext_...` Bearer token (EXT-01).
pub async fn pair_extension(
    State(state): State<AppState>,
    Json(req): Json<PairExtensionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let normalized = normalize_pairing_code(&req.code);
    if normalized.len() < 6 {
        return Err(ApiError::BadRequest("Invalid pairing code format".to_string()));
    }

    let code_hash = hash_session_token(&normalized);
    let consumed = pace_db::consume_extension_pairing_code(&state.db, &code_hash)
        .await?
        .ok_or_else(|| {
            ApiError::Unauthorized
        })?;

    let user = pace_db::find_user_by_id(&state.db, consumed.user_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    if !user.is_active {
        return Err(ApiError::Forbidden("Account is deactivated".into()));
    }

    let device_name = req
        .device_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Google Flow Chrome Extension")
        .chars()
        .take(80)
        .collect::<String>();

    let raw_token = format!("pace_ext_{}", generate_session_token());
    let token_hash = hash_session_token(&raw_token);

    pace_db::create_extension_token(&state.db, user.id, &device_name, &token_hash).await?;

    Ok((
        StatusCode::CREATED,
        Json(PairExtensionResponse {
            token: raw_token,
            user_email: user.email,
            user_name: user.name,
        }),
    ))
}

/// GET /api/v1/extension/tokens
/// Lists active Chrome extension connections for the signed-in user (EXT-01).
pub async fn list_extension_tokens(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<impl IntoResponse, ApiError> {
    let tokens = pace_db::list_active_extension_tokens(&state.db, auth.user.id).await?;
    let dtos: Vec<ExtensionTokenDto> = tokens
        .into_iter()
        .map(|t| ExtensionTokenDto {
            id: t.id,
            device_name: t.device_name,
            last_project_id: t.last_project_id,
            last_used_at: t.last_used_at,
            created_at: t.created_at,
        })
        .collect();

    Ok((StatusCode::OK, Json(dtos)))
}

/// DELETE /api/v1/extension/tokens/{id}
/// Revokes an extension token immediately (EXT-01).
pub async fn revoke_extension_token_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(token_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let revoked = pace_db::revoke_extension_token(&state.db, token_id, auth.user.id).await?;
    if !revoked {
        return Err(ApiError::NotFound("Extension connection not found".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/extension/projects
/// Returns the creator's active projects + last used project ID (EXT-03).
pub async fn list_extension_projects(
    State(state): State<AppState>,
    ext: ExtensionAuthContext,
) -> Result<impl IntoResponse, ApiError> {
    let projects = pace_db::list_projects_by_owner(&state.db, ext.user.id).await?;
    let items: Vec<ExtensionProjectDto> = projects
        .into_iter()
        .map(|p| ExtensionProjectDto {
            id: p.id,
            name: p.name,
            aspect_ratio: p.aspect_ratio,
            updated_at: p.updated_at,
        })
        .collect();

    Ok((
        StatusCode::OK,
        Json(ExtensionProjectsResponse {
            projects: items,
            last_project_id: ext.last_project_id,
            user_email: ext.user.email,
        }),
    ))
}

/// GET /api/v1/extension/projects/{id}/prompts
/// Returns ordered scene texts from the project's script so the Flow Queue Runner can load them.
pub async fn get_extension_project_prompts(
    State(state): State<AppState>,
    ext: ExtensionAuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, ext.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let sections = pace_db::get_sections_by_project(&state.db, project_id).await?;
    let _ = pace_db::touch_extension_token_project(&state.db, ext.token_id, Some(project_id)).await;

    let prompts: Vec<ExtensionPromptItem> = sections
        .into_iter()
        .map(|s| ExtensionPromptItem {
            section_id: s.id,
            position: s.position,
            text: s.text,
        })
        .collect();

    Ok((
        StatusCode::OK,
        Json(ExtensionProjectPromptsResponse {
            project_id: project.id,
            project_name: project.name,
            aspect_ratio: project.aspect_ratio,
            prompts,
        }),
    ))
}

/// POST /api/v1/extension/projects/{id}/uploads/presign
/// Generates presigned PUT URLs for an image or video generated in Google Flow.
pub async fn presign_extension_upload(
    State(state): State<AppState>,
    ext: ExtensionAuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<ExtensionPresignRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, ext.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let (kind, ext_str) = resolve_media_kind_and_ext(&req.content_type).ok_or_else(|| {
        ApiError::BadRequest(
            "Unsupported media type. Allowed: image/jpeg, image/png, image/webp, video/mp4, video/webm"
                .to_string(),
        )
    })?;

    let max_bytes = if kind == "video" {
        MAX_VIDEO_BYTES
    } else {
        MAX_IMAGE_BYTES
    };
    if req.bytes == 0 || req.bytes > max_bytes {
        return Err(ApiError::BadRequest(format!(
            "File size must be between 1 byte and {} MB",
            max_bytes / (1024 * 1024)
        )));
    }

    if kind == "image" {
        let current_count =
            pace_db::count_by_project_and_kind(&state.db, project_id, "image").await?;
        if current_count >= MAX_IMAGES_PER_PROJECT {
            return Err(ApiError::BadRequest(format!(
                "Project has reached the maximum of {MAX_IMAGES_PER_PROJECT} images"
            )));
        }
    }

    let user_db = pace_db::find_user_by_id(&state.db, ext.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;
    let remaining_quota = (user_db.storage_quota_bytes - user_db.storage_used_bytes).max(0) as u64;
    if req.bytes > remaining_quota {
        return Err(ApiError::Forbidden(
            "Storage quota exceeded. Delete unused media to free space.".to_string(),
        ));
    }

    let media_id = Uuid::new_v4();
    let storage_key = StorageKeys::media_original(&ext.user.id, &project_id, &media_id, ext_str);
    let preview_key = StorageKeys::media_preview(&ext.user.id, &project_id, &media_id);
    let thumb_key = StorageKeys::media_thumb(&ext.user.id, &project_id, &media_id);

    let ttl = Duration::from_secs(3600);
    let upload_url = state
        .storage
        .presign_put(&storage_key, req.content_type.trim(), req.bytes, ttl)
        .map_err(|e| ApiError::Internal(format!("Failed generating upload URL: {e}")))?
        .to_string();

    let (preview_upload_url, thumb_upload_url) = if kind == "image" {
        let p_url = state
            .storage
            .presign_put(&preview_key, "image/webp", MAX_IMAGE_BYTES, ttl)
            .map_err(|e| ApiError::Internal(format!("Failed generating preview URL: {e}")))?
            .to_string();
        let t_url = state
            .storage
            .presign_put(&thumb_key, "image/webp", MAX_IMAGE_BYTES, ttl)
            .map_err(|e| ApiError::Internal(format!("Failed generating thumb URL: {e}")))?
            .to_string();
        (p_url, t_url)
    } else {
        (String::new(), String::new())
    };

    Ok((
        StatusCode::OK,
        Json(ExtensionPresignResponse {
            media_id,
            kind: kind.to_string(),
            storage_key,
            upload_url,
            preview_key,
            preview_upload_url,
            thumb_key,
            thumb_upload_url,
        }),
    ))
}

/// POST /api/v1/extension/projects/{id}/media
/// Registers a Flow-generated image or video in PACE with `source = "extension"` and `prompt` (EXT-04).
pub async fn register_extension_media(
    State(state): State<AppState>,
    ext: ExtensionAuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<ExtensionRegisterMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, ext.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let (kind, _) = resolve_media_kind_and_ext(&req.mime).ok_or_else(|| {
        ApiError::BadRequest("Unsupported media mime type".to_string())
    })?;

    let sha256 = req.sha256.trim().to_string();
    if sha256.is_empty() || sha256.len() > 128 {
        return Err(ApiError::BadRequest("Invalid sha256 digest".to_string()));
    }

    if !validate_project_media_key(&req.storage_key, ext.user.id, project_id) {
        return Err(ApiError::BadRequest("Invalid storage key for project".to_string()));
    }

    // Deduplicate gracefully: if already present, clean up redundant object and return existing ID
    if let Some(existing) = pace_db::find_by_sha256(&state.db, project_id, &sha256, kind).await? {
        if existing.storage_key != req.storage_key {
            let _ = state.storage.delete_prefix(&req.storage_key).await;
            if let Some(ref k) = req.preview_key {
                let _ = state.storage.delete_prefix(k).await;
            }
            if let Some(ref k) = req.thumb_key {
                let _ = state.storage.delete_prefix(k).await;
            }
        }
        let _ =
            pace_db::touch_extension_token_project(&state.db, ext.token_id, Some(project_id)).await;
        return Ok((
            StatusCode::OK,
            Json(ExtensionMediaRegisteredResponse {
                id: existing.id,
                project_id,
                kind: existing.kind,
                filename: existing.original_filename,
                prompt: existing.prompt,
                duplicate: true,
            }),
        ));
    }

    let head_info = state
        .storage
        .head(&req.storage_key)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Failed verifying object: {e}")))?
        .ok_or_else(|| {
            ApiError::BadRequest("Uploaded media object not found in storage".to_string())
        })?;

    let authoritative_bytes = i64::try_from(head_info.size_bytes)
        .map_err(|_| ApiError::BadRequest("Invalid object size".to_string()))?;

    let verified_preview_key = match req.preview_key.as_ref() {
        Some(key) if validate_project_media_key(key, ext.user.id, project_id)
            && state.storage.head(key).await.ok().flatten().is_some() =>
        {
            Some(key.clone())
        }
        _ => None,
    };
    let verified_thumb_key = match req.thumb_key.as_ref() {
        Some(key) if validate_project_media_key(key, ext.user.id, project_id)
            && state.storage.head(key).await.ok().flatten().is_some() =>
        {
            Some(key.clone())
        }
        _ => None,
    };

    let clean_prompt = req
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(2000).collect::<String>());

    let asset = match pace_db::create_media(
        &state.db,
        CreateMediaAsset {
            project_id,
            owner_id: ext.user.id,
            kind: kind.to_string(),
            storage_key: req.storage_key.clone(),
            preview_key: verified_preview_key,
            thumb_key: verified_thumb_key,
            original_filename: req.original_filename.clone(),
            mime: req.mime.clone(),
            bytes: authoritative_bytes,
            sha256,
            width: req.width,
            height: req.height,
            duration_ms: req.duration_ms,
            source: Some("extension".to_string()),
            prompt: clean_prompt,
        },
    )
    .await
    {
        Ok(asset) => asset,
        Err(DbError::QuotaExceeded {
            current_bytes,
            max_bytes,
        }) => {
            let _ = state.storage.delete_prefix(&req.storage_key).await;
            return Err(ApiError::QuotaExceeded {
                current_bytes,
                max_bytes,
            });
        }
        Err(err) => return Err(err.into()),
    };

    let _ = pace_db::touch_extension_token_project(&state.db, ext.token_id, Some(project_id)).await;

    Ok((
        StatusCode::CREATED,
        Json(ExtensionMediaRegisteredResponse {
            id: asset.id,
            project_id,
            kind: asset.kind,
            filename: asset.original_filename,
            prompt: asset.prompt,
            duplicate: false,
        }),
    ))
}
