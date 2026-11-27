use crate::{ObjectEntry, ObjectInfo, Storage, StorageError};
use async_trait::async_trait;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use url::Url;

/// Local folder implementation of `Storage` for tests and desktop app mode.
#[derive(Debug, Clone)]
pub struct LocalStorage {
    base_dir: PathBuf,
    base_url: String,
}

impl LocalStorage {
    pub fn new(base_dir: impl Into<PathBuf>, base_url: impl Into<String>) -> Self {
        Self {
            base_dir: base_dir.into(),
            base_url: base_url.into(),
        }
    }

    fn key_to_path(&self, key: &str) -> Result<PathBuf, StorageError> {
        let safe_key = key.trim_start_matches('/');
        let mut resolved = self.base_dir.clone();
        for component in Path::new(safe_key).components() {
            match component {
                Component::Normal(part) => resolved.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(StorageError::Signing(format!(
                        "Invalid storage key path traversal attempt: {key}"
                    )));
                }
            }
        }
        Ok(resolved)
    }
}

#[async_trait]
impl Storage for LocalStorage {
    fn presign_put(
        &self,
        key: &str,
        _content_type: &str,
        _size: u64,
        _ttl: Duration,
    ) -> Result<Url, StorageError> {
        let url_str = format!("{}/put/{}", self.base_url.trim_end_matches('/'), key);
        Url::parse(&url_str).map_err(|e| StorageError::Signing(e.to_string()))
    }

    fn presign_get(
        &self,
        key: &str,
        _ttl: Duration,
        _download_name: Option<&str>,
    ) -> Result<Url, StorageError> {
        let url_str = format!("{}/get/{}", self.base_url.trim_end_matches('/'), key);
        Url::parse(&url_str).map_err(|e| StorageError::Signing(e.to_string()))
    }

    async fn head(&self, key: &str) -> Result<Option<ObjectInfo>, StorageError> {
        let path = self.key_to_path(key)?;
        if !path.exists() {
            return Ok(None);
        }

        let meta = tokio::fs::metadata(&path).await?;
        Ok(Some(ObjectInfo {
            key: key.to_string(),
            size_bytes: meta.len(),
            content_type: "application/octet-stream".to_string(),
            etag: None,
        }))
    }

    async fn download(&self, key: &str, dest: &Path) -> Result<(), StorageError> {
        let src = self.key_to_path(key)?;
        if !src.exists() {
            return Err(StorageError::NotFound(key.to_string()));
        }

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&src, dest).await?;
        Ok(())
    }

    async fn upload(&self, src: &Path, key: &str, _content_type: &str) -> Result<(), StorageError> {
        if !src.exists() {
            return Err(StorageError::NotFound(src.display().to_string()));
        }

        let dest = self.key_to_path(key)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(src, &dest).await?;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<u64, StorageError> {
        let target_dir = self.key_to_path(prefix)?;
        if !target_dir.exists() {
            return Ok(0);
        }

        let mut count = 0u64;
        if target_dir.is_dir() {
            let mut entries = tokio::fs::read_dir(&target_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_file() {
                    tokio::fs::remove_file(&path).await?;
                    count += 1;
                }
            }
            let _ = tokio::fs::remove_dir_all(&target_dir).await;
        } else {
            tokio::fs::remove_file(&target_dir).await?;
            count += 1;
        }

        Ok(count)
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<ObjectEntry>, StorageError> {
        let clean_prefix = prefix.trim_start_matches('/');
        // Walk from the deepest whole directory contained in the prefix, then filter by string prefix
        // (matching S3 ListObjectsV2 semantics where a prefix is a plain string prefix).
        let dir_part = match clean_prefix.rfind('/') {
            Some(idx) => &clean_prefix[..=idx],
            None => "",
        };
        let start_dir = self.key_to_path(dir_part)?;
        if !start_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        let mut stack = vec![start_dir];
        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let meta = entry.metadata().await?;
                if meta.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&self.base_dir) else {
                    continue;
                };
                let key = rel
                    .components()
                    .filter_map(|c| match c {
                        Component::Normal(p) => p.to_str().map(str::to_string),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                if !key.starts_with(clean_prefix) {
                    continue;
                }
                let modified = meta.modified().map_err(StorageError::Io)?;
                out.push(ObjectEntry {
                    key,
                    size_bytes: meta.len(),
                    last_modified: chrono::DateTime::<chrono::Utc>::from(modified),
                });
            }
        }
        out.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_local_storage_list_prefix_is_recursive_and_string_prefixed() {
        let tmp = tempdir().unwrap();
        let storage = LocalStorage::new(tmp.path(), "http://localhost:9000/pace");
        let src = tmp.path().join("src.bin");
        tokio::fs::write(&src, b"abc").await.unwrap();

        for key in [
            "u/1/p/1/media/a/original.png",
            "u/1/p/1/media/a/thumb.webp",
            "u/1/p/2/audio/b/audio.opus",
            "u/2/p/9/media/c/original.png",
        ] {
            storage.upload(&src, key, "x/y").await.unwrap();
        }

        let all_u1 = storage.list_prefix("u/1/").await.unwrap();
        let keys: Vec<&str> = all_u1.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "u/1/p/1/media/a/original.png",
                "u/1/p/1/media/a/thumb.webp",
                "u/1/p/2/audio/b/audio.opus",
            ]
        );
        assert!(all_u1.iter().all(|e| e.size_bytes == 3));

        let partial = storage.list_prefix("u/1/p/1/media/a/orig").await.unwrap();
        assert_eq!(partial.len(), 1);
        assert_eq!(partial[0].key, "u/1/p/1/media/a/original.png");

        assert!(storage.list_prefix("u/404/").await.unwrap().is_empty());
        assert_eq!(storage.list_prefix("u/").await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn test_local_storage_lifecycle() {
        let tmp = tempdir().unwrap();
        let storage = LocalStorage::new(tmp.path(), "http://localhost:9000/pace");

        let key = "test/lifecycle.txt";
        let upload_url = storage.presign_put(key, "text/plain", 12, Duration::from_secs(3600)).unwrap();
        assert!(upload_url.as_str().contains("/put/test/lifecycle.txt"));

        // Direct upload & head check
        let sample_file = tmp.path().join("source.txt");
        tokio::fs::write(&sample_file, b"hello world").await.unwrap();

        storage.upload(&sample_file, key, "text/plain").await.unwrap();
        let head = storage.head(key).await.unwrap().expect("object exists");
        assert_eq!(head.size_bytes, 11);

        // Download check
        let dest = tmp.path().join("downloaded.txt");
        storage.download(key, &dest).await.unwrap();
        let content = tokio::fs::read_to_string(&dest).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_local_storage_rejects_path_traversal() {
        let tmp = tempdir().unwrap();
        let storage = LocalStorage::new(tmp.path(), "http://localhost:9000/pace");

        let res = storage.head("../outside.txt").await;
        assert!(res.is_err(), "Parent directory traversal must be rejected");

        let sample = tmp.path().join("sample.txt");
        tokio::fs::write(&sample, b"x").await.unwrap();
        let upload_res = storage.upload(&sample, "nested/../../escape.txt", "text/plain").await;
        assert!(upload_res.is_err(), "Nested parent traversal must be rejected");
    }
}

