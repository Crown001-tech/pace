use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use pace_db::{
    check_lockout, create_session, delete_session, find_user_by_email, record_login_attempt,
    update_user_password, CreateSession, DbError,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{
        csrf::derive_csrf_token,
        extractors::AuthContext,
        passwords::{hash_password, verify_password},
        sessions::{generate_session_token, hash_session_token},
    },
    error::ApiError,
};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub must_change_password: bool,
    pub storage_quota_bytes: i64,
    pub storage_used_bytes: i64,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: UserProfile,
    pub csrf_token: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub status: &'static str,
    pub message: &'static str,
}

/// POST /api/v1/auth/login
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let email = payload.email.trim();

    // 1. Check lockout (AUTH-01: 10 failures in 15 mins locks account)
    if let Err(DbError::AccountLocked) = check_lockout(&state.db, email).await {
        return Err(ApiError::AccountLocked);
    }

    // 2. Lookup user and verify password
    let user_opt = find_user_by_email(&state.db, email).await?;
    let is_valid = match &user_opt {
        Some(user) => verify_password(&payload.password, &user.password_hash)?,
        None => false,
    };

    if !is_valid {
        record_login_attempt(&state.db, email, false).await?;
        return Err(ApiError::InvalidCredentials);
    }

    let user = user_opt.unwrap();
    if !user.is_active {
        return Err(ApiError::Forbidden("Account is deactivated".into()));
    }

    // 3. Record successful login attempt
    record_login_attempt(&state.db, email, true).await?;

    // 4. Create new 30-day session
    let raw_token = generate_session_token();
    let token_hash = hash_session_token(&raw_token);
    let expires_at = Utc::now() + Duration::days(30);

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    create_session(
        &state.db,
        CreateSession {
            user_id: user.id,
            token_hash,
            expires_at,
            user_agent,
        },
    )
    .await?;

    // 5. Derive CSRF token
    let csrf_token = derive_csrf_token(&raw_token, &state.config.session_secret);

    // 6. Build cookie header
    let is_prod = state.config.pace_env == "production";
    let cookie_val = format!(
        "pace_session={raw_token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        30 * 24 * 60 * 60,
        if is_prod { "; Secure" } else { "" }
    );

    let profile = UserProfile {
        id: user.id,
        email: user.email,
        name: user.name,
        role: user.role,
        must_change_password: user.must_change_password,
        storage_quota_bytes: user.storage_quota_bytes,
        storage_used_bytes: user.storage_used_bytes,
    };

    let mut response = (
        StatusCode::OK,
        Json(LoginResponse {
            user: profile,
            csrf_token,
        }),
    )
        .into_response();

    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie_val.parse().map_err(|e| ApiError::Internal(format!("Invalid cookie header: {e}")))?,
    );

    Ok(response)
}

/// POST /api/v1/auth/logout
pub async fn logout(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<impl IntoResponse, ApiError> {
    delete_session(&state.db, auth.session_id).await?;

    let is_prod = state.config.pace_env == "production";
    let clear_cookie = format!(
        "pace_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if is_prod { "; Secure" } else { "" }
    );

    let mut response = (
        StatusCode::OK,
        Json(MessageResponse {
            status: "ok",
            message: "Logged out successfully",
        }),
    )
        .into_response();

    response.headers_mut().insert(
        header::SET_COOKIE,
        clear_cookie.parse().map_err(|e| ApiError::Internal(format!("Invalid cookie header: {e}")))?,
    );

    Ok(response)
}

/// GET /api/v1/auth/me
pub async fn me(auth: AuthContext) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(UserProfile {
            id: auth.user.id,
            email: auth.user.email,
            name: auth.user.name,
            role: auth.user.role,
            must_change_password: auth.user.must_change_password,
            storage_quota_bytes: auth.user.storage_quota_bytes,
            storage_used_bytes: auth.user.storage_used_bytes,
        }),
    )
}

/// POST /api/v1/auth/password
pub async fn change_password(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if payload.new_password.len() < 10 {
        return Err(ApiError::BadRequest(
            "New password must be at least 10 characters".into(),
        ));
    }

    if !verify_password(&payload.current_password, &auth.user.password_hash)? {
        return Err(ApiError::BadRequest(
            "Current password is incorrect".into(),
        ));
    }

    let new_hash = hash_password(&payload.new_password)?;
    update_user_password(&state.db, auth.user.id, &new_hash).await?;

    Ok((
        StatusCode::OK,
        Json(MessageResponse {
            status: "ok",
            message: "Password changed successfully",
        }),
    ))
}
