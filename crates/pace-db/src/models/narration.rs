use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SilenceGap {
    pub start_ms: i32,
    pub end_ms: i32,
    pub duration_ms: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptWord {
    pub word: String,
    pub start_ms: i32,
    pub end_ms: i32,
    pub probability: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Narration {
    pub project_id: Uuid,
    pub media_id: Uuid,
    pub duration_ms: i32,
    pub loudness_lufs: Option<f32>,
    pub true_peak_db: Option<f32>,
    pub peaks_key: Option<String>,
    pub silences: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Transcript {
    pub id: Uuid,
    pub project_id: Uuid,
    pub narration_media_id: Uuid,
    pub engine: String,
    pub model: String,
    pub language: String,
    pub words: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Boundary {
    pub id: Uuid,
    pub project_id: Uuid,
    pub index: i32,
    pub time_ms: i32,
    pub source: String,
    pub confidence: Option<f32>,
    pub flags: Vec<String>,
    pub transition: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SubtitleCue {
    pub id: Uuid,
    pub project_id: Uuid,
    pub position: i32,
    pub start_ms: i32,
    pub end_ms: i32,
    pub text: String,
    pub edited: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
