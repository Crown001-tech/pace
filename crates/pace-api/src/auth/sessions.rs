use rand::RngCore;
use sha2::{Digest, Sha256};

/// Generates a cryptographically secure random session token (32 bytes, hex encoded).
pub fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hashes a raw session token with SHA-256 for secure database storage.
pub fn hash_session_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generates a cryptographically secure random CSRF token (32 bytes, hex encoded).
pub fn generate_csrf_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_token_and_hashing() {
        let token = generate_session_token();
        assert_eq!(token.len(), 64);

        let hash1 = hash_session_token(&token);
        let hash2 = hash_session_token(&token);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }
}
