use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

/// RFC 9457 Problem Details payload.
/// Returned for all 4xx and 5xx API errors with Content-Type: application/problem+json.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

/// Stable machine-readable error codes as defined in the PACE Technical Design Document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidCredentials,
    AccountLocked,
    Unauthorized,
    PasswordChangeRequired,
    Forbidden,
    NotFound,
    ProjectVersionConflict,
    BadRequest,
    InvalidCsrfToken,
    UploadQuotaExceeded,
    ServiceUnavailable,
    InternalServerError,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidCredentials => "INVALID_CREDENTIALS",
            Self::AccountLocked => "ACCOUNT_LOCKED",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::PasswordChangeRequired => "PASSWORD_CHANGE_REQUIRED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::ProjectVersionConflict => "PROJECT_VERSION_CONFLICT",
            Self::BadRequest => "BAD_REQUEST",
            Self::InvalidCsrfToken => "INVALID_CSRF_TOKEN",
            Self::UploadQuotaExceeded => "UPLOAD_QUOTA_EXCEEDED",
            Self::ServiceUnavailable => "SERVICE_UNAVAILABLE",
            Self::InternalServerError => "INTERNAL_SERVER_ERROR",
        }
    }

    pub fn default_title(&self) -> &'static str {
        match self {
            Self::InvalidCredentials => "Invalid credentials",
            Self::AccountLocked => "Account locked due to too many failed attempts",
            Self::Unauthorized => "Authentication required",
            Self::PasswordChangeRequired => "Password change required",
            Self::Forbidden => "Access forbidden",
            Self::NotFound => "Resource not found",
            Self::ProjectVersionConflict => "Project version conflict",
            Self::BadRequest => "Invalid request",
            Self::InvalidCsrfToken => "Invalid CSRF token",
            Self::UploadQuotaExceeded => "Storage quota exceeded",
            Self::ServiceUnavailable => "Service unavailable",
            Self::InternalServerError => "Internal server error",
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidCredentials | Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::AccountLocked => StatusCode::TOO_MANY_REQUESTS,
            Self::PasswordChangeRequired | Self::Forbidden | Self::InvalidCsrfToken => {
                StatusCode::FORBIDDEN
            }
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::ProjectVersionConflict => StatusCode::CONFLICT,
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::UploadQuotaExceeded => StatusCode::PAYLOAD_TOO_LARGE,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Central API error enum for all PACE endpoints.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Account locked: 10 failed login attempts in 15 minutes. Try again later.")]
    AccountLocked,

    #[error("Authentication required")]
    Unauthorized,

    #[error("Temporary password in use. Password change required before proceeding.")]
    PasswordChangeRequired,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Project version conflict: current version is {current_version}")]
    ProjectVersionConflict { current_version: i32 },

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("CSRF token missing or invalid")]
    InvalidCsrfToken,

    #[error("Storage quota exceeded ({current_bytes} / {max_bytes} bytes used)")]
    QuotaExceeded { current_bytes: i64, max_bytes: i64 },

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Database error: {0}")]
    Database(#[from] pace_db::DbError),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl ApiError {
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::InvalidCredentials => ErrorCode::InvalidCredentials,
            Self::AccountLocked => ErrorCode::AccountLocked,
            Self::Unauthorized => ErrorCode::Unauthorized,
            Self::PasswordChangeRequired => ErrorCode::PasswordChangeRequired,
            Self::Forbidden(_) => ErrorCode::Forbidden,
            Self::NotFound(_) => ErrorCode::NotFound,
            Self::ProjectVersionConflict { .. } => ErrorCode::ProjectVersionConflict,
            Self::BadRequest(_) => ErrorCode::BadRequest,
            Self::InvalidCsrfToken => ErrorCode::InvalidCsrfToken,
            Self::QuotaExceeded { .. } => ErrorCode::UploadQuotaExceeded,
            Self::ServiceUnavailable(_) => ErrorCode::ServiceUnavailable,
            Self::Database(_) | Self::Internal(_) => ErrorCode::InternalServerError,
        }
    }

    pub fn to_problem_details(&self, instance: Option<String>) -> ProblemDetails {
        let code = self.error_code();
        let status = code.status_code();
        let detail = match self {
            Self::Database(err) => {
                tracing::error!(error = %err, "Database error encountered");
                "A database error occurred. Please try again later.".to_string()
            }
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "Internal error encountered");
                "An unexpected internal error occurred.".to_string()
            }
            _ => self.to_string(),
        };

        ProblemDetails {
            problem_type: format!("https://pace.video/errors/{}", code.as_str()),
            title: code.default_title().to_string(),
            status: status.as_u16(),
            detail,
            code: code.as_str().to_string(),
            instance,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.error_code();
        let status = code.status_code();
        let problem = self.to_problem_details(None);

        let mut res = (status, Json(problem)).into_response();
        res.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_mapping_and_problem_json() {
        let conflict = ApiError::ProjectVersionConflict { current_version: 5 };
        assert_eq!(conflict.error_code(), ErrorCode::ProjectVersionConflict);
        assert_eq!(conflict.error_code().status_code(), StatusCode::CONFLICT);

        let problem = conflict.to_problem_details(Some("/api/v1/projects/123".to_string()));
        assert_eq!(problem.code, "PROJECT_VERSION_CONFLICT");
        assert_eq!(problem.status, 409);
        assert_eq!(
            problem.problem_type,
            "https://pace.video/errors/PROJECT_VERSION_CONFLICT"
        );
        assert_eq!(
            problem.instance,
            Some("/api/v1/projects/123".to_string())
        );
    }

    #[test]
    fn test_internal_database_error_masks_raw_sql_in_detail() {
        let db_err = ApiError::Internal("Raw DB syntax error near line 42".to_string());
        let problem = db_err.to_problem_details(None);
        assert_eq!(problem.status, 500);
        assert_eq!(problem.code, "INTERNAL_SERVER_ERROR");
        assert_eq!(problem.detail, "An unexpected internal error occurred.");
    }
}
