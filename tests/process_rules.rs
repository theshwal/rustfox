//! Replays contract/process-rules.json against the Rust port.
//!
//! The fixtures were produced by driving the original store through cap
//! exhaustion, log pagination and cross-session lookups.

use rustfox::procs::{ProcessStatus, ProcessStore, Stream};
use serde_json::Value;

fn contract() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contract/process-rules.json"
    ))
    .expect("contract/process-rules.json must exist");
    serde_json::from_str(&raw).expect("contract must be valid JSON")
}

fn rules() -> Value {
    contract()["rules"].clone()
}

#[test]
fn the_cap_matches_the_original() {
    let r = rules();
    assert_eq!(
        ProcessStore::max_per_session(),
        r["maxPerSession"].as_u64().expect("maxPerSession recorded") as usize
    );
}

#[test]
fn cap_scenarios_match_the_original() {
    let r = rules();
    for case in r["capScenarios"].as_array().expect("capScenarios array") {
        let exited_first = case["exitedFirst"].as_u64().unwrap() as usize;
        let want_created = case["created"].as_u64().unwrap() as usize;
        let want_live = case["liveCount"].as_u64().unwrap() as usize;
        let want_total = case["totalCount"].as_u64().unwrap() as usize;

        let mut s = ProcessStore::new();
        let session = format!("cap-{exited_first}");
        let mut ids = Vec::new();
        for i in 0..8 {
            if let Some(p) = s.create_process(&session, &format!("p{i}"), "cmd", "/w", 0) {
                let id = p.id.clone();
                ids.push(id.clone());
                if i < exited_first {
                    s.update_status(&id, &session, ProcessStatus::Exited, Some(0), 1);
                }
            }
        }

        assert_eq!(ids.len(), want_created, "exited_first={exited_first}");
        assert_eq!(
            s.session_process_count(&session),
            want_live,
            "live count for exited_first={exited_first}"
        );
        assert_eq!(
            s.session_processes(&session).len(),
            want_total,
            "total entries for exited_first={exited_first}"
        );
    }
}

#[test]
fn log_slicing_matches_the_original() {
    let r = rules();
    let mut s = ProcessStore::new();
    let p = s.create_process("s", "p", "c", "/w", 0).unwrap();
    for i in 0..10 {
        let stream = if i % 3 == 2 {
            Stream::Stderr
        } else {
            Stream::Stdout
        };
        s.append_log(&p.id, &format!("line {i}"), stream, 0);
    }

    for case in r["getLogs"].as_array().expect("getLogs array") {
        let offset = case["offset"].as_i64().unwrap();
        let limit = case["limit"].as_i64();
        let want_count = case["count"].as_u64().unwrap() as usize;
        let want_first = case["first"].as_str();
        let want_last = case["last"].as_str();

        let lines = s.get_logs(&p.id, offset, limit);
        assert_eq!(
            lines.len(),
            want_count,
            "get_logs(offset={offset}, limit={limit:?}) returned the wrong count"
        );
        assert_eq!(
            lines.first().map(|l| l.content.as_str()),
            want_first,
            "offset={offset}"
        );
        assert_eq!(
            lines.last().map(|l| l.content.as_str()),
            want_last,
            "offset={offset}"
        );
    }
}

#[test]
fn pagination_matches_the_original() {
    let r = rules();
    let mut s = ProcessStore::new();
    let p = s.create_process("s", "p", "c", "/w", 0).unwrap();
    for i in 0..10 {
        s.append_log(&p.id, &format!("line {i}"), Stream::Stdout, 0);
    }

    for case in r["paginated"].as_array().expect("paginated array") {
        let since = case["since"].as_u64().unwrap() as usize;
        let max_lines = case["maxLines"].as_u64().unwrap() as usize;
        let page = s.get_logs_paginated(&p.id, since, max_lines);

        assert_eq!(
            page.lines.len(),
            case["count"].as_u64().unwrap() as usize,
            "since={since} max_lines={max_lines}"
        );
        assert_eq!(
            page.total_lines,
            case["totalLines"].as_u64().unwrap() as usize
        );
        assert_eq!(
            page.next_offset,
            case["nextOffset"].as_u64().unwrap() as usize,
            "since={since} max_lines={max_lines}"
        );
        assert_eq!(page.has_more, case["hasMore"].as_bool().unwrap());
        assert_eq!(
            page.lines.first().map(|l| l.content.as_str()),
            case["first"].as_str()
        );
        assert_eq!(
            page.lines.last().map(|l| l.content.as_str()),
            case["last"].as_str()
        );
    }
}

#[test]
fn offsets_are_dense_and_the_clear_behaviour_matches() {
    let r = rules();
    let offsets: Vec<(u64, String)> = r["offsetCheck"]
        .as_array()
        .expect("offsetCheck array")
        .iter()
        .map(|x| {
            (
                x["offset"].as_u64().unwrap(),
                x["stream"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        offsets.iter().map(|(o, _)| *o).collect::<Vec<_>>(),
        (0..offsets.len() as u64).collect::<Vec<_>>(),
        "offsets are dense regardless of stream"
    );
    assert!(
        offsets.iter().any(|(_, st)| st == "stderr"),
        "stderr lines are interleaved, not separated"
    );

    let mut s = ProcessStore::new();
    let p = s.create_process("s", "p", "c", "/w", 0).unwrap();
    for i in 0..5 {
        s.append_log(&p.id, &format!("line {i}"), Stream::Stdout, 0);
    }
    s.clear_logs(&p.id);
    let page = s.get_logs_paginated(&p.id, 0, 500);
    assert_eq!(
        page.total_lines,
        r["afterClear"]["total"].as_u64().unwrap() as usize
    );
    assert_eq!(
        page.next_offset,
        r["afterClear"]["nextOffset"].as_u64().unwrap() as usize
    );
    assert_eq!(page.has_more, r["afterClear"]["hasMore"].as_bool().unwrap());
    assert!(
        s.get_process(&p.id, "s").is_some(),
        "clearing logs keeps the process"
    );
}

#[test]
fn unknown_processes_and_cross_session_lookups_match() {
    let r = rules();
    let s = ProcessStore::new();
    assert_eq!(
        s.get_logs("nope", 0, None).len(),
        r["unknownProcess"]["getLogs"].as_u64().unwrap() as usize
    );
    let page = s.get_logs_paginated("nope", 0, 500);
    assert_eq!(
        page.total_lines,
        r["unknownProcess"]["paginated"]["totalLines"]
            .as_u64()
            .unwrap() as usize
    );
    assert_eq!(
        page.next_offset,
        r["unknownProcess"]["paginated"]["nextOffset"]
            .as_u64()
            .unwrap() as usize
    );

    let mut s2 = ProcessStore::new();
    let p = s2.create_process("a", "p", "c", "/w", 0).unwrap();
    assert_eq!(
        s2.get_process(&p.id, "a").is_some(),
        r["crossSession"]["rightSession"].as_bool().unwrap()
    );
    assert!(s2.get_process(&p.id, "b").is_none());
    assert_eq!(
        s2.get_process_by_id(&p.id).is_some(),
        r["crossSession"]["foundById"].as_bool().unwrap()
    );
}
