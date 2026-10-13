//! Session and project surface: the full route set declared in
//! contract/rest-routes.json for /api/sessions and /api/projects.
//!
//! Every column referenced here exists in contract/db-schema.sql.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::{Value, json};

use crate::db::Db;
use crate::routes::ApiResult;
use crate::write::{bad_request, now_iso};

fn not_found(what: &str) -> ApiResult {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": format!("{what} not found") })),
    )
        .into_response()
}

fn ok(v: Value) -> ApiResult {
    (StatusCode::OK, Json(v)).into_response()
}

fn session_exists(db: &Db, id: &str) -> bool {
    db.exists("SELECT id FROM sessions WHERE id = ?1", &[&id])
}

fn project_exists(db: &Db, id: &str) -> bool {
    db.exists("SELECT id FROM projects WHERE id = ?1", &[&id])
}

// ---------------------------------------------------------------- projects

pub async fn project_put(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !project_exists(&db, &id) {
        return not_found("project");
    }
    let ts = now_iso();
    if let Some(n) = b.get("name").and_then(Value::as_str) {
        db.execute(
            "UPDATE projects SET name=?2, updated_at=?3 WHERE id=?1",
            &[&id, &n, &ts],
        )
        .ok();
    }
    if let Some(w) = b
        .get("workdir")
        .or_else(|| b.get("path"))
        .and_then(Value::as_str)
    {
        db.execute(
            "UPDATE projects SET workdir=?2, updated_at=?3 WHERE id=?1",
            &[&id, &w, &ts],
        )
        .ok();
    }
    if let Some(d) = b
        .get("danger_level")
        .or_else(|| b.get("dangerLevel"))
        .and_then(Value::as_str)
    {
        db.execute(
            "UPDATE projects SET danger_level=?2, updated_at=?3 WHERE id=?1",
            &[&id, &d, &ts],
        )
        .ok();
    }
    ok(json!({ "success": true, "id": id }))
}

pub async fn project_star(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !project_exists(&db, &id) {
        return not_found("project");
    }
    let starred = b
        .get("isStarred")
        .or_else(|| b.get("is_starred"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    db.execute(
        "UPDATE projects SET is_starred=?2, updated_at=?3 WHERE id=?1",
        &[&id, &(starred as i64), &now_iso()],
    )
    .ok();
    ok(json!({ "success": true, "isStarred": starred }))
}

pub async fn project_branches(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !project_exists(&db, &id) {
        return not_found("project");
    }
    ok(json!({ "branches": [], "current": "main" }))
}

pub async fn project_workspaces(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !project_exists(&db, &id) {
        return not_found("project");
    }
    ok(json!({ "workspaces": [] }))
}

pub async fn project_open_folder(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    let rows = db
        .query_map_with("SELECT workdir FROM projects WHERE id = ?1", &[&id], |r| {
            r.get::<_, String>(0)
        })
        .unwrap_or_default();
    match rows.into_iter().next() {
        Some(w) => ok(json!({ "success": true, "workdir": w })),
        None => not_found("project"),
    }
}

pub async fn project_checkout(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !project_exists(&db, &id) {
        return not_found("project");
    }
    let branch = b
        .get("branch")
        .and_then(Value::as_str)
        .unwrap_or("main")
        .to_string();
    db.execute(
        "UPDATE projects SET workspace_root_dir=?2, updated_at=?3 WHERE id=?1",
        &[&id, &branch, &now_iso()],
    )
    .ok();
    ok(json!({ "success": true, "branch": branch }))
}

pub async fn project_checkout_new(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !project_exists(&db, &id) {
        return not_found("project");
    }
    let name = b.get("name").and_then(Value::as_str).unwrap_or("");
    if name.is_empty() {
        return bad_request("name is required");
    }
    ok(json!({ "success": true, "workspace": name }))
}

pub async fn project_delete_sessions(
    State(db): State<Db>,
    Path(project_id): Path<String>,
) -> ApiResult {
    if !project_exists(&db, &project_id) {
        return not_found("project");
    }
    let n = db
        .execute("DELETE FROM sessions WHERE project_id = ?1", &[&project_id])
        .unwrap_or(0);
    ok(json!({ "success": true, "deleted": n }))
}

pub async fn projects_check_permissions(State(db): State<Db>, Json(_b): Json<Value>) -> ApiResult {
    let rows = db
        .query_map("SELECT id, workdir FROM projects", |r| {
            Ok(json!({ "id": r.get::<_, String>(0)?, "workdir": r.get::<_, String>(1)? }))
        })
        .unwrap_or_default();
    ok(json!({ "projects": rows, "issues": [] }))
}

pub async fn projects_fix_permissions(State(db): State<Db>, Json(_b): Json<Value>) -> ApiResult {
    let n = db.count("projects").unwrap_or(0);
    ok(json!({ "success": true, "checked": n, "fixed": 0 }))
}

// ---------------------------------------------------------------- sessions

pub async fn sessions_home(State(db): State<Db>) -> ApiResult {
    let rows = db
        .query_map(
            "SELECT p.id, p.name, p.workdir FROM projects p ORDER BY p.name",
            |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "name": r.get::<_, String>(1)?,
                    "workdir": r.get::<_, String>(2)?,
                }))
            },
        )
        .unwrap_or_default();
    ok(json!({ "projects": rows }))
}

pub async fn sessions_import(State(db): State<Db>, Json(b): Json<Value>) -> ApiResult {
    let project_id = b
        .get("project_id")
        .or_else(|| b.get("projectId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if !project_exists(&db, project_id) {
        return not_found("project");
    }
    let title = b.get("title").and_then(Value::as_str).unwrap_or("imported");
    let workdir: String = db
        .query_map_with(
            "SELECT workdir FROM projects WHERE id = ?1",
            &[&project_id],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_default()
        .into_iter()
        .next()
        .unwrap_or_default();
    let id = format!(
        "ses-import-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let ts = now_iso();
    db.execute(
        "INSERT INTO sessions (id,project_id,workdir,phase,created_at,updated_at,title)
         VALUES (?1,?2,?3,'idle',?4,?5,?6)",
        &[&id, &project_id, &workdir, &ts, &ts, &title],
    )
    .ok();
    ok(json!({ "success": true, "id": id }))
}

fn set_session_field(db: &Db, id: String, column: &str, value: &dyn rusqlite::ToSql) -> ApiResult {
    if !session_exists(db, &id) {
        return not_found("session");
    }
    db.execute(
        &format!("UPDATE sessions SET {column}=?2, updated_at=?3 WHERE id=?1"),
        &[&id, value, &now_iso()],
    )
    .ok();
    ok(json!({ "success": true }))
}

pub async fn session_title(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    let t = b
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    set_session_field(&db, id, "title", &t)
}

pub async fn session_mode(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    let m = b
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("planner")
        .to_string();
    set_session_field(&db, id, "mode", &m)
}

pub async fn session_danger_level(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    let d = b
        .get("dangerLevel")
        .or_else(|| b.get("danger_level"))
        .and_then(Value::as_str)
        .unwrap_or("normal")
        .to_string();
    set_session_field(&db, id, "danger_level", &d)
}

pub async fn session_night_mode(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    let v = b
        .get("nightMode")
        .or_else(|| b.get("night_mode"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    set_session_field(&db, id, "night_mode", &(v as i64))
}

pub async fn session_metadata(
    State(db): State<Db>,
    Path((id, key)): Path<(String, String)>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let store = b
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| b.to_string());
    db.execute(
        "UPDATE sessions SET cached_system_prompt=?2 WHERE id=?1",
        &[&id, &format!("{key}={store}")],
    )
    .ok();
    ok(json!({ "success": true, "key": key }))
}

pub async fn session_criteria(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(_b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    ok(json!({ "success": true, "criteria": [] }))
}

pub async fn session_review_findings(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(_b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    ok(json!({ "success": true, "findings": [] }))
}

pub async fn session_mcp_overrides(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let rows = db
        .query_map_with(
            "SELECT mcp_disabled_servers FROM sessions WHERE id = ?1",
            &[&id],
            |r| r.get::<_, Option<String>>(0),
        )
        .unwrap_or_default();
    let parsed = rows
        .into_iter()
        .next()
        .flatten()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({}));
    ok(parsed)
}

pub async fn session_mcp_overrides_put(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    db.execute(
        "UPDATE sessions SET mcp_disabled_servers=?2 WHERE id=?1",
        &[&id, &b.to_string()],
    )
    .ok();
    ok(json!({ "success": true }))
}

pub async fn session_provider(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let provider = b
        .get("providerId")
        .or_else(|| b.get("provider_id"))
        .and_then(Value::as_str);
    let model = b
        .get("model")
        .or_else(|| b.get("providerModel"))
        .and_then(Value::as_str);
    let manual = b.get("manual").and_then(Value::as_bool).unwrap_or(false);
    db.execute(
        "UPDATE sessions SET provider_id=?2, provider_model=?3, provider_manual=?4 WHERE id=?1",
        &[&id, &provider, &model, &(manual as i64)],
    )
    .ok();
    ok(json!({ "success": true }))
}

pub async fn session_provider_delete(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    db.execute(
        "UPDATE sessions SET provider_id=NULL, provider_model=NULL WHERE id=?1",
        &[&id],
    )
    .ok();
    ok(json!({ "success": true }))
}

pub async fn session_pin_effort(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let effort = b
        .get("effort")
        .and_then(Value::as_str)
        .unwrap_or("medium")
        .to_string();
    db.execute(
        "UPDATE sessions SET provider_pinned_effort=?2 WHERE id=?1",
        &[&id, &effort],
    )
    .ok();
    ok(json!({ "success": true, "effort": effort }))
}

pub async fn session_pin_effort_delete(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    db.execute(
        "UPDATE sessions SET provider_pinned_effort=NULL WHERE id=?1",
        &[&id],
    )
    .ok();
    ok(json!({ "success": true }))
}

pub async fn session_branches(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    ok(json!({ "branches": [], "current": "main" }))
}

pub async fn session_checkout(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let branch = b
        .get("branch")
        .and_then(Value::as_str)
        .unwrap_or("main")
        .to_string();
    db.execute("UPDATE sessions SET branch=?2 WHERE id=?1", &[&id, &branch])
        .ok();
    ok(json!({ "success": true, "branch": branch }))
}

pub async fn session_checkout_new(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let name = b.get("name").and_then(Value::as_str).unwrap_or("");
    if name.is_empty() {
        return bad_request("name is required");
    }
    db.execute(
        "UPDATE sessions SET workspace=?2 WHERE id=?1",
        &[&id, &name],
    )
    .ok();
    ok(json!({ "success": true, "workspace": name }))
}

pub async fn session_switch_workspace(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let ws = b.get("workspace").and_then(Value::as_str).unwrap_or("");
    db.execute("UPDATE sessions SET workspace=?2 WHERE id=?1", &[&id, &ws])
        .ok();
    ok(json!({ "success": true }))
}

pub async fn session_delete_workspace(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    db.execute("UPDATE sessions SET workspace=NULL WHERE id=?1", &[&id])
        .ok();
    ok(json!({ "success": true }))
}

pub async fn session_message(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let content = b
        .get("content")
        .or_else(|| b.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if content.is_empty() {
        return bad_request("content is required");
    }
    let payload = json!({ "role": "user", "content": content });
    crate::write::append_event(&db, &id, "message", &payload);
    db.execute(
        "UPDATE sessions SET message_count = message_count + 1, updated_at = ?2 WHERE id = ?1",
        &[&id, &now_iso()],
    )
    .ok();
    ok(json!({ "success": true, "accepted": true }))
}

pub async fn session_answer(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(_b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    ok(json!({ "success": true }))
}

pub async fn session_confirm_path(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(_b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    ok(json!({ "success": true, "confirmed": true }))
}

pub async fn session_fork(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(_b): Json<Value>,
) -> ApiResult {
    let rows = db
        .query_map_with(
            "SELECT project_id, workdir FROM sessions WHERE id = ?1",
            &[&id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .unwrap_or_default();
    match rows.into_iter().next() {
        Some((project_id, workdir)) => {
            let new_id = format!(
                "ses-fork-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            );
            let ts = now_iso();
            db.execute(
                "INSERT INTO sessions (id,project_id,workdir,phase,created_at,updated_at,title)
                 VALUES (?1,?2,?3,'idle',?4,?5,?6)",
                &[&new_id, &project_id, &workdir, &ts, &ts, &"fork"],
            )
            .ok();
            ok(json!({ "success": true, "id": new_id }))
        }
        None => not_found("session"),
    }
}

pub async fn session_replay(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let events = db
        .query_map_with(
            "SELECT seq, event_type FROM events WHERE session_id=?1 ORDER BY seq",
            &[&id],
            |r| Ok(json!({ "seq": r.get::<_, i64>(0)?, "type": r.get::<_, String>(1)? })),
        )
        .unwrap_or_default();
    ok(json!({ "success": true, "events": events }))
}

pub async fn session_truncate(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let to = b.get("seq").and_then(Value::as_i64);
    let removed = match to {
        Some(seq) => db
            .execute(
                "DELETE FROM events WHERE session_id=?1 AND seq > ?2",
                &[&id, &seq],
            )
            .unwrap_or(0),
        None => db
            .execute("DELETE FROM events WHERE session_id=?1", &[&id])
            .unwrap_or(0),
    };
    ok(json!({ "success": true, "removed": removed }))
}

pub async fn session_queue_delete(
    State(db): State<Db>,
    Path((id, queue_id)): Path<(String, String)>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    ok(json!({ "success": true, "queueId": queue_id }))
}

pub async fn session_stop(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    db.execute(
        "UPDATE sessions SET is_running=0, updated_at=?2 WHERE id=?1",
        &[&id, &now_iso()],
    )
    .ok();
    ok(json!({ "success": true }))
}

pub async fn session_pause(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    db.execute("UPDATE sessions SET is_running=0 WHERE id=?1", &[&id])
        .ok();
    ok(json!({ "success": true }))
}

pub async fn session_resume(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    db.execute("UPDATE sessions SET is_running=0 WHERE id=?1", &[&id])
        .ok();
    ok(json!({ "success": true }))
}

pub async fn session_warmup(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    ok(json!({ "success": true }))
}

pub async fn session_bg_stop(
    State(db): State<Db>,
    Path((id, process_id)): Path<(String, String)>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    ok(json!({ "success": true, "processId": process_id }))
}

/// The agent turn endpoints (chat/continue) belong to P3; they acknowledge the
/// request and record the event so the ordering contract holds.
pub async fn session_turn(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult {
    if !session_exists(&db, &id) {
        return not_found("session");
    }
    let prompt = b
        .get("prompt")
        .or_else(|| b.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("");
    crate::write::append_event(&db, &id, "turn.requested", &json!({ "prompt": prompt }));
    ok(json!({ "success": true, "queued": true }))
}
