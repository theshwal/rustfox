//! Guards against the Rust implementation drifting from the extracted contract.
//! The contract files are the single source of truth; these tests fail if the
//! implementation and the contract disagree.

use rustfox::db::Db;
use std::path::Path;

fn contract(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("contract")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read contract {name}: {e}"))
}

#[test]
fn rest_contract_covers_the_expected_surface() {
    let v: serde_json::Value = serde_json::from_str(&contract("rest-routes.json")).unwrap();
    let routes = v.as_array().expect("rest-routes.json must be an array");
    assert!(
        routes.len() >= 200,
        "expected the full REST surface, got {} routes",
        routes.len()
    );
}

#[test]
fn ws_contract_classifies_every_inbound_message() {
    let v: serde_json::Value = serde_json::from_str(&contract("ws-protocol.json")).unwrap();
    let inbound = v["inbound"].as_array().expect("inbound array");
    assert_eq!(
        inbound.len(),
        v["vocabularySize"].as_u64().unwrap() as usize
    );

    // Every message the contract declares must be answered by the
    // implementation with the same code the original server returned.
    for msg in inbound {
        let t = msg["type"].as_str().unwrap();
        let classification = msg["classification"].as_str().unwrap();
        let expected_code = msg["code"].as_str().unwrap_or("NO_SESSION");

        let with_payload = classification != "invalid_payload";
        let resp = rustfox::ws::classify(t, with_payload);
        let actual = resp["error"]["code"].as_str().unwrap_or("");

        match classification {
            // Deprecated types must be refused regardless of payload.
            "deprecated" => assert_eq!(
                actual, "DEPRECATED_MESSAGE_TYPE",
                "{t} must be rejected as deprecated"
            ),
            // Types whose payload shape is checked first: the original rejects
            // an empty payload with INVALID_PAYLOAD before anything else.
            "invalid_payload" => {
                assert_eq!(
                    actual, expected_code,
                    "{t} answered {actual}, original answered {expected_code}"
                );
                assert_eq!(
                    rustfox::ws::classify(t, true)["error"]["code"],
                    "NO_SESSION",
                    "{t} must proceed past validation once a payload is present"
                );
            }
            // Routed types need an active session; without one the original
            // answers NO_SESSION.
            "accepted" => assert_eq!(
                actual, expected_code,
                "{t} answered {actual}, original answered {expected_code}"
            ),
            other => panic!("unexpected classification {other} for {t}"),
        }

        // A deprecated message must stay deprecated even with a real payload,
        // and a routed message must never be treated as deprecated.
        let always = rustfox::ws::classify(t, true);
        if classification == "deprecated" {
            assert_eq!(always["error"]["code"], "DEPRECATED_MESSAGE_TYPE");
        } else {
            assert_ne!(always["error"]["code"], "DEPRECATED_MESSAGE_TYPE");
        }
    }
}

#[test]
fn schema_contract_creates_every_table_the_db_module_expects() {
    let db = Db::open(Path::new(":memory:")).unwrap();
    let tables = db.table_names().unwrap();

    // Cross-check the DDL shipped in the binary against the contract file.
    let contract_sql = contract("db-schema.sql");
    assert_eq!(
        contract_sql.trim(),
        rustfox::db::SCHEMA.trim(),
        "embedded schema has drifted from contract/db-schema.sql"
    );

    // Every table named in the DDL must exist in the live database.
    let expected = contract_sql
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            t.strip_prefix("CREATE TABLE ")
                .or_else(|| t.strip_prefix("CREATE TABLE IF NOT EXISTS "))
        })
        .map(|rest| rest.split('(').next().unwrap_or("").trim().to_string())
        .collect::<Vec<_>>();
    assert!(expected.len() >= 12, "expected 12+ tables in DDL");
    for name in expected {
        assert!(
            tables.contains(&name),
            "table {name} declared in the contract is missing from the database"
        );
    }
}

#[test]
fn empty_database_returns_empty_collections_not_errors() {
    let db = Db::open(Path::new(":memory:")).unwrap();
    assert_eq!(db.count("projects").unwrap(), 0);
    assert_eq!(db.count("sessions").unwrap(), 0);
    assert_eq!(db.count("events").unwrap(), 0);
}
