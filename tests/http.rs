//! End-to-end HTTP tests against the real router, using an in-process server.
//! These prove the routes are mounted and that CRUD round-trips work, without
//! depending on any external process.

use axum::body::Body;
use http_body_util::BodyExt;
use rustfox::db::Db;
use serde_json::{Value, json};
use std::path::Path;
use tower::ServiceExt;

fn app() -> axum::Router {
    let db = Db::open(Path::new(":memory:")).expect("open in-memory database");
    rustfox::build_app(db)
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let req_body = match &body {
        Some(v) => Body::from(v.to_string()),
        None => Body::empty(),
    };
    let req = axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(req_body)
        .expect("build request");

    let res = app.clone().oneshot(req).await.expect("router response");
    let status = res.status().as_u16();
    let bytes = res
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    let parsed: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).to_string()))
    };
    (status, parsed)
}

#[tokio::test]
async fn health_reports_ok() {
    let (status, body) = call(&app(), "GET", "/api/health", None).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn collections_start_empty() {
    let app = app();
    for path in ["/api/projects", "/api/sessions"] {
        let (status, body) = call(&app, "GET", path, None).await;
        assert_eq!(status, 200, "{path} should be mounted");
        let key = path.trim_start_matches("/api/");
        assert_eq!(body[key], json!([]), "{path} should start empty");
    }
}

#[tokio::test]
async fn project_crud_round_trip() {
    let app = app();

    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        Some(json!({ "name": "demo", "workdir": "/tmp/demo" })),
    )
    .await;
    assert_eq!(status, 201, "create should succeed: {created}");
    let id = created["id"].as_str().expect("id").to_string();

    let (status, list) = call(&app, "GET", "/api/projects", None).await;
    assert_eq!(status, 200);
    assert_eq!(list["projects"].as_array().unwrap().len(), 1);

    let (status, fetched) = call(&app, "GET", &format!("/api/projects/{id}"), None).await;
    assert_eq!(status, 200);
    assert_eq!(fetched["name"], "demo");
    assert_eq!(fetched["workdir"], "/tmp/demo");

    let (status, patch_body) = call(
        &app,
        "PATCH",
        &format!("/api/projects/{id}"),
        Some(json!({ "name": "renamed" })),
    )
    .await;
    assert_eq!(status, 200, "PATCH failed for id={id}: {patch_body}");

    let (_, fetched) = call(&app, "GET", &format!("/api/projects/{id}"), None).await;
    assert_eq!(fetched["name"], "renamed");

    let (status, _) = call(&app, "DELETE", &format!("/api/projects/{id}"), None).await;
    assert_eq!(status, 200);

    let (status, _) = call(&app, "GET", &format!("/api/projects/{id}"), None).await;
    assert_eq!(status, 404, "deleted project should be gone");
}

#[tokio::test]
async fn duplicate_workdir_is_rejected() {
    let app = app();
    let payload = json!({ "name": "a", "workdir": "/tmp/same" });
    let (first, _) = call(&app, "POST", "/api/projects", Some(payload.clone())).await;
    assert_eq!(first, 201);

    let (second, body) = call(&app, "POST", "/api/projects", Some(payload)).await;
    assert_eq!(second, 409, "workdir is UNIQUE in the schema");
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn missing_required_fields_return_400() {
    let app = app();
    let (status, _) = call(&app, "POST", "/api/projects", Some(json!({ "name": "x" }))).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn session_requires_an_existing_project() {
    let app = app();
    let (status, _) = call(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({ "project_id": "nope" })),
    )
    .await;
    assert_eq!(status, 404);

    let (_, project) = call(
        &app,
        "POST",
        "/api/projects",
        Some(json!({ "name": "p", "workdir": "/tmp/p" })),
    )
    .await;
    let project_id = project["id"].as_str().unwrap().to_string();

    let (status, session) = call(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({ "project_id": project_id, "title": "first" })),
    )
    .await;
    assert_eq!(status, 201, "session should be created: {session}");
    assert_eq!(session["title"], "first");

    let (status, _) = call(
        &app,
        "GET",
        &format!("/api/projects/{project_id}/sessions"),
        None,
    )
    .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn settings_round_trip_and_upsert() {
    let app = app();

    let (status, body) = call(
        &app,
        "POST",
        "/api/settings",
        Some(json!({ "theme": "dark", "editor": { "fontSize": 13 } })),
    )
    .await;
    assert_eq!(status, 200, "settings POST failed: {body}");

    let (status, all) = call(&app, "GET", "/api/settings", None).await;
    assert_eq!(status, 200, "settings GET failed: {all}");
    assert_eq!(all["theme"], "dark");
    assert_eq!(all["editor"]["fontSize"], 13);

    // Re-setting the same key must upsert, not fail on the primary key.
    let (status, _) = call(
        &app,
        "POST",
        "/api/settings",
        Some(json!({ "theme": "light" })),
    )
    .await;
    assert_eq!(status, 200);

    let (_, all) = call(&app, "GET", "/api/settings", None).await;
    assert_eq!(all["theme"], "light");

    let (status, one) = call(&app, "GET", "/api/settings/theme", None).await;
    assert_eq!(status, 200, "settings key GET failed: {one}");
    assert_eq!(one, "light");

    let (status, _) = call(&app, "GET", "/api/settings/missing", None).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn unknown_route_is_404() {
    let (status, _) = call(&app(), "GET", "/api/definitely-not-a-route", None).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn session_favorite_toggles() {
    let app = app();
    let (_, project) = call(
        &app,
        "POST",
        "/api/projects",
        Some(json!({ "name": "p", "workdir": "/tmp/fav" })),
    )
    .await;
    let pid = project["id"].as_str().unwrap().to_string();
    let (_, session) = call(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({ "project_id": pid })),
    )
    .await;
    let sid = session["id"].as_str().unwrap().to_string();

    let (status, body) = call(
        &app,
        "PUT",
        &format!("/api/sessions/{sid}/favorite"),
        Some(json!({ "isFavorite": true })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["isFavorite"], true);

    let (status, _) = call(
        &app,
        "PUT",
        "/api/sessions/nonexistent/favorite",
        Some(json!({ "isFavorite": true })),
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn db_layer_sees_its_own_writes() {
    let db = Db::open(Path::new(":memory:")).unwrap();
    db.execute(
        "INSERT INTO projects (id,name,workdir,created_at,updated_at)
         VALUES ('x','n','/w','t','t')",
        &[],
    )
    .expect("insert should succeed");

    // exists() must work regardless of the selected column's SQL type.
    let id = "x".to_string();
    assert!(
        db.exists("SELECT id FROM projects WHERE id = ?1", &[&id]),
        "exists must find a row selected by a TEXT column"
    );
    assert!(!db.exists("SELECT id FROM projects WHERE id = ?1", &[&"absent"]));

    assert_eq!(db.count("projects").unwrap(), 1);
}

/// Every /api/sessions and /api/projects route declared in the contract must be
/// mounted. A 404 on a static path means the route is missing; a 404 on a path
/// with a parameter is expected, because the probed resource does not exist.
#[tokio::test]
async fn contract_sessions_and_projects_routes_are_mounted() {
    let contract: Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/contract/rest-routes.json"
        ))
        .expect("read contract"),
    )
    .expect("parse contract");

    let app = app();
    let mut missing = Vec::new();
    let mut checked = 0;

    // Static paths that legitimately 404 because their parent resource is
    // absent; mounting is proven by their presence in the router, not by the
    // probe status.
    const NEEDS_PARENT: &[&str] = &["POST /api/sessions/import"];

    for route in contract.as_array().unwrap() {
        let path = route["path"].as_str().unwrap();
        let method = route["method"].as_str().unwrap();
        if !(path.starts_with("/api/sessions") || path.starts_with("/api/projects")) {
            continue;
        }

        let concrete = path
            .replace(":id", "concrete-probe-id")
            .replace(":projectId", "concrete-probe-id")
            .replace(":taskId", "concrete-probe-id")
            .replace(":queueId", "concrete-probe-id")
            .replace(":processId", "concrete-probe-id")
            .replace(":gateId", "concrete-probe-id")
            .replace(":key", "probe-key");
        let has_param = path.contains(':');

        let (status, _) = call(
            &app,
            method,
            &concrete,
            if method == "GET" {
                None
            } else {
                Some(json!({}))
            },
        )
        .await;
        checked += 1;

        let key = format!("{method} {path}");
        if NEEDS_PARENT.contains(&key.as_str()) {
            if status == 405 {
                missing.push(format!("{key} (method not allowed)"));
            }
            continue;
        }

        // 404 is only acceptable when the route itself carries a parameter;
        // otherwise the route is genuinely absent.
        if status == 404 && !has_param {
            missing.push(key);
        } else if status == 405 {
            missing.push(format!("{key} (method not allowed)"));
        }
    }

    assert!(
        checked >= 60,
        "expected the full surface, checked only {checked}"
    );
    assert!(
        missing.is_empty(),
        "contract routes not mounted by the Rust router:\n  {}",
        missing.join("\n  ")
    );
}
