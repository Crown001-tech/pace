pub mod keys;
pub mod local;
pub mod s3;

use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use url::Url;

pub use keys::StorageKeys;
pub use local::LocalStorage;
pub use s3::{S3Config, S3Storage};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Object not found: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Storage signing error: {0}")]
    Signing(String),

    #[error("Storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    pub key: String,
    pub size_bytes: u64,
    pub content_type: String,
    pub etag: Option<String>,
}

/// One row returned by `Storage::list_prefix` (used by the janitor orphan sweep, `M10-3`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectEntry {
    pub key: String,
    pub size_bytes: u64,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait Storage: Send + Sync {
    /// Generate a pre-signed PUT URL for direct browser upload (valid for ttl).
    fn presign_put(
        &self,
        key: &str,
        content_type: &str,
        size: u64,
        ttl: Duration,
    ) -> Result<Url, StorageError>;

    /// Generate a pre-signed GET URL for secure downloading or previewing.
    fn presign_get(
        &self,
        key: &str,
        ttl: Duration,
        download_name: Option<&str>,
    ) -> Result<Url, StorageError>;

    /// Get object metadata if it exists.
    async fn head(&self, key: &str) -> Result<Option<ObjectInfo>, StorageError>;

    /// Download an object from storage to a local file.
    async fn download(&self, key: &str, dest: &Path) -> Result<(), StorageError>;

    /// Upload a local file into storage.
    async fn upload(&self, src: &Path, key: &str, content_type: &str) -> Result<(), StorageError>;

    /// Delete all objects matching a given prefix (e.g. when deleting a project).
    async fn delete_prefix(&self, prefix: &str) -> Result<u64, StorageError>;

    /// List every object whose key starts with `prefix` (paginated internally).
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<ObjectEntry>, StorageError>;
}

