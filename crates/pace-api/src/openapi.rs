use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::health::healthz,
        crate::routes::health::readyz,
        crate::routes::auth::login,
        crate::routes::auth::logout,
        crate::routes::auth::me,
        crate::routes::auth::change_password,
        crate::routes::admin::create_new_user,
        crate::routes::admin::get_all_users,
        crate::routes::admin::update_user_status,
        crate::routes::admin::admin_reset_password,
        crate::routes::projects::create_project_handler,
        crate::routes::projects::list_projects_handler,
        crate::routes::projects::get_project_handler,
        crate::routes::projects::update_project_handler,
        crate::routes::projects::delete_project_handler,
    ),
    components(
        schemas(
            crate::error::ProblemDetails,
            crate::routes::health::HealthResponse,
            crate::routes::health::ReadinessResponse,
            crate::routes::auth::LoginRequest,
            crate::routes::auth::LoginResponse,
            crate::routes::auth::UserProfile,
            crate::routes::auth::ChangePasswordRequest,
            crate::routes::auth::MessageResponse,
            crate::routes::admin::CreateUserRequest,
            crate::routes::admin::UpdateUserStatusRequest,
            crate::routes::admin::ResetPasswordRequest,
            crate::routes::projects::CreateProjectRequest,
            crate::routes::projects::UpdateProjectRequest,
            crate::routes::projects::ProjectResponse,
        )
    ),
    tags(
        (name = "Health", description = "Liveness and readiness health probes"),
        (name = "Auth", description = "Authentication and session management"),
        (name = "Admin", description = "Administrative user operations"),
        (name = "Projects", description = "Project creation and metadata management"),
    ),
    info(
        title = "PACE API",
        version = "0.1.0",
        description = "Automated documentary-style narrated video production API",
    )
)]
pub struct ApiDoc;
