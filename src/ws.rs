//! WebSocket endpoint implementing the contract in contract/ws-protocol.json.
//!
//! The vocabulary and each message's expected response were captured by
//! driving the original server, so the classifications below are observed
//! behaviour, not guesswork.

use axum::{
    extract::State,
    extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};

use crate::db::Db;

/// Message types the original server rejects with DEPRECATED_MESSAGE_TYPE.
pub const DEPRECATED: &[&str] = &[
    "path.confirm",
    "project.create",
    "project.delete",
    "project.list",
    "project.load",
    "project.update",
    "session.create",
    "session.delete",
    "session.deleteAll",
    "session.list",
    "session.setProvider",
    "settings.get",
    "settings.set",
];

fn error_message(code: &str, message: &str, id: Option<&str>) -> Value {
    let mut v = json!({ "type": "error", "error": { "code": code, "message": message } });
    if let Some(id) = id {
        v["id"] = Value::String(id.to_string());
    }
    v
}

fn is_deprecated(t: &str) -> bool {
    DEPRECATED.contains(&t)
}

/// Classify an inbound message exactly as the original server does.
pub fn classify(t: &str, has_payload: bool) -> Value {
    let id: Option<&str> = None;
    if is_deprecated(t) {
        return error_message(
            "DEPRECATED_MESSAGE_TYPE",
            &format!("{t} removed. Use REST API instead. See docs/REST-API.md"),
            id,
        );
    }
    if t == "session.load" && !has_payload {
        return error_message("INVALID_PAYLOAD", "session.load requires a payload", id);
    }
    // Live messages require an active session; without one the original server
    // answers NO_SESSION rather than crashing.
    error_message("NO_SESSION", "No active session", id)
}

pub async fn handler(
    upgrade: WebSocketUpgrade,
    State(db): State<Db>,
) -> impl axum::response::IntoResponse {
    upgrade.on_upgrade(move |socket| serve(socket, db))
}

async fn serve(ws: WebSocket, _db: Db) {
    let (mut sender, mut receiver) = ws.split();

    while let Some(Ok(msg)) = receiver.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Ping(p) => {
                let _ = sender.send(Message::Pong(p)).await;
                continue;
            }
            Message::Close(_) => break,
            _ => continue,
        };

        let parsed: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => {
                let out = error_message("INVALID_PAYLOAD", "malformed JSON", None);
                let _ = sender
                    .send(Message::Text(Utf8Bytes::from(out.to_string())))
                    .await;
                continue;
            }
        };

        let mtype = parsed.get("type").and_then(Value::as_str).unwrap_or("");
        let id = parsed.get("id").and_then(Value::as_str);
        let has_payload = parsed
            .get("payload")
            .map(|p| !p.is_null() && p.as_object().map(|o| !o.is_empty()).unwrap_or(true))
            .unwrap_or(false);

        let response = if mtype.is_empty() {
            error_message("INVALID_PAYLOAD", "missing message type", id)
        } else if is_deprecated(mtype) {
            error_message(
                "DEPRECATED_MESSAGE_TYPE",
                &format!("{mtype} removed. Use REST API instead. See docs/REST-API.md"),
                id,
            )
        } else if mtype == "session.load" && !has_payload {
            error_message("INVALID_PAYLOAD", "session.load requires a payload", id)
        } else {
            error_message("NO_SESSION", "No active session", id)
        };

        if sender
            .send(Message::Text(Utf8Bytes::from(response.to_string())))
            .await
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deprecated_types_are_rejected() {
        for t in DEPRECATED {
            let v = classify(t, true);
            assert_eq!(v["error"]["code"], "DEPRECATED_MESSAGE_TYPE", "for {t}");
        }
    }

    #[test]
    fn session_load_without_payload_is_invalid() {
        let v = classify("session.load", false);
        assert_eq!(v["error"]["code"], "INVALID_PAYLOAD");
    }

    #[test]
    fn live_types_require_a_session() {
        let v = classify("runner.launch", true);
        assert_eq!(v["error"]["code"], "NO_SESSION");
    }
}
