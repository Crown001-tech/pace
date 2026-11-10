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
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{
        csrf::derive_csrf_token,
        extractors::AuthContext,
        passwords::{hash_password, verify_password},
        sessions::{generate_session_token, hash_session_token},
    },
    error::{ApiError, ProblemDetails},
};

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserProfile {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub must_change_password: bool,
    pub storage_quota_bytes: i64,
    pub storage_used_bytes: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    pub user: UserProfile,
    pub csrf_token: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MessageResponse {
    pub status: &'static str,
    pub message: &'static str,
}

/// POST /api/v1/auth/login
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Logged in successfully", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = ProblemDetails),
        (status = 429, description = "Account locked", body = ProblemDetails)
    )
)]
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

    // 2. Fetch user
    let user = match find_user_by_email(&state.db, email).await? {
        Some(u) => u,
        None => {
            record_login_attempt(&state.db, email, false).await?;
            return Err(ApiError::InvalidCredentials);
        }
    };

    // If deactivated, fail silently with invalid credentials (or account locked)
    if !user.is_active {
        record_login_attempt(&state.db, email, false).await?;
        return Err(ApiError::InvalidCredentials);
    }

    // 3. Verify Argon2id password
    let valid = verify_password(&payload.password, &user.password_hash)?;
    record_login_attempt(&state.db, email, valid).await?;

    if !valid {
        return Err(ApiError::InvalidCredentials);
    }

    // 4. Create 30-day session
    let session_token = generate_session_token();
    let token_hash = hash_session_token(&session_token);
    let expires_at = Utc::now() + Duration::days(30);

    create_session(
        &state.db,
        CreateSession {
            user_id: user.id,
            token_hash,
            user_agent: headers
                .get(header::USER_AGENT)
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string()),
            expires_at,
        },
    )
    .await?;

    // 5. Derive CSRF token
    let csrf_token = derive_csrf_token(&session_token, &state.config.session_secret);

    // 6. Build response with httpOnly session cookie
    let is_prod = state.config.pace_env == "production";
    let cookie_val = format!(
        "pace_session={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        session_token,
        30 * 24 * 60 * 60,
        if is_prod { "; Secure" } else { "" }
    );

    let mut response = (
        StatusCode::OK,
        Json(LoginResponse {
            user: UserProfile {
                id: user.id,
                email: user.email,
                name: user.name,
                role: user.role,
                must_change_password: user.must_change_password,
                storage_quota_bytes: user.storage_quota_bytes,
                storage_used_bytes: user.storage_used_bytes,
            },
            csrf_token,
        }),
    )
        .into_response();

    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie_val
            .parse()
            .map_err(|e| ApiError::Internal(format!("Invalid cookie header: {e}")))?,
    );

    Ok(response)
}

/// POST /api/v1/auth/logout
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "Auth",
    responses(
        (status = 200, description = "Logged out successfully", body = MessageResponse)
    )
)]
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
        clear_cookie
            .parse()
            .map_err(|e| ApiError::Internal(format!("Invalid cookie header: {e}")))?,
    );

    Ok(response)
}

/// GET /api/v1/auth/me
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "Auth",
    responses(
        (status = 200, description = "Current user profile", body = UserProfile),
        (status = 401, description = "Unauthorized", body = ProblemDetails)
    )
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/auth/password",
    tag = "Auth",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed successfully", body = MessageResponse),
        (status = 400, description = "Validation error", body = ProblemDetails),
        (status = 401, description = "Unauthorized", body = ProblemDetails)
    )
)]
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
        return Err(ApiError::BadRequest("Current password is incorrect".into()));
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
