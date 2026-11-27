use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("Database error: {0}")]
    Db(#[from] pace_db::DbError),

    #[error("Job queue error: {0}")]
    Job(#[from] pace_jobs::JobError),

    #[error("Storage error: {0}")]
    Storage(#[from] pace_storage::StorageError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image decode error: {0}")]
    ImageDecode(String),

    #[error("SIMD resize error: {0}")]
    Resize(String),

    #[error("FFmpeg error ({code}): {message}")]
    Ffmpeg { code: String, message: String },

    #[error("Export validation failed ({code}): {message}")]
    Validation { code: String, message: String },

    #[error("Invalid render payload: {0}")]
    InvalidPayload(String),
}
