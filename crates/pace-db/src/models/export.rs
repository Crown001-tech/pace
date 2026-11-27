use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExportRecord {
    pub id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub job_id: Option<Uuid>,
    pub spec_hash: String,
    pub resolution: String,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub duration_ms: i32,
    pub status: String,
    pub storage_key: Option<String>,
    pub size_bytes: Option<i64>,
    pub validation: serde_json::Value,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CreateExport {
    pub id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub job_id: Option<Uuid>,
    pub spec_hash: String,
    pub resolution: String,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub duration_ms: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct RenderSegmentRecord {
    pub cache_key: String,
    pub project_id: Uuid,
    pub storage_key: String,
    pub frame_count: i32,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}
