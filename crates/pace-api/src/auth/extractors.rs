use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, Method},
};
use pace_db::{find_active_session_by_token_hash, find_user_by_id, touch_session, User};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{
        csrf::{derive_csrf_token, validate_csrf_token},
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
        // 1. Extract session cookie
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
                if name == "pace_session" {
                    Some(val.to_string())
                } else {
                    None
                }
            })
            .ok_or(ApiError::Unauthorized)?;

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

        // 5. Enforce CSRF protection for mutating methods (POST, PUT, PATCH, DELETE)
        let is_mutating = matches!(
            parts.method,
            Method::POST | Method::PUT | Method::PATCH | Method::DELETE
        );

        if is_mutating {
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
        // If must_change_password is true, only password change and logout endpoints are permitted
        if user.must_change_password {
            let path = parts.uri.path();
            let is_allowed_during_pw_change =
                path.ends_with("/password") || path.ends_with("/logout");
            if !is_allowed_during_pw_change {
                return Err(ApiError::PasswordChangeRequired);
            }
        }

        // 7. Slide session (update last_seen_at)
        let db = state.db.clone();
        let sid = session.id;
        tokio::spawn(async move {
            let _ = touch_session(&db, sid).await;
        });

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
