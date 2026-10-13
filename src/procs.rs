//! Background-process bookkeeping, ported from
//! `src/server/tools/background-process/store.ts`.
//!
//! Pinned by contract/process-rules.json, captured by driving the original.

use std::collections::HashMap;

/// Concurrent processes allowed per session.
pub const MAX_PER_SESSION: usize = 5;

/// Lifecycle state of a background process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Pending,
    Running,
    Exited,
}

impl ProcessStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ProcessStatus::Pending => "pending",
            ProcessStatus::Running => "running",
            ProcessStatus::Exited => "exited",
        }
    }

    pub fn is_live(self) -> bool {
        self != ProcessStatus::Exited
    }
}

/// A tracked background process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub command: String,
    pub cwd: String,
    pub pid: Option<i32>,
    pub status: ProcessStatus,
    pub exit_code: Option<i32>,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
}

/// One captured output line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Dense index, independent of which stream the line came from.
    pub offset: usize,
    pub content: String,
    pub timestamp: u64,
    pub stream: Stream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    pub fn as_str(self) -> &'static str {
        match self {
            Stream::Stdout => "stdout",
            Stream::Stderr => "stderr",
        }
    }
}

/// A page of logs plus the cursor for the next one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaginatedLogs {
    pub lines: Vec<LogLine>,
    pub total_lines: usize,
    pub next_offset: usize,
    pub has_more: bool,
}

#[derive(Debug, Default)]
pub struct ProcessStore {
    /// session -> process id -> process
    processes: HashMap<String, HashMap<String, Process>>,
    /// process id -> log buffer
    logs: HashMap<String, Vec<LogLine>>,
    next_id: u64,
}

impl ProcessStore {
    pub fn new() -> Self {
        ProcessStore::default()
    }

    pub fn max_per_session() -> usize {
        MAX_PER_SESSION
    }

    /// Look up a process; the session is part of the key, so the same id in
    /// another session is not visible.
    pub fn get_process(&self, process_id: &str, session_id: &str) -> Option<&Process> {
        self.processes.get(session_id)?.get(process_id)
    }

    /// Look up across every session.
    pub fn get_process_by_id(&self, process_id: &str) -> Option<&Process> {
        self.processes.values().find_map(|m| m.get(process_id))
    }

    pub fn session_processes(&self, session_id: &str) -> Vec<&Process> {
        match self.processes.get(session_id) {
            Some(m) => m.values().collect(),
            None => Vec::new(),
        }
    }

    /// How many processes count against the cap.
    ///
    /// Only processes that have not exited count, which is why a session can
    /// hold more than `MAX_PER_SESSION` entries in total while the live count
    /// stays at the cap. A caller reading `session_processes().len()` and
    /// concluding the session is full would be wrong.
    pub fn session_process_count(&self, session_id: &str) -> usize {
        self.session_processes(session_id)
            .iter()
            .filter(|p| p.status.is_live())
            .count()
    }

    /// Create a process, or `None` when the live cap is reached.
    pub fn create_process(
        &mut self,
        session_id: &str,
        name: &str,
        command: &str,
        cwd: &str,
        now: u64,
    ) -> Option<Process> {
        if self.session_process_count(session_id) >= MAX_PER_SESSION {
            return None;
        }

        self.next_id += 1;
        let id = format!("bp-{session_id}-{n}", n = self.next_id);
        let process = Process {
            id: id.clone(),
            session_id: session_id.to_string(),
            name: name.to_string(),
            command: command.to_string(),
            cwd: cwd.to_string(),
            pid: None,
            status: ProcessStatus::Pending,
            exit_code: None,
            created_at: now,
            started_at: None,
            ended_at: None,
        };

        self.processes
            .entry(session_id.to_string())
            .or_default()
            .insert(id.clone(), process.clone());
        self.logs.insert(id, Vec::new());

        Some(process)
    }

    /// Record the pid once the process has actually started.
    pub fn start_process(
        &mut self,
        process_id: &str,
        session_id: &str,
        pid: i32,
        now: u64,
    ) -> Option<&Process> {
        let p = self.processes.get_mut(session_id)?.get_mut(process_id)?;
        p.pid = Some(pid);
        p.status = ProcessStatus::Running;
        p.started_at = Some(now);
        Some(p)
    }

    /// Update the status; an exit also records the code and end time.
    pub fn update_status(
        &mut self,
        process_id: &str,
        session_id: &str,
        status: ProcessStatus,
        exit_code: Option<i32>,
        now: u64,
    ) -> Option<&Process> {
        let p = self.processes.get_mut(session_id)?.get_mut(process_id)?;
        p.status = status;
        if status == ProcessStatus::Exited {
            p.exit_code = exit_code;
            p.ended_at = Some(now);
        }
        Some(p)
    }

    pub fn remove_process(&mut self, process_id: &str, session_id: &str) -> bool {
        let removed = self
            .processes
            .get_mut(session_id)
            .map(|m| m.remove(process_id).is_some())
            .unwrap_or(false);
        if removed {
            self.logs.remove(process_id);
        }
        removed
    }

    /// Append an output line; unknown process ids are ignored rather than
    /// creating a buffer.
    pub fn append_log(&mut self, process_id: &str, content: &str, stream: Stream, now: u64) {
        let Some(logs) = self.logs.get_mut(process_id) else {
            return;
        };
        let offset = logs.len();
        logs.push(LogLine {
            offset,
            content: content.to_string(),
            timestamp: now,
            stream,
        });
    }

    /// Log slice from `offset`, following JavaScript's `Array.slice` semantics
    /// because the original uses it directly.
    ///
    /// A negative `offset` counts back from the end, so `-5` means "five lines
    /// from the tail" and `-5` with a limit of 3 returns the three lines before
    /// that. A `limit` below zero likewise means "stop that many from the
    /// end". This is `Math.min(offset, length)` with no lower bound, followed by
    /// `slice`; it is not a clamp.
    pub fn get_logs(&self, process_id: &str, offset: i64, limit: Option<i64>) -> Vec<LogLine> {
        let empty = Vec::new();
        let logs = self.logs.get(process_id).unwrap_or(&empty);
        let len = logs.len() as i64;

        // Resolve a possibly-negative index against the current length, the way
        // slice() does.
        let resolve = |i: i64, base: i64| -> i64 {
            if i < 0 {
                (base + i).max(0)
            } else {
                i.min(base)
            }
        };

        let start = resolve(offset, len);
        let end = match limit {
            Some(l) => {
                let raw = start + l;
                if raw < 0 {
                    (len + raw).max(0)
                } else {
                    raw.min(len)
                }
            }
            None => len,
        };

        if end <= start {
            return Vec::new();
        }
        logs[start as usize..end as usize].to_vec()
    }

    /// Port of `getLogsPaginated`.
    ///
    /// `next_offset` falls back to `total_lines` when nothing was returned, so
    /// `max_lines = 0` still advances the cursor to the end while reporting
    /// `has_more: true`. A caller looping on `has_more` with a zero page would
    /// stop after one iteration; that is the original's behaviour.
    pub fn get_logs_paginated(
        &self,
        process_id: &str,
        since: usize,
        max_lines: usize,
    ) -> PaginatedLogs {
        let empty = Vec::new();
        let logs = self.logs.get(process_id).unwrap_or(&empty);
        let total_lines = logs.len();

        let filtered: Vec<&LogLine> = logs.iter().filter(|l| l.offset >= since).collect();
        let sliced: Vec<&LogLine> = filtered.iter().take(max_lines).copied().collect();

        let next_offset = match sliced.last() {
            Some(last) => last.offset + 1,
            None => total_lines,
        };
        let has_more = sliced.len() < filtered.len();

        PaginatedLogs {
            lines: sliced.into_iter().cloned().collect(),
            total_lines,
            next_offset,
            has_more,
        }
    }

    /// Empty a process's buffer while keeping the process itself.
    pub fn clear_logs(&mut self, process_id: &str) {
        self.logs.insert(process_id.to_string(), Vec::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_is_five_live_processes() {
        assert_eq!(ProcessStore::max_per_session(), 5);
        let mut s = ProcessStore::new();
        for i in 0..5 {
            assert!(
                s.create_process("s", &format!("p{i}"), "c", "/w", 0)
                    .is_some()
            );
        }
        assert!(
            s.create_process("s", "p6", "c", "/w", 0).is_none(),
            "the sixth live process is refused"
        );
    }

    #[test]
    fn exited_processes_stop_counting_but_are_kept() {
        let mut s = ProcessStore::new();
        let ids: Vec<String> = (0..5)
            .map(|i| {
                s.create_process("s", &format!("p{i}"), "c", "/w", 0)
                    .unwrap()
                    .id
            })
            .collect();
        // Two exit; the cap frees two slots.
        for id in ids.iter().take(2) {
            s.update_status(id, "s", ProcessStatus::Exited, Some(0), 10);
        }
        assert_eq!(s.session_process_count("s"), 3);
        assert!(
            s.create_process("s", "new", "c", "/w", 0).is_some(),
            "exited processes do not hold their slot"
        );
        assert_eq!(s.session_processes("s").len(), 6, "but they remain listed");
    }

    #[test]
    fn all_exited_means_the_cap_stops_applying() {
        let mut s = ProcessStore::new();
        let mut ids = Vec::new();
        for i in 0..5 {
            ids.push(
                s.create_process("s", &format!("p{i}"), "c", "/w", 0)
                    .unwrap()
                    .id,
            );
        }
        for id in &ids {
            s.update_status(id, "s", ProcessStatus::Exited, Some(0), 1);
        }
        assert_eq!(s.session_process_count("s"), 0);
        for i in 5..8 {
            assert!(
                s.create_process("s", &format!("p{i}"), "c", "/w", 0)
                    .is_some()
            );
        }
        assert_eq!(s.session_processes("s").len(), 8);
        assert_eq!(s.session_process_count("s"), 3);
    }

    #[test]
    fn processes_are_keyed_by_session_and_id() {
        let mut s = ProcessStore::new();
        let p = s.create_process("a", "p", "c", "/w", 0).unwrap();
        assert!(s.get_process(&p.id, "a").is_some());
        assert!(
            s.get_process(&p.id, "b").is_none(),
            "the same id is invisible from another session"
        );
        assert!(s.get_process_by_id(&p.id).is_some());
    }

    #[test]
    fn log_offsets_are_dense_across_streams() {
        let mut s = ProcessStore::new();
        let p = s.create_process("s", "p", "c", "/w", 0).unwrap();
        for i in 0..6 {
            let stream = if i % 3 == 2 {
                Stream::Stderr
            } else {
                Stream::Stdout
            };
            s.append_log(&p.id, &format!("line {i}"), stream, 0);
        }
        let lines = s.get_logs(&p.id, 0, None);
        assert_eq!(
            lines.iter().map(|l| l.offset).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5],
            "stderr lines still consume an offset"
        );
        assert_eq!(lines[2].stream, Stream::Stderr);
        assert_eq!(lines[0].stream, Stream::Stdout);
    }

    #[test]
    fn a_negative_offset_is_not_clamped_to_zero() {
        let mut s = ProcessStore::new();
        let p = s.create_process("s", "p", "c", "/w", 0).unwrap();
        for i in 0..10 {
            s.append_log(&p.id, &format!("line {i}"), Stream::Stdout, 0);
        }
        // The original uses Math.min(offset, len) with no lower bound, so a
        // negative offset starts from the end.
        let lines = s.get_logs(&p.id, -5, Some(3));
        assert_eq!(
            lines.iter().map(|l| l.content.clone()).collect::<Vec<_>>(),
            vec!["line 5", "line 6", "line 7"]
        );
    }

    #[test]
    fn pagination_advances_the_cursor() {
        let mut s = ProcessStore::new();
        let p = s.create_process("s", "p", "c", "/w", 0).unwrap();
        for i in 0..10 {
            s.append_log(&p.id, &format!("line {i}"), Stream::Stdout, 0);
        }

        let first = s.get_logs_paginated(&p.id, 0, 3);
        assert_eq!(first.lines.len(), 3);
        assert_eq!(first.next_offset, 3);
        assert!(first.has_more);
        assert_eq!(first.total_lines, 10);

        let second = s.get_logs_paginated(&p.id, first.next_offset, 3);
        assert_eq!(second.lines[0].content, "line 3");
        assert_eq!(second.next_offset, 6);

        let last = s.get_logs_paginated(&p.id, second.next_offset, 500);
        assert_eq!(last.lines.len(), 4);
        assert_eq!(last.next_offset, 10);
        assert!(!last.has_more);
    }

    #[test]
    fn a_zero_page_still_advances_to_the_end() {
        let mut s = ProcessStore::new();
        let p = s.create_process("s", "p", "c", "/w", 0).unwrap();
        for i in 0..10 {
            s.append_log(&p.id, &format!("line {i}"), Stream::Stdout, 0);
        }
        let page = s.get_logs_paginated(&p.id, 0, 0);
        assert_eq!(page.lines.len(), 0);
        assert_eq!(
            page.next_offset, 10,
            "no line returned, so the cursor jumps to the end"
        );
        assert!(page.has_more, "and it still claims there is more");
    }

    #[test]
    fn an_unknown_process_yields_empty_results() {
        let s = ProcessStore::new();
        assert_eq!(s.get_logs("nope", 0, None).len(), 0);
        let page = s.get_logs_paginated("nope", 0, 500);
        assert_eq!(page.total_lines, 0);
        assert_eq!(page.next_offset, 0);
        assert!(!page.has_more);
        assert!(s.get_process("nope", "s").is_none());
    }

    #[test]
    fn clearing_keeps_the_process_and_empties_the_buffer() {
        let mut s = ProcessStore::new();
        let p = s.create_process("s", "p", "c", "/w", 0).unwrap();
        for i in 0..5 {
            s.append_log(&p.id, &format!("line {i}"), Stream::Stdout, 0);
        }
        s.clear_logs(&p.id);
        let page = s.get_logs_paginated(&p.id, 0, 500);
        assert_eq!(page.total_lines, 0);
        assert_eq!(page.next_offset, 0);
        assert!(!page.has_more);
        assert!(s.get_process(&p.id, "s").is_some(), "the process survives");
    }

    #[test]
    fn appending_to_an_unknown_process_is_ignored() {
        let mut s = ProcessStore::new();
        s.append_log("nope", "x", Stream::Stdout, 0);
        assert_eq!(s.get_logs_paginated("nope", 0, 10).total_lines, 0);
    }
}
