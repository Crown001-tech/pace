use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pace_db::{
    create_user, delete_all_user_sessions, find_user_by_id, list_users, reset_user_password,
    set_user_active, CreateUser,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{extractors::RequireAdmin, passwords::hash_password},
    error::ApiError,
    routes::auth::{MessageResponse, UserProfile},
};

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub name: String,
    pub temporary_password: String,
    pub role: Option<String>,
    pub storage_quota_bytes: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserStatusRequest {
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub temporary_password: String,
}

/// POST /api/v1/admin/users
/// Creates a new user with a temporary password and quota. Requires admin role.
pub async fn create_new_user(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Json(payload): Json<CreateUserRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if payload.temporary_password.len() < 10 {
        return Err(ApiError::BadRequest(
            "Temporary password must be at least 10 characters".into(),
        ));
    }

    let password_hash = hash_password(&payload.temporary_password)?;
    let role = payload.role.unwrap_or_else(|| "creator".to_string());

    let user = create_user(
        &state.db,
        CreateUser {
            email: payload.email.trim().to_string(),
            name: payload.name.trim().to_string(),
            password_hash,
            role,
            must_change_password: true, // Forces password change on first sign-in (AUTH-03)
            storage_quota_bytes: payload.storage_quota_bytes,
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(UserProfile {
            id: user.id,
            email: user.email,
            name: user.name,
            role: user.role,
            must_change_password: user.must_change_password,
            storage_quota_bytes: user.storage_quota_bytes,
            storage_used_bytes: user.storage_used_bytes,
        }),
    ))
}

/// GET /api/v1/admin/users
/// Lists all registered users.
pub async fn get_all_users(
    State(state): State<AppState>,
    _admin: RequireAdmin,
) -> Result<impl IntoResponse, ApiError> {
    let users = list_users(&state.db).await?;
    let profiles: Vec<UserProfile> = users
        .into_iter()
        .map(|u| UserProfile {
            id: u.id,
            email: u.email,
            name: u.name,
            role: u.role,
            must_change_password: u.must_change_password,
            storage_quota_bytes: u.storage_quota_bytes,
            storage_used_bytes: u.storage_used_bytes,
        })
        .collect();

    Ok((StatusCode::OK, Json(profiles)))
}

/// PATCH /api/v1/admin/users/:id
/// Activates or deactivates a user. If deactivated, immediately terminates all active sessions (AUTH-04).
pub async fn update_user_status(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateUserStatusRequest>,
) -> Result<impl IntoResponse, ApiError> {
    set_user_active(&state.db, user_id, payload.is_active).await?;

    // If deactivated, kill all active sessions immediately
    if !payload.is_active {
        delete_all_user_sessions(&state.db, user_id).await?;
    }

    let user = find_user_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".into()))?;

    Ok((
        StatusCode::OK,
        Json(UserProfile {
            id: user.id,
            email: user.email,
            name: user.name,
            role: user.role,
            must_change_password: user.must_change_password,
            storage_quota_bytes: user.storage_quota_bytes,
            storage_used_bytes: user.storage_used_bytes,
        }),
    ))
}

/// POST /api/v1/admin/users/:id/reset-password
/// Resets a user's password to a temporary password, flags must_change_password = true,
/// and terminates all active sessions.
pub async fn admin_reset_password(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if payload.temporary_password.len() < 10 {
        return Err(ApiError::BadRequest(
            "Temporary password must be at least 10 characters".into(),
        ));
    }

    let password_hash = hash_password(&payload.temporary_password)?;
    reset_user_password(&state.db, user_id, &password_hash).await?;

    // Invalidate all active sessions so the user must log in again with temporary password
    delete_all_user_sessions(&state.db, user_id).await?;

    Ok((
        StatusCode::OK,
        Json(MessageResponse {
            status: "ok",
            message: "User password reset successfully; all active sessions terminated",
        }),
    ))
}
