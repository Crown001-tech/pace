use chrono::{DateTime, Utc};
use pace_engine::stickers::StickerPlacementSpec;
use pace_engine::subtitles::{MusicSettingsSpec, SubtitleCueSpec, SubtitleStyleSpec};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobQueue {
    Audio,
    Render,
}

impl JobQueue {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Render => "render",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Payload stored inside `jobs.payload` for `audio_process` and `realign` jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioJobPayload {
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub media_id: Uuid,
    pub storage_key: String,
    pub opus_key: String,
    pub asr_key: String,
    pub peaks_key: String,
    pub mime: String,
    pub original_filename: String,
    #[serde(default)]
    pub keep_manual_boundaries: bool,
    #[serde(default)]
    pub force_local_transcription: bool,
}

/// Asset download coordinate for one scene inside a frozen `RenderJobPayload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderSceneAsset {
    pub scene_index: usize,
    pub media_id: Uuid,
    pub storage_key: String,
}

/// Payload stored inside `jobs.payload` for `queue = 'render'` (`video_export` jobs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderJobPayload {
    pub export_id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub project_name: String,
    pub aspect_ratio: String,
    pub resolution: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub output_key: String,
    pub audio_key: Option<String>,
    #[serde(default)]
    pub music_key: Option<String>,
    #[serde(default)]
    pub music_settings: Option<MusicSettingsSpec>,
    #[serde(default)]
    pub subtitles: Vec<SubtitleCueSpec>,
    #[serde(default)]
    pub subtitle_style: SubtitleStyleSpec,
    #[serde(default)]
    pub stickers: Vec<StickerPlacementSpec>,
    pub spec_json: serde_json::Value,
    pub scene_assets: Vec<RenderSceneAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct JobRecord {
    pub id: Uuid,
    pub queue: String,
    #[sqlx(rename = "type")]
    pub job_type: String,
    pub project_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub status: String,
    pub priority: i32,
    pub payload: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub progress: i32,
    pub stage: Option<String>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub locked_by: Option<String>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub run_after: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EnqueueJob {
    pub queue: JobQueue,
    pub job_type: String,
    pub project_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub priority: i32,
    pub payload: serde_json::Value,
    pub max_attempts: i32,
}
