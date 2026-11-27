use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pace_db::SceneMedia;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthContext,
    error::{ApiError, ProblemDetails},
    routes::effects::{validate_fit_mode, validate_focal_coord, SceneEffectDto},
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct AssignSectionRequest {
    pub media_id: Uuid,
    pub fit_mode: Option<String>,
    pub focal_x: Option<f32>,
    pub focal_y: Option<f32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SwapSectionsRequest {
    pub section_id_a: Uuid,
    pub section_id_b: Uuid,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AssignmentResponse {
    pub id: Uuid,
    pub section_id: Uuid,
    pub media_id: Uuid,
    pub fit_mode: String,
    pub focal_x: f32,
    pub focal_y: f32,
    pub effect: SceneEffectDto,
    pub effect_overridden: bool,
}

impl From<SceneMedia> for AssignmentResponse {
    fn from(sm: SceneMedia) -> Self {
        Self {
            id: sm.id,
            section_id: sm.section_id,
            media_id: sm.media_id,
            fit_mode: sm.fit_mode,
            focal_x: sm.focal_x,
            focal_y: sm.focal_y,
            effect: SceneEffectDto::from_json(&sm.effect),
            effect_overridden: sm.effect_overridden,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/assignments",
    tag = "Assignments",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "List of section assignments", body = Vec<AssignmentResponse>),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn list_assignments(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let assignments = pace_db::get_assignments_by_project(&state.db, project_id).await?;

    let responses: Vec<AssignmentResponse> = assignments.into_iter().map(Into::into).collect();
    Ok((StatusCode::OK, Json(responses)))
}

#[utoipa::path(
    put,
    path = "/api/v1/projects/{id}/assignments/auto",
    tag = "Assignments",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Auto-assigned media to sections 1-to-1", body = Vec<AssignmentResponse>),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn auto_assign(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let sections = pace_db::get_sections_by_project(&state.db, project_id).await?;
    let image_assets: Vec<_> = pace_db::find_by_project(&state.db, project_id)
        .await?
        .into_iter()
        .filter(|asset| asset.kind == "image")
        .collect();

    let mut results = Vec::new();
    // Assign image k to section k (ASN-01)
    for (idx, sec) in sections.iter().enumerate() {
        if idx < image_assets.len() {
            let assigned = pace_db::assign_media(
                &state.db,
                project_id,
                sec.id,
                image_assets[idx].id,
                "fill",
                0.5,
                0.5,
            )
            .await?;

            results.push(AssignmentResponse::from(assigned));
        } else {
            // Unassign section if fewer images than sections (MED-06)
            let _ = pace_db::unassign_section(&state.db, project_id, sec.id).await?;
        }
    }

    Ok((StatusCode::OK, Json(results)))
}

#[utoipa::path(
    put,
    path = "/api/v1/projects/{id}/assignments/swap",
    tag = "Assignments",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = SwapSectionsRequest,
    responses(
        (status = 200, description = "Sections swapped successfully"),
        (status = 404, description = "Project or sections not found", body = ProblemDetails)
    )
)]
pub async fn swap_sections(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<SwapSectionsRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    pace_db::swap_assignments(&state.db, project_id, req.section_id_a, req.section_id_b).await?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    put,
    path = "/api/v1/projects/{id}/assignments/{section_id}",
    tag = "Assignments",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("section_id" = Uuid, Path, description = "Section UUID")
    ),
    request_body = AssignSectionRequest,
    responses(
        (status = 200, description = "Section assigned", body = AssignmentResponse),
        (status = 400, description = "Invalid framing parameters", body = ProblemDetails),
        (status = 404, description = "Project, section, or media asset not found", body = ProblemDetails)
    )
)]
pub async fn assign_section(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, section_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<AssignSectionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let fit_mode = validate_fit_mode(req.fit_mode.as_deref().unwrap_or("fill"))?;
    let focal_x = validate_focal_coord(req.focal_x.unwrap_or(0.5), "x")?;
    let focal_y = validate_focal_coord(req.focal_y.unwrap_or(0.5), "y")?;

    let assigned = pace_db::assign_media(
        &state.db,
        project_id,
        section_id,
        req.media_id,
        fit_mode,
        focal_x,
        focal_y,
    )
    .await?;

    Ok((StatusCode::OK, Json(AssignmentResponse::from(assigned))))
}

#[utoipa::path(
    delete,
    path = "/api/v1/projects/{id}/assignments/{section_id}",
    tag = "Assignments",
    params(
        ("id" = Uuid, Path, description = "Project UUID"),
        ("section_id" = Uuid, Path, description = "Section UUID")
    ),
    responses(
        (status = 204, description = "Section unassigned (now Empty)"),
        (status = 404, description = "Project or section not found", body = ProblemDetails)
    )
)]
pub async fn unassign_section(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_id, section_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let _ = pace_db::unassign_section(&state.db, project_id, section_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
