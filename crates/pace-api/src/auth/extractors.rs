use axum::{
    extract::{FromRequestParts, OriginalUri},
    http::{header, request::Parts, Method},
};
use pace_db::{
    find_active_extension_token_by_hash, find_active_session_by_token_hash, find_user_by_id,
    touch_session, User,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{
        csrf::{derive_csrf_token, validate_csrf_token, validate_request_origin},
        sessions::hash_session_token,
    },
    error::ApiError,
};

/// Authenticated user context extracted from the `pace_session` cookie and database.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user: User,
    pub session_id: Uuid,
    pub session_token: String,
    pub csrf_token: String,
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Extract and validate format of session cookie before querying DB
        let cookie_header = parts
            .headers
            .get(header::COOKIE)
            .and_then(|h| h.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;

        let session_token = cookie_header
            .split(';')
            .find_map(|pair| {
                let mut p = pair.trim().splitn(2, '=');
                let name = p.next()?.trim();
                let val = p.next()?.trim();
                if name == "pace_session" && !val.is_empty() {
                    Some(val.to_string())
                } else {
                    None
                }
            })
            .ok_or(ApiError::Unauthorized)?;

        if session_token.len() != 64 || !session_token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ApiError::Unauthorized);
        }

        // 2. Hash token and query active session in database
        let token_hash = hash_session_token(&session_token);
        let session = find_active_session_by_token_hash(&state.db, &token_hash)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::Unauthorized)?;

        // 3. Look up user and check active status
        let user = find_user_by_id(&state.db, session.user_id)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::Unauthorized)?;

        if !user.is_active {
            return Err(ApiError::Forbidden("Account is deactivated".into()));
        }

        // 4. Derive expected CSRF token
        let expected_csrf = derive_csrf_token(&session_token, &state.config.session_secret);

        // 5. Enforce Origin and CSRF protection for mutating methods (POST, PUT, PATCH, DELETE)
        let is_mutating = matches!(
            parts.method,
            Method::POST | Method::PUT | Method::PATCH | Method::DELETE
        );

        if is_mutating {
            validate_request_origin(&parts.headers, &state.config.cors_origin)?;

            let provided_csrf = parts
                .headers
                .get("x-pace-csrf")
                .and_then(|h| h.to_str().ok())
                .unwrap_or_default();

            if !validate_csrf_token(provided_csrf, &expected_csrf) {
                return Err(ApiError::InvalidCsrfToken);
            }
        }

        // 6. Enforce temporary password change rule (AUTH-03)
        // Use OriginalUri because Axum `.nest(...)` strips prefixes from `parts.uri.path()`.
        if user.must_change_password {
            let full_path = parts
                .extensions
                .get::<OriginalUri>()
                .map(|uri| uri.0.path())
                .unwrap_or_else(|| parts.uri.path());
            let is_allowed_during_pw_change =
                matches!(full_path, "/api/v1/auth/password" | "/api/v1/auth/logout");
            if !is_allowed_during_pw_change {
                return Err(ApiError::PasswordChangeRequired);
            }
        }

        // 7. Slide 30-day session expiry synchronously so active sessions extend deterministically
        if let Err(err) = touch_session(&state.db, session.id).await {
            tracing::warn!(error = %err, session_id = %session.id, "Failed to update sliding session expiry");
        }

        Ok(AuthContext {
            user,
            session_id: session.id,
            session_token,
            csrf_token: expected_csrf,
        })
    }
}

/// Admin authorization extractor. Ensures caller has an active session with role == "admin".
pub struct RequireAdmin(pub AuthContext);

impl FromRequestParts<AppState> for RequireAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthContext::from_request_parts(parts, state).await?;
        if auth.user.role != "admin" {
            return Err(ApiError::Forbidden("Admin privileges required".into()));
        }
        Ok(RequireAdmin(auth))
    }
}

/// Scoped authorization extractor for the PACE Chrome Extension (`EXT-01..05`).
/// Accepts only `Authorization: Bearer pace_ext_<hex>` tokens and cannot be used
/// on non-extension routes (which require `AuthContext` cookie + CSRF).
#[derive(Debug, Clone)]
pub struct ExtensionAuthContext {
    pub user: User,
    pub token_id: Uuid,
    pub last_project_id: Option<Uuid>,
}

impl FromRequestParts<AppState> for ExtensionAuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;

        let raw_token = auth_header
            .strip_prefix("Bearer ")
            .or_else(|| auth_header.strip_prefix("bearer "))
            .map(str::trim)
            .ok_or(ApiError::Unauthorized)?;

        let hex_part = raw_token
            .strip_prefix("pace_ext_")
            .ok_or(ApiError::Unauthorized)?;

        if hex_part.len() != 64 || !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ApiError::Unauthorized);
        }

        let token_hash = hash_session_token(raw_token);
        let ext_token = find_active_extension_token_by_hash(&state.db, &token_hash)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::Unauthorized)?;

        let user = find_user_by_id(&state.db, ext_token.user_id)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::Unauthorized)?;

        if !user.is_active {
            return Err(ApiError::Forbidden("Account is deactivated".into()));
        }
        if user.must_change_password {
            return Err(ApiError::PasswordChangeRequired);
        }

        Ok(ExtensionAuthContext {
            user,
            token_id: ext_token.id,
            last_project_id: ext_token.last_project_id,
        })
    }
}
