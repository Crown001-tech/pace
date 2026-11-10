use axum::{
    http::{header, HeaderName, HeaderValue, Method},
    routing::{get, patch, post},
    Router,
};
use pace_db::DbPool;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::{config::AppConfig, routes};

/// Shared application state injected into Axum handlers via State extractor.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub config: AppConfig,
}

pub fn create_app(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            state
                .config
                .cors_origin
                .parse::<HeaderValue>()
                .unwrap_or_else(|_| HeaderValue::from_static("http://localhost:3000")),
        )
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::IF_MATCH,
            HeaderName::from_static("x-pace-csrf"),
        ])
        .allow_credentials(true);

    let x_request_id = HeaderName::from_static("x-request-id");

    let middleware = ServiceBuilder::new()
        .layer(SetRequestIdLayer::new(
            x_request_id.clone(),
            MakeRequestUuid,
        ))
        .layer(PropagateRequestIdLayer::new(x_request_id))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(cors);

    // Auth routes
    let auth_routes = Router::new()
        .route("/login", post(routes::auth::login))
        .route("/logout", post(routes::auth::logout))
        .route("/me", get(routes::auth::me))
        .route("/password", post(routes::auth::change_password));

    // Admin routes
    let admin_routes = Router::new()
        .route(
            "/users",
            post(routes::admin::create_new_user).get(routes::admin::get_all_users),
        )
        .route("/users/{id}", patch(routes::admin::update_user_status))
        .route(
            "/users/{id}/reset-password",
            post(routes::admin::admin_reset_password),
        );

    // Project routes
    let project_routes = Router::new()
        .route(
            "/",
            post(routes::projects::create_project_handler)
                .get(routes::projects::list_projects_handler),
        )
        .route(
            "/{id}",
            get(routes::projects::get_project_handler)
                .patch(routes::projects::update_project_handler)
                .delete(routes::projects::delete_project_handler),
        );

    // /api/v1 router
    let api_v1 = Router::new()
        .nest("/auth", auth_routes)
        .nest("/admin", admin_routes)
        .nest("/projects", project_routes);

    Router::new()
        .route("/healthz", get(routes::health::healthz))
        .route("/readyz", get(routes::health::readyz))
        .nest("/api/v1", api_v1)
        .layer(middleware)
        .with_state(state)
}
