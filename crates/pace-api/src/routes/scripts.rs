use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use pace_db::{Section, UpsertSection};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthContext,
    error::{ApiError, ProblemDetails},
};

const MAX_SECTIONS_PER_PROJECT: usize = 1000;

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertSectionItem {
    #[serde(default)]
    pub id: Option<Uuid>,
    pub text: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveScriptRequest {
    #[serde(default)]
    pub raw_text: Option<String>,
    pub sections: Vec<UpsertSectionItem>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SectionDto {
    pub id: Uuid,
    pub position: i32,
    pub text: String,
    pub needs_resync: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Section> for SectionDto {
    fn from(s: Section) -> Self {
        Self {
            id: s.id,
            position: s.position,
            text: s.text,
            needs_resync: s.needs_resync,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ScriptResponse {
    pub project_id: Uuid,
    pub raw_text: String,
    pub sections: Vec<SectionDto>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{id}/script",
    tag = "Scripts",
    params(("id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Project script and ordered sections", body = ScriptResponse),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn get_script_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    let script = pace_db::get_script(&state.db, project_id).await?;
    let sections = pace_db::get_sections_by_project(&state.db, project_id).await?;

    let raw_text = match &script {
        Some(rec) => rec.raw_text.clone(),
        None => project
            .settings
            .get("raw_script")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };

    Ok((
        StatusCode::OK,
        Json(ScriptResponse {
            project_id,
            raw_text,
            sections: sections.into_iter().map(Into::into).collect(),
            updated_at: script.map(|s| s.updated_at),
        }),
    ))
}

#[utoipa::path(
    put,
    path = "/api/v1/projects/{id}/script",
    tag = "Scripts",
    params(("id" = Uuid, Path, description = "Project UUID")),
    request_body = SaveScriptRequest,
    responses(
        (status = 200, description = "Script and sections saved (IDs preserved)", body = ScriptResponse),
        (status = 400, description = "Invalid script payload", body = ProblemDetails),
        (status = 404, description = "Project not found", body = ProblemDetails)
    )
)]
pub async fn save_script_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_id): Path<Uuid>,
    Json(req): Json<SaveScriptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _project = pace_db::find_project_by_id_and_owner(&state.db, project_id, auth.user.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".to_string()))?;

    if req.sections.len() > MAX_SECTIONS_PER_PROJECT {
        return Err(ApiError::BadRequest(format!(
            "Maximum {MAX_SECTIONS_PER_PROJECT} sections allowed per project"
        )));
    }

    let items: Vec<UpsertSection> = req
        .sections
        .into_iter()
        .map(|s| UpsertSection {
            id: s.id,
            text: s.text,
        })
        .collect();

    let raw_text = req.raw_text.unwrap_or_else(|| {
        items
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    });

    let (script, saved_sections) =
        pace_db::upsert_script_and_sections(&state.db, project_id, &raw_text, &items).await?;

    Ok((
        StatusCode::OK,
        Json(ScriptResponse {
            project_id,
            raw_text: script.raw_text,
            sections: saved_sections.into_iter().map(Into::into).collect(),
            updated_at: Some(script.updated_at),
        }),
    ))
}
