use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use pace_jobs::JobRecord;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthContext,
    error::{ApiError, ProblemDetails},
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobStatusDto {
    pub id: Uuid,
    pub queue: String,
    pub job_type: String,
    pub status: String,
    pub progress: i32,
    pub stage: Option<String>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub result: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<JobRecord> for JobStatusDto {
    fn from(j: JobRecord) -> Self {
        Self {
            id: j.id,
            queue: j.queue,
            job_type: j.job_type,
            status: j.status,
            progress: j.progress,
            stage: j.stage,
            attempts: j.attempts,
            max_attempts: j.max_attempts,
            error_code: j.error_code,
            error_message: j.error_message,
            result: j.result,
            created_at: j.created_at,
            updated_at: j.updated_at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/jobs/{job_id}",
    tag = "Narration",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("job_id" = Uuid, Path, description = "Job UUID")
    ),
    responses(
        (status = 200, description = "Current job status and progress", body = JobStatusDto),
        (status = 404, description = "Project or job not found", body = ProblemDetails)
    )
)]
pub async fn get_project_job(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, job_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let job = pace_jobs::find_job_by_id_and_project(&state.db, job_id, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Job not found".to_string()))?;

    Ok((StatusCode::OK, Json(JobStatusDto::from(job))))
}
