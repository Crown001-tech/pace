use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use pace_db::{
    create_project, delete_project, find_project_by_id_and_owner, list_projects_by_owner,
    update_project, CreateProject, Project, UpdateProject,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::extractors::AuthContext,
    error::{ApiError, ProblemDetails},
};

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateProjectRequest {
    pub name: String,
    pub aspect_ratio: String,
    #[serde(default)]
    pub fps: Option<i32>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub effect_seed: Option<i64>,
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateProjectRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub aspect_ratio: Option<String>,
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub version: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub aspect_ratio: String,
    pub fps: i32,
    pub language: String,
    pub effect_seed: i64,
    pub settings: serde_json::Value,
    pub version: i32,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<Project> for ProjectResponse {
    fn from(p: Project) -> Self {
        Self {
            id: p.id,
            owner_id: p.owner_id,
            name: p.name,
            aspect_ratio: p.aspect_ratio,
            fps: p.fps,
            language: p.language,
            effect_seed: p.effect_seed,
            settings: p.settings,
            version: p.version,
            status: p.status,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

/// POST /api/v1/projects
/// Creates a new project for the authenticated user (PRJ-01, AUTH-05).
#[utoipa::path(
    post,
    path = "/api/v1/projects",
    tag = "Projects",
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "Project created successfully", body = ProjectResponse,
            headers(
                ("ETag" = String, description = "Entity tag representing project version"),
                ("Location" = String, description = "URI of the newly created project")
            )
        ),
        (status = 400, description = "Validation error", body = ProblemDetails),
        (status = 401, description = "Authentication required", body = ProblemDetails)
    )
)]
pub async fn create_project_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let name = payload.name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(ApiError::BadRequest(
            "Project name must be between 1 and 255 characters".into(),
        ));
    }

    if payload.aspect_ratio != "16:9" && payload.aspect_ratio != "9:16" {
        return Err(ApiError::BadRequest(
            "Invalid aspect_ratio. Must be '16:9' or '9:16'".into(),
        ));
    }

    if let Some(fps) = payload.fps {
        if fps != 30 {
            return Err(ApiError::BadRequest(
                "Invalid fps. Only 30 fps is supported in v1".into(),
            ));
        }
    }

    let project = create_project(
        &state.db,
        CreateProject {
            owner_id: auth.user.id,
            name: name.to_string(),
            aspect_ratio: payload.aspect_ratio,
            fps: payload.fps,
            language: payload.language,
            effect_seed: payload.effect_seed,
            settings: payload.settings,
        },
    )
    .await?;

    let etag_val = format!("\"{}\"", project.version);
    let location_val = format!("/api/v1/projects/{}", project.id);
    let response = ProjectResponse::from(project);

    Ok((
        StatusCode::CREATED,
        [(header::ETAG, etag_val), (header::LOCATION, location_val)],
        Json(response),
    ))
}

/// GET /api/v1/projects
/// Lists all active projects owned by the authenticated user (PRJ-02, AUTH-05).
#[utoipa::path(
    get,
    path = "/api/v1/projects",
    tag = "Projects",
    responses(
        (status = 200, description = "List of user projects sorted by updated_at DESC", body = Vec<ProjectResponse>),
        (status = 401, description = "Authentication required", body = ProblemDetails)
    )
)]
pub async fn list_projects_handler(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<impl IntoResponse, ApiError> {
    let projects = list_projects_by_owner(&state.db, auth.user.id).await?;
    let response: Vec<ProjectResponse> = projects.into_iter().map(ProjectResponse::from).collect();
    Ok(Json(response))
}

/// GET /api/v1/projects/{id}
/// Retrieves a specific project owned by the authenticated user (PRJ-03, AUTH-05).
/// Returns 404 Not Found if the project doesn't exist or is owned by another user.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}",
    tag = "Projects",
    params(
        ("id" = Uuid, Path, description = "Project UUID")
    ),
    responses(
        (status = 200, description = "Project details", body = ProjectResponse,
            headers(
                ("ETag" = String, description = "Entity tag representing current project version")
            )
        ),
        (status = 401, description = "Authentication required", body = ProblemDetails),
        (status = 404, description = "Project not found or owned by another user", body = ProblemDetails)
    )
)]
pub async fn get_project_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let project = find_project_by_id_and_owner(&state.db, id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".into()))?;

    let etag_val = format!("\"{}\"", project.version);
    let response = ProjectResponse::from(project);

    Ok(([(header::ETAG, etag_val)], Json(response)))
}

/// PATCH /api/v1/projects/{id}
/// Updates project metadata with optimistic concurrency control (PRJ-04, PRJ-05, AUTH-05).
/// Requires an If-Match header or version field matching the current version.
#[utoipa::path(
    patch,
    path = "/api/v1/projects/{id}",
    tag = "Projects",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("If-Match" = Option<String>, Header, description = "Expected project version for optimistic concurrency control")
    ),
    request_body = UpdateProjectRequest,
    responses(
        (status = 200, description = "Project updated successfully", body = ProjectResponse,
            headers(
                ("ETag" = String, description = "Entity tag representing updated project version")
            )
        ),
        (status = 400, description = "Validation error or missing version", body = ProblemDetails),
        (status = 401, description = "Authentication required", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails),
        (status = 409, description = "Project version conflict", body = ProblemDetails)
    )
)]
pub async fn update_project_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<UpdateProjectRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let expected_version = if let Some(if_match) = headers.get(header::IF_MATCH) {
        let raw = if_match
            .to_str()
            .map_err(|_| ApiError::BadRequest("Invalid characters in If-Match header".into()))?;
        let cleaned = raw.trim().trim_matches('"');
        cleaned.parse::<i32>().map_err(|_| {
            ApiError::BadRequest("If-Match header must contain an integer version".into())
        })?
    } else if let Some(v) = payload.version {
        v
    } else {
        return Err(ApiError::BadRequest(
            "If-Match header or version field is required for concurrent modification control"
                .into(),
        ));
    };

    if let Some(ref name) = payload.name {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.len() > 255 {
            return Err(ApiError::BadRequest(
                "Project name must be between 1 and 255 characters".into(),
            ));
        }
    }

    if let Some(ref aspect_ratio) = payload.aspect_ratio {
        if aspect_ratio != "16:9" && aspect_ratio != "9:16" {
            return Err(ApiError::BadRequest(
                "Invalid aspect_ratio. Must be '16:9' or '9:16'".into(),
            ));
        }
    }

    if let Some(ref status) = payload.status {
        if status != "active" && status != "archived" {
            return Err(ApiError::BadRequest(
                "Invalid status. Must be 'active' or 'archived'".into(),
            ));
        }
    }

    let updated = update_project(
        &state.db,
        id,
        auth.user.id,
        expected_version,
        UpdateProject {
            name: payload.name.map(|n| n.trim().to_string()),
            aspect_ratio: payload.aspect_ratio,
            settings: payload.settings,
            status: payload.status,
        },
    )
    .await?;

    let etag_val = format!("\"{}\"", updated.version);
    let response = ProjectResponse::from(updated);

    Ok(([(header::ETAG, etag_val)], Json(response)))
}

/// DELETE /api/v1/projects/{id}
/// Soft-deletes a project owned by the authenticated user (PRJ-06, AUTH-05).
#[utoipa::path(
    delete,
    path = "/api/v1/projects/{id}",
    tag = "Projects",
    params(
        ("id" = Uuid, Path, description = "Project UUID")
    ),
    responses(
        (status = 204, description = "Project soft-deleted successfully"),
        (status = 401, description = "Authentication required", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn delete_project_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    delete_project(&state.db, id, auth.user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
