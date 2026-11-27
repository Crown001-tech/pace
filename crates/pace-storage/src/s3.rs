use crate::{ObjectEntry, ObjectInfo, Storage, StorageError};
use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::Method;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

type HmacSha256 = Hmac<Sha256>;

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub path_style: bool,
}

#[derive(Debug, Clone)]
pub struct S3Storage {
    config: S3Config,
    client: reqwest::Client,
}

struct TargetUrlParts {
    host_header: String,
    canonical_uri: String,
    base_request_url: String,
}

impl S3Storage {
    pub fn new(config: S3Config) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { config, client }
    }

    fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    fn get_signing_key(&self, date_stamp: &str) -> Vec<u8> {
        let k_secret = format!("AWS4{}", self.config.secret_access_key);
        let k_date = Self::hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes());
        let k_region = Self::hmac_sha256(&k_date, self.config.region.as_bytes());
        let k_service = Self::hmac_sha256(&k_region, b"s3");
        Self::hmac_sha256(&k_service, b"aws4_request")
    }

    fn build_target(&self, key: &str) -> Result<TargetUrlParts, StorageError> {
        let endpoint_url = Url::parse(&self.config.endpoint)
            .map_err(|e| StorageError::Signing(e.to_string()))?;
        let host = endpoint_url
            .host_str()
            .ok_or_else(|| StorageError::Signing("Invalid endpoint host".to_string()))?;

        let raw_host_header = if let Some(port) = endpoint_url.port() {
            format!("{host}:{port}")
        } else {
            host.to_string()
        };

        let base_path = endpoint_url.path().trim_end_matches('/');
        let encoded_bucket = urlencoding::encode(&self.config.bucket);
        let clean_key = key.trim_start_matches('/');

        if self.config.path_style {
            let canonical_uri = if clean_key.is_empty() {
                format!("{base_path}/{encoded_bucket}")
            } else {
                let encoded_key = urlencoding::encode_uri_path(clean_key);
                format!("{base_path}/{encoded_bucket}/{encoded_key}")
            };
            let base_request_url = format!(
                "{}://{}{}",
                endpoint_url.scheme(),
                raw_host_header,
                canonical_uri
            );
            Ok(TargetUrlParts {
                host_header: raw_host_header,
                canonical_uri,
                base_request_url,
            })
        } else {
            let host_header = format!("{}.{}", self.config.bucket, raw_host_header);
            let canonical_uri = if clean_key.is_empty() {
                if base_path.is_empty() {
                    "/".to_string()
                } else {
                    format!("{base_path}/")
                }
            } else {
                let encoded_key = urlencoding::encode_uri_path(clean_key);
                format!("{base_path}/{encoded_key}")
            };
            let base_request_url = format!(
                "{}://{}{}",
                endpoint_url.scheme(),
                host_header,
                canonical_uri
            );
            Ok(TargetUrlParts {
                host_header,
                canonical_uri,
                base_request_url,
            })
        }
    }

    fn canonical_query(params: &[(&str, &str)]) -> String {
        let mut encoded: Vec<(String, String)> = params
            .iter()
            .map(|&(k, v)| (urlencoding::encode(k), urlencoding::encode(v)))
            .collect();
        encoded.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        encoded
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    fn generate_presigned_url(
        &self,
        method: &str,
        key: &str,
        ttl: Duration,
        signed_content_type: Option<&str>,
        extra_query: &[(&str, String)],
    ) -> Result<Url, StorageError> {
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();

        let target = self.build_target(key)?;

        let credential_scope = format!(
            "{}/{}/{}/s3/aws4_request",
            self.config.access_key_id, date_stamp, self.config.region
        );
        let expires = ttl.as_secs().to_string();

        let (canonical_headers, signed_headers) = match signed_content_type {
            Some(ct) if !ct.trim().is_empty() => (
                format!("content-type:{}\nhost:{}\n", ct.trim(), target.host_header),
                "content-type;host",
            ),
            _ => (format!("host:{}\n", target.host_header), "host"),
        };

        let mut query_params: Vec<(&str, &str)> = vec![
            ("X-Amz-Algorithm", "AWS4-HMAC-SHA256"),
            ("X-Amz-Credential", &credential_scope),
            ("X-Amz-Date", &amz_date),
            ("X-Amz-Expires", &expires),
            ("X-Amz-SignedHeaders", signed_headers),
        ];
        for (k, v) in extra_query {
            query_params.push((k, v.as_str()));
        }

        let canonical_query_string = Self::canonical_query(&query_params);

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\nUNSIGNED-PAYLOAD",
            method,
            target.canonical_uri,
            canonical_query_string,
            canonical_headers,
            signed_headers
        );

        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}/{}/s3/aws4_request\n{}",
            amz_date, date_stamp, self.config.region, canonical_request_hash
        );

        let signing_key = self.get_signing_key(&date_stamp);
        let signature = hex::encode(Self::hmac_sha256(&signing_key, string_to_sign.as_bytes()));

        let full_url_str = format!(
            "{}?{}&X-Amz-Signature={}",
            target.base_request_url, canonical_query_string, signature
        );

        Url::parse(&full_url_str).map_err(|e| StorageError::Signing(e.to_string()))
    }

    fn sign_headers(
        &self,
        method: &str,
        key: &str,
        query_params: &[(&str, &str)],
        content_type: Option<&str>,
        payload_hash: &str,
    ) -> Result<(Url, HeaderMap), StorageError> {
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();

        let target = self.build_target(key)?;
        let canonical_query_string = Self::canonical_query(query_params);

        let trimmed_ct = content_type.map(str::trim).filter(|s| !s.is_empty());
        let (canonical_headers, signed_headers) = match trimmed_ct {
            Some(ct) => (
                format!(
                    "content-type:{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
                    ct, target.host_header, payload_hash, amz_date
                ),
                "content-type;host;x-amz-content-sha256;x-amz-date",
            ),
            None => (
                format!(
                    "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
                    target.host_header, payload_hash, amz_date
                ),
                "host;x-amz-content-sha256;x-amz-date",
            ),
        };

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            target.canonical_uri,
            canonical_query_string,
            canonical_headers,
            signed_headers,
            payload_hash
        );

        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let scope = format!("{}/{}/s3/aws4_request", date_stamp, self.config.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date, scope, canonical_request_hash
        );

        let signing_key = self.get_signing_key(&date_stamp);
        let signature = hex::encode(Self::hmac_sha256(&signing_key, string_to_sign.as_bytes()));

        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.config.access_key_id, scope, signed_headers, signature
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-amz-date",
            HeaderValue::from_str(&amz_date).map_err(|e| StorageError::Signing(e.to_string()))?,
        );
        headers.insert(
            "x-amz-content-sha256",
            HeaderValue::from_str(payload_hash).map_err(|e| StorageError::Signing(e.to_string()))?,
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&authorization)
                .map_err(|e| StorageError::Signing(e.to_string()))?,
        );
        if let Some(ct) = trimmed_ct {
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_str(ct).map_err(|e| StorageError::Signing(e.to_string()))?,
            );
        }

        let full_url_str = if canonical_query_string.is_empty() {
            target.base_request_url
        } else {
            format!("{}?{}", target.base_request_url, canonical_query_string)
        };
        let url = Url::parse(&full_url_str).map_err(|e| StorageError::Signing(e.to_string()))?;

        Ok((url, headers))
    }

    async fn delete_object(&self, key: &str) -> Result<(), StorageError> {
        let (url, headers) = self.sign_headers("DELETE", key, &[], None, EMPTY_SHA256)?;
        let resp = self
            .client
            .request(Method::DELETE, url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 DELETE request failed: {e}")))?;

        let status = resp.status();
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            let text = resp.text().await.unwrap_or_default();
            Err(StorageError::Backend(format!(
                "S3 DELETE failed for '{key}' with {status}: {text}"
            )))
        }
    }
}

fn unescape_xml(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

fn extract_xml_tag_values(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after_open = &rest[start + open.len()..];
        if let Some(end) = after_open.find(&close) {
            values.push(unescape_xml(&after_open[..end]));
            rest = &after_open[end + close.len()..];
        } else {
            break;
        }
    }
    values
}

/// Parses every `<Contents>` block of an S3 `ListObjectsV2` XML response into `ObjectEntry` rows.
fn parse_list_contents(xml: &str) -> Vec<ObjectEntry> {
    let mut entries = Vec::new();
    for block in xml.split("<Contents>").skip(1) {
        let block = block.split("</Contents>").next().unwrap_or(block);
        let key = extract_xml_tag_values(block, "Key").into_iter().next();
        let size = extract_xml_tag_values(block, "Size")
            .into_iter()
            .next()
            .and_then(|s| s.parse::<u64>().ok());
        let modified = extract_xml_tag_values(block, "LastModified")
            .into_iter()
            .next()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        if let (Some(key), Some(size_bytes), Some(last_modified)) = (key, size, modified) {
            entries.push(ObjectEntry {
                key,
                size_bytes,
                last_modified,
            });
        }
    }
    entries
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        for &b in s.as_bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                result.push(b as char);
            } else {
                result.push_str(&format!("%{:02X}", b));
            }
        }
        result
    }

    pub fn encode_uri_path(path: &str) -> String {
        path.split('/')
            .map(encode)
            .collect::<Vec<_>>()
            .join("/")
    }
}

#[async_trait]
impl Storage for S3Storage {
    fn presign_put(
        &self,
        key: &str,
        content_type: &str,
        _size: u64,
        ttl: Duration,
    ) -> Result<Url, StorageError> {
        self.generate_presigned_url("PUT", key, ttl, Some(content_type), &[])
    }

    fn presign_get(
        &self,
        key: &str,
        ttl: Duration,
        download_name: Option<&str>,
    ) -> Result<Url, StorageError> {
        let extra = match download_name.map(str::trim).filter(|s| !s.is_empty()) {
            Some(name) => vec![(
                "response-content-disposition",
                format!("attachment; filename=\"{}\"", name.replace('"', "")),
            )],
            None => Vec::new(),
        };
        self.generate_presigned_url("GET", key, ttl, None, &extra)
    }

    async fn head(&self, key: &str) -> Result<Option<ObjectInfo>, StorageError> {
        let (url, headers) = self.sign_headers("HEAD", key, &[], None, EMPTY_SHA256)?;
        let resp = self
            .client
            .request(Method::HEAD, url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 HEAD request failed: {e}")))?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(StorageError::Backend(format!(
                "S3 HEAD failed for '{key}' with {status}"
            )));
        }

        let size_bytes = resp
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let content_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_matches('"').to_string());

        Ok(Some(ObjectInfo {
            key: key.to_string(),
            size_bytes,
            content_type,
            etag,
        }))
    }

    async fn download(&self, key: &str, dest: &Path) -> Result<(), StorageError> {
        let (url, headers) = self.sign_headers("GET", key, &[], None, EMPTY_SHA256)?;
        let mut resp = self
            .client
            .request(Method::GET, url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 GET request failed: {e}")))?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(StorageError::NotFound(key.to_string()));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(StorageError::Backend(format!(
                "S3 GET failed for '{key}' with {status}: {text}"
            )));
        }

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut file = tokio::fs::File::create(dest).await?;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 stream read failed: {e}")))?
        {
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        Ok(())
    }

    async fn upload(&self, src: &Path, key: &str, content_type: &str) -> Result<(), StorageError> {
        if !src.exists() {
            return Err(StorageError::NotFound(src.display().to_string()));
        }

        let meta = tokio::fs::metadata(src).await?;
        let file_len = meta.len();

        // Stream hash in 64 KB chunks so large audio/video files never buffer fully in RAM.
        let mut hash_file = tokio::fs::File::open(src).await?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = hash_file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let payload_hash = hex::encode(hasher.finalize());

        let (url, headers) =
            self.sign_headers("PUT", key, &[], Some(content_type), &payload_hash)?;

        let upload_file = tokio::fs::File::open(src).await?;
        let resp = self
            .client
            .request(Method::PUT, url)
            .headers(headers)
            .header(CONTENT_LENGTH, file_len)
            .body(reqwest::Body::from(upload_file))
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("S3 PUT request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(StorageError::Backend(format!(
                "S3 PUT failed for '{key}' with {status}: {text}"
            )));
        }

        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<u64, StorageError> {
        let clean_prefix = prefix.trim_start_matches('/');
        let mut continuation: Option<String> = None;
        let mut deleted_count = 0u64;

        loop {
            let mut query: Vec<(&str, &str)> = vec![("list-type", "2"), ("prefix", clean_prefix)];
            if let Some(ref token) = continuation {
                query.push(("continuation-token", token.as_str()));
            }

            let (url, headers) = self.sign_headers("GET", "", &query, None, EMPTY_SHA256)?;
            let resp = self
                .client
                .request(Method::GET, url)
                .headers(headers)
                .send()
                .await
                .map_err(|e| StorageError::Backend(format!("S3 ListObjectsV2 failed: {e}")))?;

            let status = resp.status();
            let xml = resp.text().await.map_err(|e| {
                StorageError::Backend(format!("Failed reading S3 list response: {e}"))
            })?;
            if !status.is_success() {
                return Err(StorageError::Backend(format!(
                    "S3 ListObjectsV2 failed with {status}: {xml}"
                )));
            }

            for key in extract_xml_tag_values(&xml, "Key") {
                self.delete_object(&key).await?;
                deleted_count += 1;
            }

            let is_truncated = extract_xml_tag_values(&xml, "IsTruncated")
                .first()
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

            if is_truncated {
                continuation = extract_xml_tag_values(&xml, "NextContinuationToken").into_iter().next();
                if continuation.is_none() {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(deleted_count)
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<ObjectEntry>, StorageError> {
        let clean_prefix = prefix.trim_start_matches('/');
        let mut continuation: Option<String> = None;
        let mut out = Vec::new();

        loop {
            let mut query: Vec<(&str, &str)> = vec![("list-type", "2"), ("prefix", clean_prefix)];
            if let Some(ref token) = continuation {
                query.push(("continuation-token", token.as_str()));
            }

            let (url, headers) = self.sign_headers("GET", "", &query, None, EMPTY_SHA256)?;
            let resp = self
                .client
                .request(Method::GET, url)
                .headers(headers)
                .send()
                .await
                .map_err(|e| StorageError::Backend(format!("S3 ListObjectsV2 failed: {e}")))?;

            let status = resp.status();
            let xml = resp.text().await.map_err(|e| {
                StorageError::Backend(format!("Failed reading S3 list response: {e}"))
            })?;
            if !status.is_success() {
                return Err(StorageError::Backend(format!(
                    "S3 ListObjectsV2 failed with {status}: {xml}"
                )));
            }

            out.extend(parse_list_contents(&xml));

            let is_truncated = extract_xml_tag_values(&xml, "IsTruncated")
                .first()
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            if !is_truncated {
                break;
            }
            continuation = extract_xml_tag_values(&xml, "NextContinuationToken")
                .into_iter()
                .next();
            if continuation.is_none() {
                break;
            }
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_list_contents_reads_key_size_and_last_modified() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult><Name>pace</Name><Prefix>u/</Prefix><KeyCount>2</KeyCount><IsTruncated>false</IsTruncated>
<Contents><Key>u/1/p/1/media/a/original.png</Key><LastModified>2026-11-20T10:00:00.000Z</LastModified><ETag>&quot;abc&quot;</ETag><Size>1234</Size><StorageClass>STANDARD</StorageClass></Contents>
<Contents><Key>u/1/p/1/media/a/caf&amp;e.png</Key><LastModified>2026-11-21T11:30:15.250Z</LastModified><ETag>&quot;def&quot;</ETag><Size>7</Size><StorageClass>STANDARD</StorageClass></Contents>
</ListBucketResult>"#;
        let entries = parse_list_contents(xml);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "u/1/p/1/media/a/original.png");
        assert_eq!(entries[0].size_bytes, 1234);
        assert_eq!(entries[0].last_modified.to_rfc3339(), "2026-11-20T10:00:00+00:00");
        assert_eq!(entries[1].key, "u/1/p/1/media/a/caf&e.png");
        assert_eq!(entries[1].size_bytes, 7);
    }

    #[test]
    fn test_s3_presigned_put_url_structure() {
        let config = S3Config {
            endpoint: "http://localhost:9000".to_string(),
            bucket: "pace".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: "pace".to_string(),
            secret_access_key: "pace-secret".to_string(),
            path_style: true,
        };

        let storage = S3Storage::new(config);
        let url = storage
            .presign_put("media/sample.jpg", "image/jpeg", 1024, Duration::from_secs(3600))
            .unwrap();

        let s = url.as_str();
        assert!(s.starts_with("http://localhost:9000/pace/media/sample.jpg?"));
        assert!(s.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(s.contains("X-Amz-Credential=pace"));
        assert!(s.contains("X-Amz-Expires=3600"));
        assert!(s.contains("X-Amz-Signature="));
    }

    #[test]
    fn test_s3_presigned_url_encodes_utf8_and_signed_content_type() {
        let config = S3Config {
            endpoint: "http://localhost:9000".to_string(),
            bucket: "pace".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: "pace".to_string(),
            secret_access_key: "pace-secret".to_string(),
            path_style: true,
        };

        let storage = S3Storage::new(config);
        let url = storage
            .presign_put("u/1/café photo.png", "image/png", 512, Duration::from_secs(300))
            .unwrap();
        let s = url.as_str();
        assert!(s.contains("caf%C3%A9%20photo.png"), "Expected UTF-8 percent-encoded key, got: {s}");
        assert!(
            s.contains("X-Amz-SignedHeaders=content-type%3Bhost"),
            "Expected signed content-type header, got: {s}"
        );

        let get_url = storage
            .presign_get("u/1/café photo.png", Duration::from_secs(300), Some("my file.png"))
            .unwrap();
        assert!(
            get_url.as_str().contains("response-content-disposition="),
            "Expected content-disposition on download presign, got: {}",
            get_url.as_str()
        );
    }

    #[tokio::test]
    async fn test_s3_minio_live_lifecycle_when_available() {
        let client = reqwest::Client::new();
        let health = client
            .get("http://localhost:9000/minio/health/live")
            .timeout(Duration::from_secs(2))
            .send()
            .await;
        if health.map(|r| !r.status().is_success()).unwrap_or(true) {
            eprintln!("Skipping MinIO live test (localhost:9000 not reachable)");
            return;
        }

        let storage = S3Storage::new(S3Config {
            endpoint: "http://localhost:9000".to_string(),
            bucket: "pace".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: "pace".to_string(),
            secret_access_key: "pace-secret".to_string(),
            path_style: true,
        });

        let prefix = format!("test/s3-lifecycle-{}/", uuid::Uuid::new_v4());
        let key = format!("{}café sample.txt", prefix);
        let payload = b"hello minio lifecycle".to_vec();

        // 1. Presigned PUT
        let put_url = storage
            .presign_put(&key, "text/plain", payload.len() as u64, Duration::from_secs(300))
            .unwrap();
        let put_resp = client
            .put(put_url)
            .header("Content-Type", "text/plain")
            .body(payload.clone())
            .send()
            .await
            .unwrap();
        assert!(put_resp.status().is_success(), "Presigned PUT failed: {}", put_resp.status());

        // 2. HEAD
        let info = storage
            .head(&key)
            .await
            .unwrap()
            .expect("Uploaded object should exist via HEAD");
        assert_eq!(info.size_bytes, payload.len() as u64);

        // 3. Download
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("downloaded.txt");
        storage.download(&key, &dest).await.unwrap();
        assert_eq!(tokio::fs::read(&dest).await.unwrap(), payload);

        // 4. Direct S3Storage::upload
        let src = tmp.path().join("direct.txt");
        let direct_key = format!("{}direct upload.txt", prefix);
        tokio::fs::write(&src, b"direct upload body").await.unwrap();
        storage.upload(&src, &direct_key, "text/plain").await.unwrap();
        let direct_info = storage.head(&direct_key).await.unwrap().expect("Direct upload missing");
        assert_eq!(direct_info.size_bytes, 18);

        // 5. Delete prefix
        let deleted = storage.delete_prefix(&prefix).await.unwrap();
        assert_eq!(deleted, 2);
        assert!(storage.head(&key).await.unwrap().is_none(), "Deleted object should return None");
        assert!(storage.head(&direct_key).await.unwrap().is_none(), "Deleted direct object should return None");
    }
}
