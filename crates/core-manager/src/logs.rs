//! In-memory ring buffer capturing this app's own `tracing` events
//! (`source: "app"`, fed by `src-tauri`'s custom `tracing_subscriber::Layer`)
//! and sing-box's child-process stdout/stderr lines (`source: "core"`, fed
//! by `spawn_line_reader`, called from `CoreManager::start`) into one
//! bounded, evict-oldest `VecDeque` -- same capped-ring-buffer-with-eviction
//! shape as `history::append_entries`'s `MAX_HISTORY_LINES` cap, just kept
//! purely in memory (never persisted to disk) since these are ephemeral
//! diagnostic lines, not a look-back audit trail like connection history.
//!
//! Exposed to the frontend via the `logs_get`/`logs_clear` Tauri commands
//! (`src-tauri/src/commands/logs.rs`), which read/clear the same `LogBuffer`
//! instance `AppState` holds and `CoreManager` was handed via
//! `set_log_buffer` -- see that method's doc comment for why this is a
//! setter (constructed independently, before `AppState`/`CoreManager` exist)
//! rather than built inside `CoreManager::new()`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use shared_types::{LogEntry, LogLevel};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::task::JoinHandle;

/// Maximum number of log lines kept in memory. Enforced on every push
/// (evict-oldest), not periodically -- mirrors `history::MAX_HISTORY_LINES`'s
/// reasoning, just in memory instead of on disk, and a bit larger since
/// there's no per-line disk-write cost to worry about.
pub const MAX_LOG_LINES: usize = 2000;

/// Owns the shared ring buffer. Cheap to construct (`Mutex<VecDeque>`, no
/// I/O) -- the one instance for the whole app's lifetime is created once in
/// `src-tauri`'s `run()` (so the tracing layer and `CoreManager` can share
/// the exact same `Arc`), not per-run like `history::HistoryRecorder`.
pub struct LogBuffer {
    entries: Mutex<VecDeque<LogEntry>>,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self { entries: Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES)) }
    }

    /// Appends one entry, evicting the oldest if this would exceed
    /// `MAX_LOG_LINES`. Timestamp is generated here (not passed in) via
    /// `history::now_rfc3339`, same helper `HistoryRecorder` uses -- see
    /// that module's doc comment for why this is hand-rolled instead of
    /// pulling in a `chrono`/`time` dependency.
    pub fn push(&self, level: LogLevel, source: &str, target: Option<String>, message: String) {
        let entry = LogEntry {
            timestamp: crate::history::now_rfc3339(),
            level,
            source: source.to_string(),
            target,
            message,
        };
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= MAX_LOG_LINES {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Every currently-buffered entry, oldest first (insertion order) --
    /// `logs_get` returns this as-is, no reversal (unlike `history_list`,
    /// which reverses to most-recent-first for a look-back table; a live
    /// log view reads top-to-bottom, oldest-first, same as a terminal).
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawns a background task that reads `reader` line-by-line until EOF/error
/// and pushes each non-blank line into `buffer` tagged `source: "core"` --
/// used for both a spawned sing-box child's stdout and stderr (see
/// `CoreManager::start`). Runs until the pipe closes (the process exited) or
/// the returned `JoinHandle` is `.abort()`-ed (see `CoreManager::stop_running`,
/// which aborts these alongside `history_task` on stop).
pub fn spawn_line_reader<R>(reader: R, buffer: Arc<LogBuffer>) -> JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    // sing-box colorizes its level tag (e.g.
                    // `\x1b[36mINFO\x1b[0m`) even when stdout is piped, not a
                    // real terminal -- confirmed via a live run capturing
                    // real sing-box output. Left in, these render as garbled
                    // control-character boxes in the frontend's plain-text
                    // log view, so strip them before storing.
                    let line = strip_ansi_codes(&line);
                    let level = infer_level(&line);
                    buffer.push(level, "core", None, line);
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!("core log reader: failed to read line: {e}");
                    break;
                }
            }
        }
    })
}

/// Strips ANSI SGR color escape sequences (`\x1b[...m`) from a raw sing-box
/// log line. Only handles the `m`-terminated SGR form sing-box actually
/// emits (e.g. `\x1b[36m`/`\x1b[0m` around its level tag), not the full ANSI
/// escape spec -- hand-rolled rather than pulling in a dedicated crate for
/// this one narrow case, same "no dependency for one small thing" preference
/// as `history::now_rfc3339`.
fn strip_ansi_codes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            for c2 in chars.by_ref() {
                if c2 == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Best-effort level guess from a raw sing-box log line -- sing-box's own
/// log format (`<time> <LEVEL> ...`) isn't parsed structurally here, just
/// scanned for the well-known level tokens it emits (`FATAL`/`ERROR`/`WARN`/
/// `DEBUG`/`TRACE`), defaulting to `Info` when none match (sing-box's own
/// default level, and a safe fallback for a line this doesn't recognize).
fn infer_level(line: &str) -> LogLevel {
    let upper = line.to_ascii_uppercase();
    if upper.contains("FATAL") || upper.contains("ERROR") {
        LogLevel::Error
    } else if upper.contains("WARN") {
        LogLevel::Warn
    } else if upper.contains("DEBUG") {
        LogLevel::Debug
    } else if upper.contains("TRACE") {
        LogLevel::Trace
    } else {
        LogLevel::Info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_snapshot_preserves_order() {
        let buffer = LogBuffer::new();
        buffer.push(LogLevel::Info, "app", Some("mod_a".into()), "first".into());
        buffer.push(LogLevel::Warn, "core", None, "second".into());

        let snapshot = buffer.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].message, "first");
        assert_eq!(snapshot[0].source, "app");
        assert_eq!(snapshot[0].target.as_deref(), Some("mod_a"));
        assert_eq!(snapshot[1].message, "second");
        assert_eq!(snapshot[1].source, "core");
        assert!(snapshot[1].target.is_none());
    }

    #[test]
    fn push_evicts_oldest_past_cap() {
        let buffer = LogBuffer::new();
        for i in 0..MAX_LOG_LINES + 10 {
            buffer.push(LogLevel::Info, "app", None, format!("line-{i}"));
        }
        let snapshot = buffer.snapshot();
        assert_eq!(snapshot.len(), MAX_LOG_LINES);
        assert_eq!(snapshot.first().unwrap().message, "line-10");
        assert_eq!(snapshot.last().unwrap().message, format!("line-{}", MAX_LOG_LINES + 9));
    }

    #[test]
    fn clear_empties_the_buffer() {
        let buffer = LogBuffer::new();
        buffer.push(LogLevel::Info, "app", None, "hello".into());
        buffer.clear();
        assert!(buffer.snapshot().is_empty());
    }

    #[test]
    fn strip_ansi_codes_removes_sgr_sequences() {
        let raw = "+0800 2026-08-30 03:08:57 \u{1b}[36mINFO\u{1b}[0m sing-box started (0.26s)";
        assert_eq!(
            strip_ansi_codes(raw),
            "+0800 2026-08-30 03:08:57 INFO sing-box started (0.26s)"
        );
    }

    #[test]
    fn strip_ansi_codes_is_a_noop_on_plain_text() {
        assert_eq!(strip_ansi_codes("plain line, no escapes"), "plain line, no escapes");
    }

    #[test]
    fn infer_level_recognizes_known_tokens() {
        assert_eq!(infer_level("2024-01-15 ERROR failed to dial"), LogLevel::Error);
        assert_eq!(infer_level("2024-01-15 FATAL panic"), LogLevel::Error);
        assert_eq!(infer_level("2024-01-15 WARN retrying"), LogLevel::Warn);
        assert_eq!(infer_level("2024-01-15 DEBUG verbose"), LogLevel::Debug);
        assert_eq!(infer_level("2024-01-15 TRACE fine-grained"), LogLevel::Trace);
        assert_eq!(infer_level("2024-01-15 sing-box started"), LogLevel::Info);
    }
}
