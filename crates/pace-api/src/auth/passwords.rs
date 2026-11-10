use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use crate::error::ApiError;

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
    Ok(argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok())
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
}
