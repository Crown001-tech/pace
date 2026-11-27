use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct MediaAsset {
    pub id: Uuid,
    pub project_id: Uuid,
    pub owner_id: Uuid,
    pub kind: String,
    pub storage_key: String,
    pub preview_key: Option<String>,
    pub thumb_key: Option<String>,
    pub original_filename: String,
    pub mime: String,
    pub bytes: i64,
    pub sha256: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    pub source: String,
    pub prompt: Option<String>,
    pub status: String,
    pub probe: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMediaAsset {
    pub project_id: Uuid,
    pub owner_id: Uuid,
    pub kind: String,
    pub storage_key: String,
    pub preview_key: Option<String>,
    pub thumb_key: Option<String>,
    pub original_filename: String,
    pub mime: String,
    pub bytes: i64,
    pub sha256: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    pub source: Option<String>,
    pub prompt: Option<String>,
}

