use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SceneMedia {
    pub id: Uuid,
    pub section_id: Uuid,
    pub media_id: Uuid,
    pub position: i32,
    pub fit_mode: String,
    pub focal_x: f32,
    pub focal_y: f32,
    pub effect: serde_json::Value,
    pub effect_overridden: bool,
    pub trim: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignSceneMedia {
    pub section_id: Uuid,
    pub media_id: Uuid,
    pub fit_mode: Option<String>,
    pub focal_x: Option<f32>,
    pub focal_y: Option<f32>,
}

