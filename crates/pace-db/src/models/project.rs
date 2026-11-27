use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub aspect_ratio: String,
    pub fps: i32,
    pub language: String,
    pub effect_seed: i64,
    pub settings: serde_json::Value,
    pub version: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProject {
    pub owner_id: Uuid,
    pub name: String,
    pub aspect_ratio: String,
    pub fps: Option<i32>,
    pub language: Option<String>,
    pub effect_seed: Option<i64>,
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub aspect_ratio: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub status: Option<String>,
}
