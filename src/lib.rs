pub mod db;
pub mod pathsec;
pub mod routes;
pub mod session;
pub mod write;
pub mod ws;

use anyhow::Result;
use axum::{Router, http::Method};
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};

pub struct AppConfig {
    pub database_path: PathBuf,
    pub host: String,
    pub port: u16,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            database_path: std::env::var("OPENFOX_DB_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("rustfox.db")),
            host: std::env::var("OPENFOX_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("OPENFOX_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(10369),
        }
    }
}

/// Build the application router. Every REST path here comes from
/// contract/rest-routes.json, which was extracted from the original server.
pub fn build_app(state: crate::db::Db) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/health", axum::routing::get(routes::health))
        .route("/api/config", axum::routing::get(routes::config))
        .route(
            "/api/settings",
            axum::routing::get(routes::settings_get).post(write::settings_set),
        )
        .route(
            "/api/settings/{key}",
            axum::routing::get(routes::settings_key),
        )
        .route(
            "/api/projects",
            axum::routing::get(routes::projects_list).post(write::projects_create),
        )
        .route(
            "/api/projects/{id}",
            axum::routing::get(routes::project_get)
                .patch(write::project_update)
                .put(session::project_put)
                .delete(write::project_delete),
        )
        .route(
            "/api/projects/{id}/sessions",
            axum::routing::get(routes::project_sessions).delete(session::project_delete_sessions),
        )
        .route(
            "/api/sessions",
            axum::routing::get(routes::sessions_list).post(write::sessions_create),
        )
        .route(
            "/api/sessions/{id}",
            axum::routing::get(routes::session_get).delete(write::session_delete),
        )
        .route(
            "/api/sessions/{id}/favorite",
            axum::routing::put(write::session_set_favorite),
        )
        .route(
            "/api/sessions/{id}/status",
            axum::routing::get(routes::session_status),
        )
        .route(
            "/api/sessions/{id}/stats",
            axum::routing::get(routes::session_stats),
        )
        .route(
            "/api/sessions/{id}/export",
            axum::routing::get(routes::session_export),
        )
        .route(
            "/api/sessions/{id}/background-processes",
            axum::routing::get(routes::session_background_processes),
        )
        .route("/api/providers", axum::routing::get(routes::providers_list))
        // ---- projects (extended) ----
        .route(
            "/api/projects/{id}/branches",
            axum::routing::get(session::project_branches),
        )
        .route(
            "/api/projects/{id}/workspaces",
            axum::routing::get(session::project_workspaces),
        )
        .route(
            "/api/projects/{id}/open-folder",
            axum::routing::get(session::project_open_folder),
        )
        .route(
            "/api/projects/{id}/star",
            axum::routing::put(session::project_star),
        )
        .route(
            "/api/projects/{id}/checkout",
            axum::routing::post(session::project_checkout),
        )
        .route(
            "/api/projects/{id}/checkout-new",
            axum::routing::post(session::project_checkout_new),
        )
        .route(
            "/api/projects/check-permissions",
            axum::routing::post(session::projects_check_permissions),
        )
        .route(
            "/api/projects/fix-permissions",
            axum::routing::post(session::projects_fix_permissions),
        )
        // ---- sessions (extended) ----
        .route(
            "/api/sessions/home",
            axum::routing::get(session::sessions_home),
        )
        .route(
            "/api/sessions/import",
            axum::routing::post(session::sessions_import),
        )
        .route(
            "/api/sessions/{id}/title",
            axum::routing::put(session::session_title),
        )
        .route(
            "/api/sessions/{id}/mode",
            axum::routing::put(session::session_mode),
        )
        .route(
            "/api/sessions/{id}/danger-level",
            axum::routing::put(session::session_danger_level),
        )
        .route(
            "/api/sessions/{id}/night-mode",
            axum::routing::put(session::session_night_mode),
        )
        .route(
            "/api/sessions/{id}/metadata/{key}",
            axum::routing::put(session::session_metadata),
        )
        .route(
            "/api/sessions/{id}/criteria",
            axum::routing::put(session::session_criteria),
        )
        .route(
            "/api/sessions/{id}/review-findings",
            axum::routing::put(session::session_review_findings),
        )
        .route(
            "/api/sessions/{id}/mcp/overrides",
            axum::routing::get(session::session_mcp_overrides)
                .put(session::session_mcp_overrides_put),
        )
        .route(
            "/api/sessions/{id}/provider",
            axum::routing::post(session::session_provider).delete(session::session_provider_delete),
        )
        .route(
            "/api/sessions/{id}/pin-effort",
            axum::routing::post(session::session_pin_effort)
                .delete(session::session_pin_effort_delete),
        )
        .route(
            "/api/sessions/{id}/branches",
            axum::routing::get(session::session_branches),
        )
        .route(
            "/api/sessions/{id}/checkout",
            axum::routing::post(session::session_checkout),
        )
        .route(
            "/api/sessions/{id}/checkout-new",
            axum::routing::post(session::session_checkout_new),
        )
        .route(
            "/api/sessions/{id}/switch-workspace",
            axum::routing::post(session::session_switch_workspace),
        )
        .route(
            "/api/sessions/{id}/delete-workspace",
            axum::routing::post(session::session_delete_workspace),
        )
        .route(
            "/api/sessions/{id}/message",
            axum::routing::post(session::session_message),
        )
        .route(
            "/api/sessions/{id}/answer",
            axum::routing::post(session::session_answer),
        )
        .route(
            "/api/sessions/{id}/confirm-path",
            axum::routing::post(session::session_confirm_path),
        )
        .route(
            "/api/sessions/{id}/fork",
            axum::routing::post(session::session_fork),
        )
        .route(
            "/api/sessions/{id}/replay",
            axum::routing::post(session::session_replay),
        )
        .route(
            "/api/sessions/{id}/truncate",
            axum::routing::post(session::session_truncate),
        )
        .route(
            "/api/sessions/{id}/queue/{queueId}",
            axum::routing::delete(session::session_queue_delete),
        )
        .route(
            "/api/sessions/{id}/stop",
            axum::routing::post(session::session_stop),
        )
        .route(
            "/api/sessions/{id}/pause",
            axum::routing::post(session::session_pause),
        )
        .route(
            "/api/sessions/{id}/resume",
            axum::routing::post(session::session_resume),
        )
        .route(
            "/api/sessions/{id}/warmup",
            axum::routing::post(session::session_warmup),
        )
        .route(
            "/api/sessions/{id}/background-process/{processId}/stop",
            axum::routing::post(session::session_bg_stop),
        )
        .route(
            "/api/sessions/{id}/chat",
            axum::routing::post(session::session_turn),
        )
        .route(
            "/api/sessions/{id}/continue",
            axum::routing::post(session::session_turn),
        )
        .route("/api/tools", axum::routing::get(routes::tools_list))
        .route("/api/plugins", axum::routing::get(routes::plugins_list))
        .route("/api/mcp/servers", axum::routing::get(routes::mcp_servers))
        .route(
            "/api/provider-presets",
            axum::routing::get(routes::provider_presets),
        )
        .route(
            "/api/provider-adapters",
            axum::routing::get(routes::provider_adapters),
        )
        .route("/api/changelog", axum::routing::get(routes::changelog))
        .route("/api/auth", axum::routing::get(routes::auth))
        .route("/ws", axum::routing::get(ws::handler))
        .with_state(state);

    app.layer(cors)
}

pub async fn run(config: AppConfig) -> Result<()> {
    let db = crate::db::Db::open(&config.database_path)?;
    let app = build_app(db);

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "rustfox listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown signal received");
        })
        .await?;

    Ok(())
}
