//! Tests for the read-before-write guard.
//!
//! The rules matter more than the hash: this is what stops the agent from
//! overwriting a file it never saw, or that changed under it. Each case mirrors
//! one branch of `validateFileForWrite` in the original.

use rustfox::filetrack::{
    FileReadEntry, Rejection, compute_file_hash, normalize_path, sha256_hex,
    validate_file_for_write,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Create a temp file with known content and return its path.
fn write_temp(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("rustfox-filetrack-tests");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join(name);
    std::fs::write(&path, content).expect("write temp file");
    path
}

#[test]
fn hash_is_stable_and_content_addressed() {
    let a = write_temp("stable-a.txt", "same content");
    let b = write_temp("stable-b.txt", "same content");
    let c = write_temp("stable-c.txt", "different");

    assert_eq!(
        compute_file_hash(&a),
        compute_file_hash(&b),
        "identical content must hash identically"
    );
    assert_ne!(compute_file_hash(&a), compute_file_hash(&c));
    assert_eq!(
        compute_file_hash(&a),
        Some(sha256_hex(b"same content")),
        "the file hash must match the raw content hash"
    );
}

#[test]
fn a_file_that_was_never_read_is_rejected() {
    let path = write_temp("never-read.txt", "content");
    let reads = HashMap::new();
    let v = validate_file_for_write(&path, &reads, None);

    assert!(!v.valid);
    assert_eq!(v.rejection, Some(Rejection::NotRead));
    assert!(
        Rejection::NotRead
            .message(&path.to_string_lossy())
            .contains("must be read before writing")
    );
}

#[test]
fn a_file_read_at_the_same_hash_is_allowed() {
    let path = write_temp("read-ok.txt", "content");
    let mut reads = HashMap::new();
    reads.insert(
        normalize_path(&path),
        FileReadEntry {
            hash: sha256_hex(b"content"),
            rel_path: "read-ok.txt".to_string(),
        },
    );

    let v = validate_file_for_write(&path, &reads, None);
    assert!(v.valid, "an unmodified file that was read may be written");
    assert_eq!(v.rejection, None);
}

#[test]
fn a_file_changed_since_the_read_is_rejected() {
    let path = write_temp("changed.txt", "original");
    let mut reads = HashMap::new();
    reads.insert(
        normalize_path(&path),
        FileReadEntry {
            hash: sha256_hex(b"original"),
            rel_path: "changed.txt".to_string(),
        },
    );

    // Something else modifies the file after the read.
    std::fs::write(&path, "modified by someone else").expect("modify file");

    let v = validate_file_for_write(&path, &reads, None);
    assert!(!v.valid);
    assert_eq!(v.rejection, Some(Rejection::ChangedExternally));
    assert_eq!(
        Rejection::ChangedExternally.error_name(),
        "FileChangedExternallyError"
    );
}

#[test]
fn a_new_file_needs_no_read() {
    let path = std::env::temp_dir()
        .join("rustfox-filetrack-tests")
        .join("brand-new.txt");
    let _ = std::fs::remove_file(&path);

    let v = validate_file_for_write(&path, &HashMap::new(), None);
    assert!(v.valid, "a file that does not exist yet may be created");
}

#[test]
fn an_identical_hash_in_another_workspace_counts_as_read() {
    let dir = std::env::temp_dir().join("rustfox-filetrack-tests/ws-new");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    std::fs::write(&path, "{}").unwrap();

    let mut reads = HashMap::new();
    // Recorded against a different absolute path, but the same repo-relative
    // path and identical content.
    reads.insert(
        PathBuf::from("/old-workspace/config.json"),
        FileReadEntry {
            hash: sha256_hex(b"{}"),
            rel_path: "config.json".to_string(),
        },
    );

    let v = validate_file_for_write(&path, &reads, Some(&dir));
    assert!(
        v.valid,
        "an identical file read in another workspace must be writable"
    );
}

#[test]
fn a_differing_hash_in_another_workspace_does_not_count() {
    let dir = std::env::temp_dir().join("rustfox-filetrack-tests/ws-new2");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    std::fs::write(&path, "{\"new\":true}").unwrap();

    let mut reads = HashMap::new();
    reads.insert(
        PathBuf::from("/old-workspace/config.json"),
        FileReadEntry {
            hash: sha256_hex(b"{\"old\":true}"),
            rel_path: "config.json".to_string(),
        },
    );

    let v = validate_file_for_write(&path, &reads, Some(&dir));
    assert!(
        !v.valid,
        "different content must not satisfy the read check"
    );
    assert_eq!(v.rejection, Some(Rejection::NotRead));
}

#[test]
fn paths_escaping_the_base_are_never_matched_cross_workspace() {
    let base = std::env::temp_dir().join("rustfox-filetrack-tests/base");
    let outside = std::env::temp_dir().join("rustfox-filetrack-tests/escaped.json");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(&outside, "{}").unwrap();

    let mut reads = HashMap::new();
    reads.insert(
        PathBuf::from("/anywhere/config.json"),
        FileReadEntry {
            hash: sha256_hex(b"{}"),
            rel_path: "../escaped.json".to_string(),
        },
    );

    // The target is outside `base`, so the cross-workspace fallback must not
    // apply even though the recorded relPath happens to match.
    let v = validate_file_for_write(&outside, &reads, Some(&base));
    assert!(!v.valid);
}

#[test]
fn path_normalisation_collapses_dot_segments() {
    assert_eq!(
        normalize_path(Path::new("/a/b/../c/./d")),
        PathBuf::from("/a/c/d")
    );
    assert_eq!(
        normalize_path(Path::new("/a/b/../c/../d")),
        PathBuf::from("/a/d")
    );
}
