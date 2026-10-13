//! REST handlers. Paths mirror contract/rest-routes.json; payloads follow the
//! shapes observed on the original server.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::{Value, json};

use crate::db::Db;

pub type ApiResult = axum::response::Response;

fn ok(v: Value) -> ApiResult {
    (StatusCode::OK, Json(v)).into_response()
}

fn not_found(what: &str) -> ApiResult {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": format!("{what} not found") })),
    )
        .into_response()
}

pub async fn health() -> ApiResult {
    ok(json!({
        "status": "ok",
        "timestamp": chrono_like_timestamp(),
    }))
}

fn chrono_like_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{now}")
}

pub async fn config() -> ApiResult {
    ok(json!({ "ok": true }))
}

pub async fn auth() -> ApiResult {
    ok(json!({ "authenticated": false, "strategy": null }))
}

pub async fn settings_get(State(db): State<Db>) -> ApiResult {
    let rows = db
        .query_map("SELECT key, value FROM settings ORDER BY key", |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap_or_default();
    let map: serde_json::Map<String, Value> = rows
        .into_iter()
        .map(|(k, v)| {
            let parsed: Value = serde_json::from_str(&v).unwrap_or(Value::String(v));
            (k, parsed)
        })
        .collect();
    ok(Value::Object(map))
}

pub async fn settings_key(State(db): State<Db>, Path(key): Path<String>) -> ApiResult {
    let rows = db
        .query_map_with("SELECT value FROM settings WHERE key = ?1", &[&key], |r| {
            r.get::<_, String>(0)
        })
        .unwrap_or_default();
    match rows.into_iter().next() {
        Some(v) => {
            let parsed: Value = serde_json::from_str(&v).unwrap_or(Value::String(v));
            ok(parsed)
        }
        None => not_found("setting"),
    }
}

pub async fn projects_list(State(db): State<Db>) -> ApiResult {
    let rows = db
        .query_map(
            "SELECT id, name, workdir, created_at, is_starred FROM projects ORDER BY name",
            |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "name": r.get::<_, String>(1)?,
                    "workdir": r.get::<_, String>(2)?,
                    "created_at": r.get::<_, String>(3)?,
                    "isStarred": r.get::<_, i64>(4)? == 1,
                }))
            },
        )
        .unwrap_or_default();
    ok(json!({ "projects": rows }))
}

pub async fn project_get(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    let rows = db
        .query_map_with(
            "SELECT id, name, workdir, created_at, is_starred FROM projects WHERE id = ?1",
            &[&id],
            |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "name": r.get::<_, String>(1)?,
                    "workdir": r.get::<_, String>(2)?,
                    "created_at": r.get::<_, String>(3)?,
                    "isStarred": r.get::<_, i64>(4)? == 1,
                }))
            },
        )
        .unwrap_or_default();
    match rows.into_iter().next() {
        Some(p) => ok(p),
        None => not_found("project"),
    }
}

pub async fn project_sessions(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    let exists: Vec<String> = db
        .query_map_with("SELECT id FROM projects WHERE id = ?1", &[&id], |r| {
            r.get(0)
        })
        .unwrap_or_default();
    if exists.is_empty() {
        return not_found("project");
    }
    let rows = db
        .query_map_with(
            "SELECT id, title, phase, is_running FROM sessions
             WHERE project_id = ?1 ORDER BY created_at DESC",
            &[&id],
            |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "title": r.get::<_, Option<String>>(1)?,
                    "phase": r.get::<_, String>(2)?,
                    "isRunning": r.get::<_, i64>(3)? == 1,
                }))
            },
        )
        .unwrap_or_default();
    ok(json!({ "sessions": rows }))
}

pub async fn sessions_list(State(db): State<Db>) -> ApiResult {
    let rows = db
        .query_map(
            "SELECT id, project_id, title, phase, is_running FROM sessions
             ORDER BY created_at DESC",
            |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "project_id": r.get::<_, String>(1)?,
                    "title": r.get::<_, Option<String>>(2)?,
                    "phase": r.get::<_, String>(3)?,
                    "isRunning": r.get::<_, i64>(4)? == 1,
                }))
            },
        )
        .unwrap_or_default();
    ok(json!({ "sessions": rows }))
}

pub async fn session_get(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    let rows = db
        .query_map_with(
            "SELECT id, project_id, title, phase, is_running, message_count
             FROM sessions WHERE id = ?1",
            &[&id],
            |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "project_id": r.get::<_, String>(1)?,
                    "title": r.get::<_, Option<String>>(2)?,
                    "phase": r.get::<_, String>(3)?,
                    "isRunning": r.get::<_, i64>(4)? == 1,
                    "messageCount": r.get::<_, i64>(5)?,
                }))
            },
        )
        .unwrap_or_default();
    match rows.into_iter().next() {
        Some(s) => ok(s),
        None => not_found("session"),
    }
}

pub async fn session_status(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    let exists: Vec<i64> = db
        .query_map_with(
            "SELECT is_running FROM sessions WHERE id = ?1",
            &[&id],
            |r| r.get(0),
        )
        .unwrap_or_default();
    match exists.into_iter().next() {
        Some(running) => ok(json!({ "id": id, "isRunning": running == 1 })),
        None => not_found("session"),
    }
}

pub async fn session_stats(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    let rows: Vec<i64> = db
        .query_map_with(
            "SELECT message_count FROM sessions WHERE id = ?1",
            &[&id],
            |r| r.get(0),
        )
        .unwrap_or_default();
    match rows.into_iter().next() {
        Some(count) => ok(json!({ "sessionId": id, "messageCount": count })),
        None => not_found("session"),
    }
}

pub async fn session_export(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    let exists: Vec<String> = db
        .query_map_with("SELECT id FROM sessions WHERE id = ?1", &[&id], |r| {
            r.get(0)
        })
        .unwrap_or_default();
    if exists.is_empty() {
        return not_found("session");
    }
    let events = db
        .query_map_with(
            "SELECT seq, timestamp, event_type, payload FROM events
             WHERE session_id = ?1 ORDER BY seq",
            &[&id],
            |r| {
                Ok(json!({
                    "seq": r.get::<_, i64>(0)?,
                    "timestamp": r.get::<_, i64>(1)?,
                    "type": r.get::<_, String>(2)?,
                    "payload": serde_json::from_str::<Value>(&r.get::<_, String>(3)?)
                        .unwrap_or(Value::Null),
                }))
            },
        )
        .unwrap_or_default();
    ok(json!({ "sessionId": id, "events": events }))
}

pub async fn session_background_processes(
    State(db): State<Db>,
    Path(id): Path<String>,
) -> ApiResult {
    let exists: Vec<String> = db
        .query_map_with("SELECT id FROM sessions WHERE id = ?1", &[&id], |r| {
            r.get(0)
        })
        .unwrap_or_default();
    if exists.is_empty() {
        return not_found("session");
    }
    ok(json!({ "processes": [] }))
}

pub async fn providers_list() -> ApiResult {
    ok(json!({ "providers": [] }))
}

pub async fn provider_presets() -> ApiResult {
    ok(json!({ "presets": [] }))
}

pub async fn provider_adapters() -> ApiResult {
    ok(json!({ "authAdapters": [], "transportAdapters": [] }))
}

pub async fn tools_list() -> ApiResult {
    ok(json!({ "tools": [] }))
}

pub async fn plugins_list() -> ApiResult {
    ok(json!({ "plugins": [] }))
}

pub async fn mcp_servers() -> ApiResult {
    ok(json!({ "servers": [] }))
}

pub async fn changelog() -> ApiResult {
    ok(json!({ "entries": [] }))
}
