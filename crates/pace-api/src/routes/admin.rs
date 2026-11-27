use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pace_db::{
    create_user, delete_all_user_sessions, find_user_by_email, find_user_by_id, list_users,
    record_audit_entry, reset_user_password, set_user_active, CreateAuditEntry, CreateUser,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{
        extractors::RequireAdmin,
        passwords::{hash_password, validate_password_strength},
    },
    error::{ApiError, ProblemDetails},
    routes::auth::{MessageResponse, UserProfile},
};

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateUserRequest {
    pub email: String,
    pub name: String,
    pub temporary_password: String,
    pub role: Option<String>,
    pub storage_quota_bytes: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct UpdateUserStatusRequest {
    pub is_active: bool,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ResetPasswordRequest {
    pub temporary_password: String,
}

/// POST /api/v1/admin/users
/// Creates a new user with a temporary password and quota. Requires admin role.
#[utoipa::path(
    post,
    path = "/api/v1/admin/users",
    tag = "Admin",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created successfully", body = UserProfile),
        (status = 400, description = "Validation error", body = ProblemDetails),
        (status = 403, description = "Admin role required", body = ProblemDetails)
    )
)]
pub async fn create_new_user(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Json(payload): Json<CreateUserRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_password_strength(&payload.temporary_password)?;

    let email = payload.email.trim();
    let name = payload.name.trim();
    if email.is_empty() || !email.contains('@') {
        return Err(ApiError::BadRequest("A valid email address is required".into()));
    }
    if name.is_empty() || name.len() > 255 {
        return Err(ApiError::BadRequest(
            "Name must be between 1 and 255 characters".into(),
        ));
    }

    let role = payload.role.unwrap_or_else(|| "creator".to_string());
    if role != "creator" && role != "admin" {
        return Err(ApiError::BadRequest(
            "Invalid role. Must be 'creator' or 'admin'".into(),
        ));
    }

    if let Some(quota) = payload.storage_quota_bytes {
        if quota <= 0 {
            return Err(ApiError::BadRequest(
                "storage_quota_bytes must be a positive integer".into(),
            ));
        }
    }

    if find_user_by_email(&state.db, email).await?.is_some() {
        return Err(ApiError::Conflict(
            "A user with this email already exists".into(),
        ));
    }

    let password_hash = hash_password(&payload.temporary_password)?;

    let user = create_user(
        &state.db,
        CreateUser {
            email: email.to_string(),
            name: name.to_string(),
            password_hash,
            role: role.clone(),
            must_change_password: true, // Forces password change on first sign-in (AUTH-03)
            storage_quota_bytes: payload.storage_quota_bytes,
        },
    )
    .await?;

    record_audit_entry(
        &state.db,
        CreateAuditEntry {
            actor_id: Some(admin.0.user.id),
            action: "user.create".to_string(),
            target_type: "user".to_string(),
            target_id: Some(user.id.to_string()),
            details: Some(serde_json::json!({
                "email": user.email,
                "role": user.role,
                "storage_quota_bytes": user.storage_quota_bytes,
            })),
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
            csrf_token: None,
        }),
    ))
}

/// GET /api/v1/admin/users
/// Lists all registered users.
#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    tag = "Admin",
    responses(
        (status = 200, description = "List of all registered users", body = Vec<UserProfile>),
        (status = 403, description = "Admin role required", body = ProblemDetails)
    )
)]
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
            csrf_token: None,
        })
        .collect();

    Ok((StatusCode::OK, Json(profiles)))
}

/// PATCH /api/v1/admin/users/{id}
/// Activates or deactivates a user. If deactivated, immediately terminates all active sessions (AUTH-04).
#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{id}",
    tag = "Admin",
    params(
        ("id" = Uuid, Path, description = "User UUID")
    ),
    request_body = UpdateUserStatusRequest,
    responses(
        (status = 200, description = "User status updated", body = UserProfile),
        (status = 404, description = "User not found", body = ProblemDetails),
        (status = 403, description = "Admin role required", body = ProblemDetails)
    )
)]
pub async fn update_user_status(
    State(state): State<AppState>,
    admin: RequireAdmin,
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

    record_audit_entry(
        &state.db,
        CreateAuditEntry {
            actor_id: Some(admin.0.user.id),
            action: if payload.is_active {
                "user.activate".to_string()
            } else {
                "user.deactivate".to_string()
            },
            target_type: "user".to_string(),
            target_id: Some(user.id.to_string()),
            details: Some(serde_json::json!({ "is_active": payload.is_active })),
        },
    )
    .await?;

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
            csrf_token: None,
        }),
    ))
}

/// POST /api/v1/admin/users/{id}/reset-password
/// Resets a user's password to a temporary password, flags must_change_password = true,
/// and terminates all active sessions.
#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{id}/reset-password",
    tag = "Admin",
    params(
        ("id" = Uuid, Path, description = "User UUID")
    ),
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset successfully", body = MessageResponse),
        (status = 400, description = "Validation error", body = ProblemDetails),
        (status = 403, description = "Admin role required", body = ProblemDetails)
    )
)]
pub async fn admin_reset_password(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_password_strength(&payload.temporary_password)?;

    let password_hash = hash_password(&payload.temporary_password)?;
    reset_user_password(&state.db, user_id, &password_hash).await?;

    // Invalidate all active sessions so the user must log in again with temporary password
    delete_all_user_sessions(&state.db, user_id).await?;

    record_audit_entry(
        &state.db,
        CreateAuditEntry {
            actor_id: Some(admin.0.user.id),
            action: "user.reset_password".to_string(),
            target_type: "user".to_string(),
            target_id: Some(user_id.to_string()),
            details: None,
        },
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(MessageResponse {
            status: "ok",
            message: "User password reset successfully; all active sessions terminated",
        }),
    ))
}
