//! Write operations: project and session CRUD, and the event log append.
//! Payload shapes follow the original server's REST contract.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::{Value, json};

use crate::db::Db;
use crate::routes::ApiResult;

pub fn now_iso() -> String {
    // The original stores timestamps as ISO-8601 text.
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Minimal civil-time conversion (UTC) so ordering and display stay sane.
    let days = d / 86_400;
    let secs = d % 86_400;
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let (y, mo, dd) = civil_from_days(days as i64);
    format!("{y:04}-{mo:02}-{dd:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant's days-from-civil inverse, for days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn new_id(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{nanos:x}")
}

pub fn bad_request(msg: &str) -> ApiResult {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
}

pub async fn projects_create(State(db): State<Db>, Json(body): Json<Value>) -> ApiResult {
    let name = body.get("name").and_then(Value::as_str).unwrap_or("");
    let workdir = body
        .get("workdir")
        .or_else(|| body.get("path"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if name.is_empty() || workdir.is_empty() {
        return bad_request("name and workdir are required");
    }

    let id = body
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| new_id("prj"));

    // workdir is UNIQUE in the schema; surface the conflict the way the
    // original does rather than failing with a 500.
    if db.exists("SELECT id FROM projects WHERE workdir = ?1", &[&workdir]) {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "a project already exists for this directory" })),
        )
            .into_response();
    }

    let ts = now_iso();
    let res = db.execute(
        "INSERT INTO projects (id, name, workdir, created_at, updated_at) VALUES (?1,?2,?3,?4,?5)",
        &[&id, &name, &workdir, &ts, &ts],
    );
    if res.is_err() {
        return bad_request("failed to create project");
    }

    (
        StatusCode::CREATED,
        Json(json!({ "id": id, "name": name, "workdir": workdir, "created_at": ts })),
    )
        .into_response()
}

pub async fn project_update(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    if !db.exists("SELECT id FROM projects WHERE id = ?1", &[&id]) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "project not found" })),
        )
            .into_response();
    }

    let name = body.get("name").and_then(Value::as_str);
    if let Some(n) = name {
        db.execute(
            "UPDATE projects SET name = ?2, updated_at = ?3 WHERE id = ?1",
            &[&id, &n, &now_iso()],
        )
        .ok();
    }
    if let Some(starred) = body.get("isStarred").and_then(Value::as_bool) {
        db.execute(
            "UPDATE projects SET is_starred = ?2, updated_at = ?3 WHERE id = ?1",
            &[&id, &(starred as i64), &now_iso()],
        )
        .ok();
    }
    if let Some(instructions) = body.get("customInstructions").and_then(Value::as_str) {
        db.execute(
            "UPDATE projects SET custom_instructions = ?2, updated_at = ?3 WHERE id = ?1",
            &[&id, &instructions, &now_iso()],
        )
        .ok();
    }

    (StatusCode::OK, Json(json!({ "success": true, "id": id }))).into_response()
}

pub async fn project_delete(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    let n = db
        .execute("DELETE FROM projects WHERE id = ?1", &[&id])
        .unwrap_or(0);
    if n == 0 {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "project not found" })),
        )
            .into_response();
    }
    (StatusCode::OK, Json(json!({ "success": true }))).into_response()
}

pub async fn sessions_create(State(db): State<Db>, Json(body): Json<Value>) -> ApiResult {
    let project_id = body
        .get("project_id")
        .or_else(|| body.get("projectId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if project_id.is_empty() {
        return bad_request("project_id is required");
    }
    if !db.exists("SELECT id FROM projects WHERE id = ?1", &[&project_id]) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "project not found" })),
        )
            .into_response();
    }

    let id = body
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| new_id("ses"));
    let title = body.get("title").and_then(Value::as_str);
    let workdir: String = db
        .query_map("SELECT workdir FROM projects WHERE id = ?1", |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .and_then(|mut v| {
            if v.is_empty() {
                None
            } else {
                Some(v.remove(0))
            }
        })
        .unwrap_or_default();

    let ts = now_iso();
    let res = db.execute(
        "INSERT INTO sessions (id, project_id, workdir, phase, created_at, updated_at, title)
         VALUES (?1,?2,?3,'idle',?4,?5,?6)",
        &[&id, &project_id, &workdir, &ts, &ts, &title],
    );
    if res.is_err() {
        return bad_request("failed to create session");
    }

    (
        StatusCode::CREATED,
        Json(json!({ "id": id, "project_id": project_id, "title": title })),
    )
        .into_response()
}

pub async fn session_delete(State(db): State<Db>, Path(id): Path<String>) -> ApiResult {
    let n = db
        .execute("DELETE FROM sessions WHERE id = ?1", &[&id])
        .unwrap_or(0);
    if n == 0 {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "session not found" })),
        )
            .into_response();
    }
    (StatusCode::OK, Json(json!({ "success": true }))).into_response()
}

pub async fn session_set_favorite(
    State(db): State<Db>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    if !db.exists("SELECT id FROM sessions WHERE id = ?1", &[&id]) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "session not found" })),
        )
            .into_response();
    }
    let fav = body
        .get("isFavorite")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    db.execute(
        "UPDATE sessions SET is_favorite = ?2 WHERE id = ?1",
        &[&id, &(fav as i64)],
    )
    .ok();
    (
        StatusCode::OK,
        Json(json!({ "success": true, "isFavorite": fav })),
    )
        .into_response()
}

pub async fn settings_set(State(db): State<Db>, Json(body): Json<Value>) -> ApiResult {
    let obj = match body.as_object() {
        Some(o) => o,
        None => return bad_request("expected a JSON object of settings"),
    };
    if obj.is_empty() {
        return bad_request("no settings provided");
    }

    let ts = now_iso();
    for (key, value) in obj {
        let stored = value.to_string();
        if db
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1,?2,?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                &[key, &stored, &ts],
            )
            .is_err()
        {
            return bad_request(&format!("failed to persist setting {key}"));
        }
    }

    (StatusCode::OK, Json(json!({ "success": true }))).into_response()
}

/// Append an event to the session log, honouring the (session_id, seq) uniqueness
/// the original relies on for ordered replay.
pub fn append_event(db: &Db, session_id: &str, event_type: &str, payload: &Value) -> Option<i64> {
    let next_seq: i64 = db
        .query_scalar_i64(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE session_id = ?1",
            &[&session_id],
        )
        .unwrap_or(1);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let body = payload.to_string();
    db.insert_returning_id(
        "INSERT INTO events (session_id, seq, timestamp, event_type, payload)
         VALUES (?1,?2,?3,?4,?5)",
        &[&session_id, &next_seq, &ts, &event_type, &body],
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_is_iso_shaped() {
        let ts = now_iso();
        assert_eq!(ts.len(), 20, "got {ts}");
        assert!(ts.ends_with('Z'), "got {ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn civil_conversion_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }

    #[test]
    fn event_seq_increments_per_session() {
        let db = Db::open(std::path::Path::new(":memory:")).unwrap();
        db.execute(
            "INSERT INTO projects (id,name,workdir,created_at,updated_at)
             VALUES ('p1','p','/tmp/p','t','t')",
            &[],
        )
        .unwrap();
        db.execute(
            "INSERT INTO sessions (id,project_id,workdir,phase,created_at,updated_at)
             VALUES ('s1','p1','/tmp/p','idle','t','t')",
            &[],
        )
        .unwrap();

        assert!(append_event(&db, "s1", "a", &json!({"i": 1})).is_some());
        assert!(append_event(&db, "s1", "b", &json!({"i": 2})).is_some());

        let seqs = db
            .query_map(
                "SELECT seq FROM events WHERE session_id = 's1' ORDER BY seq",
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(seqs, vec![1, 2]);
    }
}
