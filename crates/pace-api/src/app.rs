use std::sync::Arc;

use axum::{
    extract::Request,
    http::{header, HeaderName, HeaderValue, Method},
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, patch, post, put},
    Router,
};
use pace_db::DbPool;
use pace_storage::{S3Config, S3Storage, Storage};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::{config::AppConfig, routes};

/// Shared application state injected into Axum handlers via State extractor.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub config: AppConfig,
    pub storage: Arc<dyn Storage>,
}

impl AppState {
    /// Builds application state with an S3-compatible storage backend configured from `AppConfig`.
    pub fn new(db: DbPool, config: AppConfig) -> Self {
        let storage = Arc::new(S3Storage::new(S3Config {
            endpoint: config.s3_endpoint.clone(),
            bucket: config.s3_bucket.clone(),
            region: config.s3_region.clone(),
            access_key_id: config.s3_access_key.clone(),
            secret_access_key: config.s3_secret_key.clone(),
            path_style: true,
        }));

        Self {
            db,
            config,
            storage,
        }
    }
}

async fn apply_security_headers(req: Request, next: Next) -> Response {
    let is_auth_route = req.uri().path().starts_with("/api/v1/auth/");
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );

    if is_auth_route {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, max-age=0"),
        );
    }

    response
}

pub fn create_app(state: AppState) -> Router {
    let allowed_origins: Vec<String> = state
        .config
        .allowed_origins()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let allow_origin = AllowOrigin::predicate(move |origin: &HeaderValue, _req_parts| {
        let Ok(origin_str) = origin.to_str() else {
            return false;
        };
        if origin_str.starts_with("chrome-extension://") {
            return true;
        }
        if allowed_origins.is_empty() {
            origin_str == "http://localhost:3000"
        } else {
            allowed_origins.iter().any(|allowed| *allowed == origin_str)
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(allow_origin)
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
        .expose_headers([
            header::ETAG,
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
        .layer(cors)
        .layer(middleware::from_fn(apply_security_headers));

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
        )
        .route(
            "/{id}/script",
            get(routes::scripts::get_script_handler).put(routes::scripts::save_script_handler),
        )
        .route(
            "/{id}/uploads/presign",
            post(routes::uploads::presign_upload),
        )
        .route(
            "/{id}/media",
            get(routes::media::list_media).post(routes::media::register_media),
        )
        .route(
            "/{id}/media/stock/search",
            get(routes::media_sources::search_stock_media),
        )
        .route(
            "/{id}/media/stock/import",
            post(routes::media_sources::import_stock_media),
        )
        .route(
            "/{id}/media/stock/auto-match",
            post(routes::media_sources::auto_match_stock_media),
        )
        .route(
            "/{id}/media/ai/generate",
            post(routes::media_sources::generate_ai_media),
        )
        .route(
            "/{id}/media/{media_id}",
            delete(routes::media::delete_media),
        )
        .route(
            "/{id}/assignments",
            get(routes::assignments::list_assignments),
        )
        .route(
            "/{id}/assignments/auto",
            put(routes::assignments::auto_assign),
        )
        .route(
            "/{id}/assignments/swap",
            put(routes::assignments::swap_sections),
        )
        .route(
            "/{id}/assignments/{section_id}",
            put(routes::assignments::assign_section).delete(routes::assignments::unassign_section),
        )
        .route(
            "/{id}/assignments/{section_id}/effect",
            patch(routes::effects::update_scene_effect),
        )
        .route(
            "/{id}/effects/reroll",
            post(routes::effects::reroll_effects),
        )
        .route(
            "/{id}/render-spec",
            get(routes::effects::get_render_spec),
        )
        .route(
            "/{id}/narration/presign",
            post(routes::narration::presign_narration),
        )
        .route(
            "/{id}/narration",
            get(routes::narration::get_narration_state)
                .post(routes::narration::register_narration)
                .delete(routes::narration::delete_narration),
        )
        .route(
            "/{id}/narration/peaks",
            get(routes::narration::get_narration_peaks),
        )
        .route(
            "/{id}/narration/realign",
            post(routes::narration::realign_narration),
        )
        .route(
            "/{id}/timeline",
            get(routes::timeline::get_timeline_state),
        )
        .route(
            "/{id}/boundaries",
            patch(routes::timeline::batch_patch_boundaries),
        )
        .route(
            "/{id}/boundaries/{index}",
            patch(routes::narration::patch_boundary),
        )
        .route(
            "/{id}/music/presign",
            post(routes::music::presign_music),
        )
        .route(
            "/{id}/music",
            get(routes::music::get_music_state)
                .post(routes::music::register_music)
                .patch(routes::music::update_music_settings)
                .delete(routes::music::delete_music),
        )
        .route(
            "/{id}/subtitles",
            get(routes::music::get_subtitles_state),
        )
        .route(
            "/{id}/subtitles/style",
            put(routes::music::update_subtitle_style),
        )
        .route(
            "/{id}/subtitles/merge",
            post(routes::music::merge_subtitle_cues_handler),
        )
        .route(
            "/{id}/subtitles/{cue_id}",
            patch(routes::narration::patch_subtitle_cue),
        )
        .route(
            "/{id}/subtitles/{cue_id}/split",
            post(routes::music::split_subtitle_cue_handler),
        )
        .route(
            "/{id}/jobs/{job_id}",
            get(routes::jobs::get_project_job),
        )
        .route(
            "/{id}/exports",
            post(routes::exports::start_export).get(routes::exports::list_exports),
        )
        .route(
            "/{id}/exports/{export_id}",
            get(routes::exports::get_export),
        )
        .route(
            "/{id}/exports/{export_id}/retry",
            post(routes::exports::retry_export),
        )
        .route(
            "/{id}/exports/{export_id}/events",
            get(routes::exports::stream_export_events),
        );

    // Extension routes (EXT-01..05)
    let extension_routes = Router::new()
        .route("/pairing-code", post(routes::extension::create_pairing_code))
        .route("/pair", post(routes::extension::pair_extension))
        .route("/tokens", get(routes::extension::list_extension_tokens))
        .route(
            "/tokens/{id}",
            delete(routes::extension::revoke_extension_token_handler),
        )
        .route("/projects", get(routes::extension::list_extension_projects))
        .route(
            "/projects/{id}/prompts",
            get(routes::extension::get_extension_project_prompts),
        )
        .route(
            "/projects/{id}/uploads/presign",
            post(routes::extension::presign_extension_upload),
        )
        .route(
            "/projects/{id}/media",
            post(routes::extension::register_extension_media),
        );

    // /api/v1 router
    let api_v1 = Router::new()
        .nest("/auth", auth_routes)
        .nest("/admin", admin_routes)
        .nest("/projects", project_routes)
        .nest("/extension", extension_routes);

    Router::new()
        .route("/healthz", get(routes::health::healthz))
        .route("/readyz", get(routes::health::readyz))
        .nest("/api/v1", api_v1)
        .layer(middleware)
        .with_state(state)
}
