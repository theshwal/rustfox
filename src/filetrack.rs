//! Read-before-write tracking, ported from `src/server/tools/file-tracker.ts`.
//!
//! This is the guard that stops the agent from overwriting a file it never saw,
//! or that changed since it read it. The exact semantics matter: what counts as
//! "read", when a cross-workspace match is allowed, and which error is reported.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A record of one `read_file` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReadEntry {
    /// SHA-256 of the content at read time.
    pub hash: String,
    /// Path relative to the workspace, used for cross-workspace matching.
    pub rel_path: String,
}

/// Which rule rejected a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// The file was never read.
    NotRead,
    /// The file changed on disk since it was read.
    ChangedExternally,
}

impl Rejection {
    pub fn error_name(self) -> &'static str {
        match self {
            Rejection::NotRead => "FileNotReadError",
            Rejection::ChangedExternally => "FileChangedExternallyError",
        }
    }

    /// The exact message the original's error classes produce.
    pub fn message(self, path: &str) -> String {
        match self {
            Rejection::NotRead => {
                format!("File \"{path}\" must be read before writing. Use read_file first.")
            }
            Rejection::ChangedExternally => format!(
                "File \"{path}\" was modified externally and must be read before writing. Use read_file to see current contents."
            ),
        }
    }
}

/// Outcome of `validateFileForWrite`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validation {
    pub valid: bool,
    pub rejection: Option<Rejection>,
}

/// SHA-256 of a file, or `None` when it does not exist.
///
/// Mirrors `computeFileHash`, which swallows ENOENT but propagates every other
/// filesystem error rather than treating them as "missing".
pub fn compute_file_hash(path: &Path) -> Option<String> {
    match std::fs::read(path) {
        Ok(bytes) => Some(sha256_hex(&bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => panic!("computeFileHash only swallows ENOENT, got: {e}"),
    }
}

/// Port of `validateFileForWrite`.
///
/// The rules, in the original's order:
/// 1. a file that does not exist on disk may be written without being read
/// 2. a file that exists must have been read, or it is `NotRead`
/// 3. if it was read but the hash moved, it is `ChangedExternally`
/// 4. with a `baseDir`, an identical hash recorded under the same repo-relative
///    path from another workspace also counts as read
pub fn validate_file_for_write(
    file_path: &Path,
    read_files: &HashMap<PathBuf, FileReadEntry>,
    base_dir: Option<&Path>,
) -> Validation {
    let normalized = normalize_path(file_path);

    // Rule 1: a new file needs no prior read.
    let Some(current_hash) = compute_file_hash(&normalized) else {
        return Validation {
            valid: true,
            rejection: None,
        };
    };

    if let Some(entry) = read_files.get(&normalized) {
        return if entry.hash == current_hash {
            Validation {
                valid: true,
                rejection: None,
            }
        } else {
            Validation {
                valid: false,
                rejection: Some(Rejection::ChangedExternally),
            }
        };
    }

    // Cross-workspace fallback. Paths that escape the base directory are not
    // comparable: two unrelated directories can both yield "../config.json".
    if let Some(base) = base_dir {
        if let Some(rel) = relative_within(base, &normalized) {
            let matches = read_files
                .values()
                .filter(|e| e.rel_path == rel)
                .any(|e| e.hash == current_hash);
            if matches {
                return Validation {
                    valid: true,
                    rejection: None,
                };
            }
        }
    }

    Validation {
        valid: false,
        rejection: Some(Rejection::NotRead),
    }
}

/// Normalise a path the way `path.resolve` does: absolute, with `.` and `..`
/// collapsed lexically and no trailing separator.
pub fn normalize_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// `path.relative`, returning `None` when the result escapes the base.
pub fn relative_within(base: &Path, target: &Path) -> Option<String> {
    let rel = target.strip_prefix(base).ok()?;
    let s = rel.to_string_lossy().to_string();
    if s.starts_with("..") {
        return None;
    }
    Some(s)
}

/// SHA-256, hex encoded. Kept dependency-free and self-contained so the port
/// stays auditable next to the original's `createHash('sha256')`.
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // NIST vector: the 448-bit message.
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn missing_file_has_no_hash() {
        assert_eq!(compute_file_hash(Path::new("/nonexistent/path/xyz")), None);
    }

    #[test]
    fn error_messages_match_the_original_classes() {
        assert_eq!(
            Rejection::NotRead.message("/tmp/a.ts"),
            "File \"/tmp/a.ts\" must be read before writing. Use read_file first."
        );
        assert!(
            Rejection::NotRead
                .message("/x")
                .starts_with("File \"/x\" must be read before writing.")
        );
        assert_eq!(
            Rejection::ChangedExternally.error_name(),
            "FileChangedExternallyError"
        );
    }

    #[test]
    fn paths_outside_the_base_are_not_comparable() {
        assert_eq!(
            relative_within(Path::new("/repo"), Path::new("/repo/src/a.ts")),
            Some("src/a.ts".to_string())
        );
        assert_eq!(
            relative_within(Path::new("/repo"), Path::new("/other/a.ts")),
            None
        );
    }

    #[test]
    fn a_new_file_needs_no_prior_read() {
        let reads = HashMap::new();
        let v = validate_file_for_write(Path::new("/tmp/rustfox-does-not-exist"), &reads, None);
        assert!(v.valid);
        assert_eq!(v.rejection, None);
    }
}
