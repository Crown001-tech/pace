use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pace_db::CreateMediaAsset;
use pace_storage::StorageKeys;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::time::Duration;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthContext,
    error::{ApiError, ProblemDetails},
    routes::media::MediaResponse,
};

const MAX_REMOTE_IMAGE_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Deserialize, IntoParams)]
pub struct StockSearchQuery {
    pub q: String,
    pub per_page: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StockMediaItem {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub thumb_url: String,
    pub image_url: String,
    pub width: i32,
    pub height: i32,
    pub author: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StockSearchResponse {
    pub provider: String,
    pub query: String,
    pub items: Vec<StockMediaItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportStockMediaRequest {
    pub image_url: String,
    pub title: Option<String>,
    pub provider: Option<String>,
    pub section_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AutoMatchStockRequest {
    pub section_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AutoMatchedSceneItem {
    pub section_id: Uuid,
    pub query: String,
    pub media: MediaResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AutoMatchStockResponse {
    pub matched_count: usize,
    pub skipped_count: usize,
    pub matched: Vec<AutoMatchedSceneItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GenerateAiMediaRequest {
    pub prompt: String,
    pub section_id: Option<Uuid>,
    pub style: Option<String>,
}

fn build_http_client() -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .user_agent("PACE-Studio/1.0 (https://pace.local)")
        .build()
        .map_err(|e| ApiError::Internal(format!("HTTP client error: {e}")))
}

fn url_encode_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() * 2);
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Extracts concise visual search keywords from narration script text for stock auto-matching.
pub fn extract_visual_keywords(text: &str) -> String {
    const STOPWORDS: &[&str] = &[
        "the", "and", "that", "this", "with", "from", "your", "into", "about", "there", "their",
        "they", "them", "these", "those", "when", "where", "which", "while", "what", "will",
        "would", "could", "should", "have", "has", "had", "been", "being", "were", "was", "are",
        "for", "not", "but", "you", "our", "out", "over", "under", "then", "than", "just",
        "like", "very", "more", "most", "some", "such", "only", "every", "single", "without",
        "one", "two", "three", "begins", "begin", "starts", "start", "appears", "appear",
    ];
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .map(|w| w.trim().to_ascii_lowercase())
        .filter(|w| w.len() >= 4 && !stop.contains(w.as_str()))
        .take(4)
        .collect();

    if words.is_empty() {
        "cinematic landscape".to_string()
    } else {
        words.join(" ")
    }
}

async fn query_pexels(
    client: &reqwest::Client,
    api_key: &str,
    query: &str,
    aspect_ratio: &str,
    per_page: usize,
) -> Result<Vec<StockMediaItem>, ApiError> {
    let orientation = if aspect_ratio == "9:16" {
        "portrait"
    } else {
        "landscape"
    };
    let url = format!(
        "https://api.pexels.com/v1/search?query={}&orientation={orientation}&per_page={}",
        url_encode_component(query),
        per_page.clamp(1, 20)
    );
    let resp = client
        .get(&url)
        .header("Authorization", api_key)
        .send()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Pexels request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(ApiError::ServiceUnavailable(format!(
            "Pexels API returned HTTP {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Invalid Pexels JSON: {e}")))?;

    let mut items = Vec::new();
    if let Some(photos) = body.get("photos").and_then(|v| v.as_array()) {
        for p in photos {
            let id = p.get("id").and_then(|v| v.as_i64()).unwrap_or_default();
            let src = p.get("src");
            let image_url = src
                .and_then(|s| s.get("large2x").or_else(|| s.get("original")))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let thumb_url = src
                .and_then(|s| s.get("medium").or_else(|| s.get("small")))
                .and_then(|v| v.as_str())
                .unwrap_or(&image_url)
                .to_string();
            if image_url.is_empty() {
                continue;
            }
            items.push(StockMediaItem {
                id: format!("pexels-{id}"),
                provider: "pexels".to_string(),
                title: p
                    .get("alt")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(query)
                    .to_string(),
                thumb_url,
                image_url,
                width: p.get("width").and_then(|v| v.as_i64()).unwrap_or(1920) as i32,
                height: p.get("height").and_then(|v| v.as_i64()).unwrap_or(1080) as i32,
                author: p
                    .get("photographer")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Pexels")
                    .to_string(),
            });
        }
    }
    Ok(items)
}

async fn query_pixabay(
    client: &reqwest::Client,
    api_key: &str,
    query: &str,
    aspect_ratio: &str,
    per_page: usize,
) -> Result<Vec<StockMediaItem>, ApiError> {
    let orientation = if aspect_ratio == "9:16" {
        "vertical"
    } else {
        "horizontal"
    };
    let url = format!(
        "https://pixabay.com/api/?key={}&q={}&image_type=photo&orientation={orientation}&safesearch=true&per_page={}",
        url_encode_component(api_key),
        url_encode_component(query),
        per_page.clamp(3, 20)
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Pixabay request failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(ApiError::ServiceUnavailable(format!(
            "Pixabay API returned HTTP {}",
            resp.status()
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Invalid Pixabay JSON: {e}")))?;

    let mut items = Vec::new();
    if let Some(hits) = body.get("hits").and_then(|v| v.as_array()) {
        for h in hits {
            let id = h.get("id").and_then(|v| v.as_i64()).unwrap_or_default();
            let image_url = h
                .get("largeImageURL")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let thumb_url = h
                .get("webformatURL")
                .and_then(|v| v.as_str())
                .unwrap_or(&image_url)
                .to_string();
            if image_url.is_empty() {
                continue;
            }
            items.push(StockMediaItem {
                id: format!("pixabay-{id}"),
                provider: "pixabay".to_string(),
                title: h
                    .get("tags")
                    .and_then(|v| v.as_str())
                    .unwrap_or(query)
                    .to_string(),
                thumb_url,
                image_url,
                width: h.get("imageWidth").and_then(|v| v.as_i64()).unwrap_or(1920) as i32,
                height: h.get("imageHeight").and_then(|v| v.as_i64()).unwrap_or(1080) as i32,
                author: h
                    .get("user")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Pixabay")
                    .to_string(),
            });
        }
    }
    Ok(items)
}

async fn query_wikimedia_commons(
    client: &reqwest::Client,
    query: &str,
    per_page: usize,
) -> Result<Vec<StockMediaItem>, ApiError> {
    let url = format!(
        "https://commons.wikimedia.org/w/api.php?action=query&generator=search&gsrnamespace=6&gsrsearch={}%20filetype:bitmap&gsrlimit={}&prop=imageinfo&iiprop=url|size|mime&iiurlwidth=1280&format=json",
        url_encode_component(query),
        per_page.clamp(1, 15)
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Open stock search failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(ApiError::ServiceUnavailable(format!(
            "Open stock search returned HTTP {}",
            resp.status()
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Invalid open stock JSON: {e}")))?;

    let mut items = Vec::new();
    if let Some(pages) = body
        .get("query")
        .and_then(|q| q.get("pages"))
        .and_then(|p| p.as_object())
    {
        for (page_id, page) in pages {
            let Some(info) = page
                .get("imageinfo")
                .and_then(|arr| arr.as_array())
                .and_then(|arr| arr.first())
            else {
                continue;
            };
            let mime = info
                .get("mime")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if !matches!(mime, "image/jpeg" | "image/png" | "image/webp") {
                continue;
            }
            let image_url = info
                .get("thumburl")
                .or_else(|| info.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if image_url.is_empty() {
                continue;
            }
            let title = page
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or(query)
                .trim_start_matches("File:")
                .to_string();
            items.push(StockMediaItem {
                id: format!("commons-{page_id}"),
                provider: "wikimedia".to_string(),
                title,
                thumb_url: image_url.clone(),
                image_url,
                width: info.get("thumbwidth").or_else(|| info.get("width")).and_then(|v| v.as_i64()).unwrap_or(1280) as i32,
                height: info.get("thumbheight").or_else(|| info.get("height")).and_then(|v| v.as_i64()).unwrap_or(720) as i32,
                author: "Wikimedia Commons".to_string(),
            });
        }
    }
    Ok(items)
}

async fn run_stock_search(
    state: &AppState,
    query: &str,
    aspect_ratio: &str,
    per_page: usize,
) -> Result<(String, Vec<StockMediaItem>), ApiError> {
    let client = build_http_client()?;
    if let Some(ref key) = state.config.pexels_api_key {
        let items = query_pexels(&client, key, query, aspect_ratio, per_page).await?;
        if !items.is_empty() {
            return Ok(("pexels".to_string(), items));
        }
    }
    if let Some(ref key) = state.config.pixabay_api_key {
        let items = query_pixabay(&client, key, query, aspect_ratio, per_page).await?;
        if !items.is_empty() {
            return Ok(("pixabay".to_string(), items));
        }
    }
    let items = query_wikimedia_commons(&client, query, per_page).await?;
    Ok(("wikimedia".to_string(), items))
}

async fn store_image_bytes_as_media(
    state: &AppState,
    user_id: Uuid,
    project_id: Uuid,
    bytes: Vec<u8>,
    filename_hint: &str,
    source: &str,
    prompt: Option<String>,
) -> Result<MediaResponse, ApiError> {
    if bytes.is_empty() || bytes.len() > MAX_REMOTE_IMAGE_BYTES {
        return Err(ApiError::BadRequest(
            "Image size must be between 1 byte and 20 MB".to_string(),
        ));
    }

    let format = image::guess_format(&bytes)
        .map_err(|_| ApiError::BadRequest("Downloaded file is not a valid image".to_string()))?;
    let (mime, ext) = match format {
        image::ImageFormat::Png => ("image/png", "png"),
        image::ImageFormat::WebP => ("image/webp", "webp"),
        _ => ("image/jpeg", "jpg"),
    };

    let decoded = image::load_from_memory(&bytes)
        .map_err(|e| ApiError::BadRequest(format!("Failed decoding image: {e}")))?;
    let width = i32::try_from(decoded.width()).ok();
    let height = i32::try_from(decoded.height()).ok();

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha256 = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    let ttl = Duration::from_secs(3600);
    if let Some(existing) = pace_db::find_by_sha256(&state.db, project_id, &sha256, "image").await?
    {
        return Ok(MediaResponse {
            id: existing.id,
            project_id: existing.project_id,
            filename: existing.original_filename,
            mime: existing.mime,
            bytes: existing.bytes,
            sha256: existing.sha256,
            width: existing.width,
            height: existing.height,
            download_url: state
                .storage
                .presign_get(&existing.storage_key, ttl, None)
                .map(|u| u.to_string())
                .unwrap_or_default(),
            thumb_url: existing
                .thumb_key
                .as_ref()
                .and_then(|k| state.storage.presign_get(k, ttl, None).map(|u| u.to_string()).ok()),
            preview_url: existing
                .preview_key
                .as_ref()
                .and_then(|k| state.storage.presign_get(k, ttl, None).map(|u| u.to_string()).ok()),
            source: existing.source,
            prompt: existing.prompt,
        });
    }

    let media_id = Uuid::now_v7();
    let storage_key = StorageKeys::media_original(&user_id, &project_id, &media_id, ext);

    let tmp_path = std::env::temp_dir().join(format!("pace-import-{media_id}.{ext}"));
    tokio::fs::write(&tmp_path, &bytes)
        .await
        .map_err(|e| ApiError::Internal(format!("Temp file write failed: {e}")))?;
    let upload_res = state.storage.upload(&tmp_path, &storage_key, mime).await;
    let _ = tokio::fs::remove_file(&tmp_path).await;
    upload_res.map_err(|e| ApiError::ServiceUnavailable(format!("Storage upload failed: {e}")))?;

    let clean_name: String = filename_hint
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(48)
        .collect();
    let original_filename = if clean_name.is_empty() {
        format!("{source}_{media_id}.{ext}")
    } else {
        format!("{clean_name}.{ext}")
    };

    let asset = pace_db::create_media(
        &state.db,
        CreateMediaAsset {
            project_id,
            owner_id: user_id,
            kind: "image".to_string(),
            storage_key: storage_key.clone(),
            preview_key: Some(storage_key.clone()),
            thumb_key: Some(storage_key),
            original_filename,
            mime: mime.to_string(),
            bytes: bytes.len() as i64,
            sha256,
            width,
            height,
            duration_ms: None,
            source: Some(source.to_string()),
            prompt,
        },
    )
    .await?;

    let download_url = state
        .storage
        .presign_get(&asset.storage_key, ttl, None)
        .map(|u| u.to_string())
        .unwrap_or_default();

    Ok(MediaResponse {
        id: asset.id,
        project_id: asset.project_id,
        filename: asset.original_filename,
        mime: asset.mime,
        bytes: asset.bytes,
        sha256: asset.sha256,
        width: asset.width,
        height: asset.height,
        download_url: download_url.clone(),
        thumb_url: Some(download_url.clone()),
        preview_url: Some(download_url),
        source: asset.source,
        prompt: asset.prompt,
    })
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/media/stock/search",
    tag = "Media",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        StockSearchQuery
    ),
    responses(
        (status = 200, description = "Free stock photo search results", body = StockSearchResponse),
        (status = 400, description = "Empty query", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn search_stock_media(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Query(params): Query<StockSearchQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let query = params.q.trim();
    if query.is_empty() {
        return Err(ApiError::BadRequest("Search query q cannot be empty".to_string()));
    }

    let (provider, items) = run_stock_search(
        &state,
        query,
        &project.aspect_ratio,
        params.per_page.unwrap_or(12),
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(StockSearchResponse {
            provider,
            query: query.to_string(),
            items,
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/media/stock/import",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = ImportStockMediaRequest,
    responses(
        (status = 201, description = "Stock image imported into project library", body = MediaResponse),
        (status = 400, description = "Invalid image URL", body = ProblemDetails)
    )
)]
pub async fn import_stock_media(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<ImportStockMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let url = req.image_url.trim();
    if !url.starts_with("https://") {
        return Err(ApiError::BadRequest("image_url must use https://".to_string()));
    }

    let client = build_http_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Failed downloading stock image: {e}")))?;
    if !resp.status().is_success() {
        return Err(ApiError::ServiceUnavailable(format!(
            "Stock image download returned HTTP {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Failed reading stock image bytes: {e}")))?
        .to_vec();

    let source_label = req.provider.as_deref().unwrap_or("stock");
    let media = store_image_bytes_as_media(
        &state,
        auth.user.id,
        project_id,
        bytes,
        req.title.as_deref().unwrap_or(source_label),
        "stock",
        None,
    )
    .await?;

    if let Some(section_id) = req.section_id {
        pace_db::assign_media(&state.db, project_id, section_id, media.id, "fill", 0.5, 0.5)
            .await?;
    }

    Ok((StatusCode::CREATED, Json(media)))
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/media/stock/auto-match",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = AutoMatchStockRequest,
    responses(
        (status = 200, description = "Auto-matched empty scenes with stock photos", body = AutoMatchStockResponse)
    )
)]
pub async fn auto_match_stock_media(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<AutoMatchStockRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let sections = pace_db::get_sections_by_project(&state.db, project_id).await?;
    let assignments = pace_db::get_assignments_by_project(&state.db, project_id).await?;
    let assigned_sections: HashSet<Uuid> = assignments.into_iter().map(|a| a.section_id).collect();
    let requested_set: Option<HashSet<Uuid>> = req.section_ids.map(|v| v.into_iter().collect());

    let client = build_http_client()?;
    let mut matched = Vec::new();
    let mut skipped_count = 0usize;
    let mut used_urls = HashSet::new();

    for sec in sections {
        if let Some(ref filter) = requested_set {
            if !filter.contains(&sec.id) {
                continue;
            }
        } else if assigned_sections.contains(&sec.id) {
            continue;
        }

        let query = extract_visual_keywords(&sec.text);
        let (_, items) = match run_stock_search(&state, &query, &project.aspect_ratio, 6).await {
            Ok(res) => res,
            Err(_) => {
                skipped_count += 1;
                continue;
            }
        };

        let Some(chosen) = items.into_iter().find(|i| !used_urls.contains(&i.image_url)) else {
            skipped_count += 1;
            continue;
        };
        used_urls.insert(chosen.image_url.clone());

        let Ok(resp) = client.get(&chosen.image_url).send().await else {
            skipped_count += 1;
            continue;
        };
        if !resp.status().is_success() {
            skipped_count += 1;
            continue;
        }
        let Ok(bytes) = resp.bytes().await else {
            skipped_count += 1;
            continue;
        };

        let Ok(media) = store_image_bytes_as_media(
            &state,
            auth.user.id,
            project_id,
            bytes.to_vec(),
            &format!("stock_{}_{}", sec.position + 1, query),
            "stock",
            None,
        )
        .await
        else {
            skipped_count += 1;
            continue;
        };

        pace_db::assign_media(&state.db, project_id, sec.id, media.id, "fill", 0.5, 0.5)
            .await?;

        matched.push(AutoMatchedSceneItem {
            section_id: sec.id,
            query,
            media,
        });
    }

    Ok((
        StatusCode::OK,
        Json(AutoMatchStockResponse {
            matched_count: matched.len(),
            skipped_count,
            matched,
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{id}/media/ai/generate",
    tag = "Media",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = GenerateAiMediaRequest,
    responses(
        (status = 201, description = "AI image generated and saved to project library", body = MediaResponse),
        (status = 400, description = "Empty prompt", body = ProblemDetails)
    )
)]
pub async fn generate_ai_media(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<GenerateAiMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let raw_prompt = req.prompt.trim();
    if raw_prompt.is_empty() {
        return Err(ApiError::BadRequest("Prompt cannot be empty".to_string()));
    }

    let styled_prompt = match req.style.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(style) => format!("{raw_prompt}, {style}"),
        None => format!("{raw_prompt}, cinematic documentary photography, rich lighting, ultra detailed"),
    };

    let (w, h) = if project.aspect_ratio == "9:16" {
        (720, 1280)
    } else {
        (1280, 720)
    };

    let client = build_http_client()?;
    let bytes = if let (Some(acc_id), Some(token)) = (
        state.config.cloudflare_account_id.as_ref(),
        state.config.cloudflare_api_token.as_ref(),
    ) {
        let cf_url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{acc_id}/ai/run/@cf/black-forest-labs/flux-1-schnell"
        );
        let resp = client
            .post(&cf_url)
            .bearer_auth(token)
            .json(&serde_json::json!({ "prompt": styled_prompt }))
            .send()
            .await
            .map_err(|e| ApiError::ServiceUnavailable(format!("Cloudflare AI failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::ServiceUnavailable(format!(
                "Cloudflare AI returned HTTP {}",
                resp.status()
            )));
        }
        let json_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::ServiceUnavailable(format!("Invalid Cloudflare AI JSON: {e}")))?;
        let b64 = json_body
            .get("result")
            .and_then(|r| r.get("image"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::ServiceUnavailable("Missing image in Cloudflare AI response".to_string()))?;
        decode_base64_simple(b64)
            .map_err(|e| ApiError::ServiceUnavailable(format!("Invalid base64 image: {e}")))?
    } else {
        generate_zero_cost_ai_image(&client, &styled_prompt, &project.aspect_ratio, w, h).await?
    };

    let media = store_image_bytes_as_media(
        &state,
        auth.user.id,
        project_id,
        bytes,
        "ai_scene",
        "ai_gen",
        Some(raw_prompt.to_string()),
    )
    .await?;

    if let Some(section_id) = req.section_id {
        pace_db::assign_media(&state.db, project_id, section_id, media.id, "fill", 0.5, 0.5)
            .await?;
    }

    Ok((StatusCode::CREATED, Json(media)))
}

async fn generate_zero_cost_ai_image(
    client: &reqwest::Client,
    styled_prompt: &str,
    aspect_ratio: &str,
    w: u32,
    h: u32,
) -> Result<Vec<u8>, ApiError> {
    // 1. Try zero-key Pollinations first (fast when available)
    let seed = rand::random::<u32>() % 999_999;
    let pollinations_url = format!(
        "https://image.pollinations.ai/prompt/{}?width={w}&height={h}&nologo=true&seed={seed}",
        url_encode_component(styled_prompt)
    );
    let pollinations_err = match client.get(&pollinations_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(bytes) = resp.bytes().await {
                if !bytes.is_empty() && image::guess_format(&bytes).is_ok() {
                    return Ok(bytes.to_vec());
                }
            }
            "Pollinations returned non-image response".to_string()
        }
        Ok(resp) => format!("Pollinations HTTP {}", resp.status()),
        Err(e) => format!("Pollinations error: {e}"),
    };
    tracing::warn!(%pollinations_err, "Pollinations unavailable, falling back to Stable Horde");

    // 2. Fallback to Stable Horde anonymous $0 generation (max 576px & multiples of 64 for anonymous key)
    let (sh_w, sh_h) = if aspect_ratio == "9:16" {
        (320, 576)
    } else {
        (576, 320)
    };
    let submit_resp = client
        .post("https://stablehorde.net/api/v2/generate/async")
        .header("apikey", "0000000000")
        .header("Client-Agent", "PACE:0.1.0:pace-local")
        .json(&serde_json::json!({
            "prompt": styled_prompt,
            "params": {
                "width": sh_w,
                "height": sh_h,
                "steps": 16,
                "n": 1
            },
            "nsfw": false,
            "censor_nsfw": true,
            "r2": true
        }))
        .send()
        .await
        .map_err(|e| {
            ApiError::ServiceUnavailable(format!(
                "AI image generators unavailable ({pollinations_err}; Stable Horde submit failed: {e})"
            ))
        })?;

    if !submit_resp.status().is_success() {
        let status = submit_resp.status();
        return Err(ApiError::ServiceUnavailable(format!(
            "AI image generators unavailable ({pollinations_err}; Stable Horde HTTP {status})"
        )));
    }

    let submit_json: serde_json::Value = submit_resp.json().await.map_err(|e| {
        ApiError::ServiceUnavailable(format!("Invalid Stable Horde submit response: {e}"))
    })?;
    let job_id = submit_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::ServiceUnavailable("Stable Horde did not return a job id".to_string())
        })?;

    let check_url = format!("https://stablehorde.net/api/v2/generate/check/{job_id}");
    let mut done = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let chk_resp = match client.get(&check_url).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => continue,
        };
        let Ok(chk_json) = chk_resp.json::<serde_json::Value>().await else {
            continue;
        };
        if chk_json.get("faulted").and_then(|v| v.as_bool()) == Some(true) {
            return Err(ApiError::ServiceUnavailable(
                "AI image generator job faulted; please try again".to_string(),
            ));
        }
        if chk_json.get("done").and_then(|v| v.as_bool()) == Some(true) {
            done = true;
            break;
        }
    }

    if !done {
        return Err(ApiError::ServiceUnavailable(format!(
            "AI image generator timed out ({pollinations_err}; Stable Horde still queued)"
        )));
    }

    let status_url = format!("https://stablehorde.net/api/v2/generate/status/{job_id}");
    let status_resp = client
        .get(&status_url)
        .send()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Stable Horde status failed: {e}")))?;
    let status_json: serde_json::Value = status_resp
        .json()
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Stable Horde status parse error: {e}")))?;
    let img_val = status_json
        .get("generations")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|g| g.get("img"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::ServiceUnavailable("Stable Horde returned no image".to_string())
        })?;

    if img_val.starts_with("http://") || img_val.starts_with("https://") {
        let img_resp = client
            .get(img_val)
            .send()
            .await
            .map_err(|e| ApiError::ServiceUnavailable(format!("Failed fetching AI image: {e}")))?;
        if !img_resp.status().is_success() {
            return Err(ApiError::ServiceUnavailable(format!(
                "AI image download returned HTTP {}",
                img_resp.status()
            )));
        }
        Ok(img_resp
            .bytes()
            .await
            .map_err(|e| ApiError::ServiceUnavailable(format!("Failed reading AI image bytes: {e}")))?
            .to_vec())
    } else {
        decode_base64_simple(img_val).map_err(|e| {
            ApiError::ServiceUnavailable(format!("Invalid Stable Horde base64 image: {e}"))
        })
    }
}

fn decode_base64_simple(input: &str) -> Result<Vec<u8>, &'static str> {
    let clean: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if clean.len() % 4 != 0 {
        return Err("invalid base64 length");
    }
    let val = |c: u8| -> Result<u8, &'static str> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("invalid base64 char"),
        }
    };
    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    for chunk in clean.chunks_exact(4) {
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = val(chunk[2])?;
            out.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                let d = val(chunk[3])?;
                out.push((c << 6) | d);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_visual_keywords_strips_stopwords_and_keeps_concrete_nouns() {
        let text = "Every single night, without a conscious command, your brain flips a hidden switch inside a glowing forest.";
        let kw = extract_visual_keywords(text);
        assert!(kw.contains("night"));
        assert!(kw.contains("conscious") || kw.contains("command") || kw.contains("brain"));
        assert!(!kw.contains("every"));
    }

    #[test]
    fn decode_base64_simple_roundtrips_bytes() {
        assert_eq!(decode_base64_simple("SGVsbG8=").unwrap(), b"Hello");
        assert_eq!(decode_base64_simple("SGk=").unwrap(), b"Hi");
    }
}
