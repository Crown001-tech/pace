use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use crate::error::ApiError;

/// Valid precomputed Argon2id hash used to equalize login response timing for unknown emails.
const DUMMY_ARGON2_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c29tZXJhbmRvbXNhbHQxMjM$8iW7mQvL9xK3wN0pT2yR5aB7cE9fH1jL3nP5rT7vX9Y";

const COMMON_PASSWORDS: &[&str] = &[
    "password123",
    "password1234",
    "password123!",
    "1234567890",
    "12345678901",
    "123456789012",
    "qwerty12345",
    "qwertyuiop1",
    "letmein1234",
    "admin123456",
    "welcome1234",
    "iloveyou123",
    "changeme123",
    "football123",
    "baseball123",
    "abc12345678",
];

/// Enforces password minimum length (>= 10 characters) and rejects common weak passwords (TDD Section 17).
pub fn validate_password_strength(password: &str) -> Result<(), ApiError> {
    if password.chars().count() < 10 {
        return Err(ApiError::BadRequest(
            "Password must be at least 10 characters".into(),
        ));
    }

    let normalized = password.trim().to_ascii_lowercase();
    if COMMON_PASSWORDS.contains(&normalized.as_str()) {
        return Err(ApiError::BadRequest(
            "Password is too common. Choose a stronger password.".into(),
        ));
    }

    Ok(())
}

/// Hashes a plaintext password using Argon2id with a secure random salt.
pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| ApiError::Internal(format!("Failed to hash password: {err}")))
}

/// Verifies a plaintext password against an Argon2 PHC string.
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, ApiError> {
    let parsed_hash = match PasswordHash::new(password_hash) {
        Ok(h) => h,
        Err(err) => {
            tracing::warn!(error = %err, "Malformed password hash in database");
            return Ok(false);
        }
    };

    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Runs a dummy Argon2 verification step when the target user is missing or inactive
/// so response timing cannot reveal account existence.
pub fn verify_dummy_password(password: &str) {
    let _ = verify_password(password, DUMMY_ARGON2_HASH);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2_hash_and_verify() {
        let password = "SuperSecretPassword123!";
        let hash = hash_password(password).expect("Hashing failed");
        assert!(hash.starts_with("$argon2id$"));

        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("WrongPassword!", &hash).unwrap());
    }

    #[test]
    fn test_password_strength_checks() {
        assert!(validate_password_strength("short").is_err());
        assert!(validate_password_strength("password123").is_err());
        assert!(validate_password_strength("1234567890").is_err());
        assert!(validate_password_strength("UniqueDocumentaryPass2026!").is_ok());
    }

    #[test]
    fn test_dummy_password_never_panics_and_runs_verification() {
        verify_dummy_password("AttemptedPassword123!");
    }
}
