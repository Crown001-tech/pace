use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

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
    if provided.len() != expected.len() {
        return false;
    }
    provided.as_bytes().ct_eq(expected.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csrf_derivation_and_validation() {
        let session = "raw_session_token_12345";
        let secret = "secret_server_key_67890";

        let csrf1 = derive_csrf_token(session, secret);
        let csrf2 = derive_csrf_token(session, secret);
        assert_eq!(csrf1, csrf2);

        assert!(validate_csrf_token(&csrf1, &csrf2));
        assert!(!validate_csrf_token("invalid_token", &csrf1));
    }
}
