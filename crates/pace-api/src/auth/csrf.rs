use axum::http::{header, HeaderMap};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::error::ApiError;

/// Cryptographically derives a CSRF token for a given session token and server secret.
pub fn derive_csrf_token(session_token: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session_token.as_bytes());
    hasher.update(b":csrf:");
    hasher.update(secret.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Validates an incoming CSRF token against the expected session CSRF token
/// using constant-time equality to prevent timing attacks.
pub fn validate_csrf_token(provided: &str, expected: &str) -> bool {
    if provided.len() != expected.len() || provided.is_empty() {
        return false;
    }
    provided.as_bytes().ct_eq(expected.as_bytes()).into()
}

fn parse_allowed_origins(allowed_origin: &str) -> Vec<&str> {
    allowed_origin
        .split(',')
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Enforces same-origin validation on browser mutating requests (TDD Section 17).
/// Supports single or comma-separated `allowed_origin` allowlists.
/// If `Origin` is present, it must match an entry in `allowed_origin`.
/// Otherwise if `Referer` is present, its scheme+host+port prefix must match an entry cleanly.
pub fn validate_request_origin(headers: &HeaderMap, allowed_origin: &str) -> Result<(), ApiError> {
    let allowlist = parse_allowed_origins(allowed_origin);

    if let Some(origin_val) = headers.get(header::ORIGIN) {
        let origin = origin_val
            .to_str()
            .map_err(|_| ApiError::Forbidden("Invalid Origin header".into()))?
            .trim()
            .trim_end_matches('/');

        if !allowlist.iter().any(|allowed| origin.eq_ignore_ascii_case(allowed)) {
            return Err(ApiError::Forbidden("Request Origin is not allowed".into()));
        }
        return Ok(());
    }

    if let Some(referer_val) = headers.get(header::REFERER) {
        let referer = referer_val
            .to_str()
            .map_err(|_| ApiError::Forbidden("Invalid Referer header".into()))?
            .trim();

        let matches_any = allowlist.iter().any(|expected| {
            let matches_prefix = referer
                .get(..expected.len())
                .map(|prefix| prefix.eq_ignore_ascii_case(expected))
                .unwrap_or(false);
            let boundary_ok = matches!(
                referer.as_bytes().get(expected.len()),
                None | Some(b'/') | Some(b'?')
            );
            matches_prefix && boundary_ok
        });

        if !matches_any {
            return Err(ApiError::Forbidden("Request Referer is not allowed".into()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_csrf_derivation_and_validation() {
        let session = "raw_session_token_12345";
        let secret = "secret_server_key_67890";

        let csrf1 = derive_csrf_token(session, secret);
        let csrf2 = derive_csrf_token(session, secret);
        assert_eq!(csrf1, csrf2);

        assert!(validate_csrf_token(&csrf1, &csrf2));
        assert!(!validate_csrf_token("invalid_token", &csrf1));
        assert!(!validate_csrf_token("", &csrf1));
    }

    #[test]
    fn test_origin_and_referer_validation_single_and_multi() {
        let allowlist = "http://localhost:3000, http://127.0.0.1:3000";

        let mut ok_headers = HeaderMap::new();
        ok_headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:3000"),
        );
        assert!(validate_request_origin(&ok_headers, allowlist).is_ok());

        let mut evil_headers = HeaderMap::new();
        evil_headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        assert!(validate_request_origin(&evil_headers, allowlist).is_err());

        let mut evil_ref = HeaderMap::new();
        evil_ref.insert(
            header::REFERER,
            HeaderValue::from_static("http://localhost:3000.attacker.test/steal"),
        );
        assert!(validate_request_origin(&evil_ref, allowlist).is_err());

        let mut ok_ref = HeaderMap::new();
        ok_ref.insert(
            header::REFERER,
            HeaderValue::from_static("http://localhost:3000/projects"),
        );
        assert!(validate_request_origin(&ok_ref, allowlist).is_ok());
    }
}
