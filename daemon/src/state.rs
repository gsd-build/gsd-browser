//! Daemon-side state management for action timeline, versioned refs, and diff snapshots.

use browser_tools_common::types::{ActionEntry, CompactPageState};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum number of action entries kept in the timeline FIFO.
const MAX_TIMELINE_ENTRIES: usize = 60;

/// Returns the current time as seconds since UNIX_EPOCH (f64).
fn now_epoch_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// Top-level daemon state shared across all connections.
pub struct DaemonState {
    pub timeline: Mutex<ActionTimeline>,
    pub refs: Mutex<RefStore>,
    pub diff: Mutex<DiffState>,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            timeline: Mutex::new(ActionTimeline::new()),
            refs: Mutex::new(RefStore::new()),
            diff: Mutex::new(DiffState::new()),
        }
    }
}

/// Bounded FIFO of ActionEntry records, capped at MAX_TIMELINE_ENTRIES.
pub struct ActionTimeline {
    entries: VecDeque<ActionEntry>,
    next_id: u64,
}

impl ActionTimeline {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_TIMELINE_ENTRIES),
            next_id: 1,
        }
    }

    /// Begin a new action, recording tool name, params summary, and before URL.
    /// Returns the action ID.
    pub fn begin_action(&mut self, tool: &str, params_summary: &str, before_url: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let entry = ActionEntry {
            id,
            tool: tool.to_string(),
            params_summary: params_summary.to_string(),
            started_at: now_epoch_secs(),
            finished_at: 0.0,
            status: "running".to_string(),
            before_url: before_url.to_string(),
            after_url: String::new(),
            verification_summary: String::new(),
            warning_summary: String::new(),
            diff_summary: String::new(),
            changed: false,
            error: String::new(),
        };

        if self.entries.len() >= MAX_TIMELINE_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        id
    }

    /// Finish an action, updating its after_url, status, and timing.
    pub fn finish_action(&mut self, id: u64, after_url: &str, status: &str, error: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.finished_at = now_epoch_secs();
            entry.after_url = after_url.to_string();
            entry.status = status.to_string();
            if !error.is_empty() {
                entry.error = error.to_string();
            }
        }
    }

    /// Return a snapshot of all entries (cloned).
    pub fn snapshot(&self) -> Vec<ActionEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Look up an entry by ID.
    pub fn get(&self, id: u64) -> Option<&ActionEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Storage for versioned deterministic element refs from page snapshots.
pub struct RefStore {
    pub version: u64,
    pub refs: HashMap<String, Value>,
    pub metadata: Value,
}

impl RefStore {
    pub fn new() -> Self {
        Self {
            version: 0,
            refs: HashMap::new(),
            metadata: Value::Null,
        }
    }
}

/// Stores before/after page state for diff computation.
pub struct DiffState {
    pub before: Option<CompactPageState>,
    pub after: Option<CompactPageState>,
}

impl DiffState {
    pub fn new() -> Self {
        Self {
            before: None,
            after: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_timeline_begin_finish() {
        let mut timeline = ActionTimeline::new();
        let id = timeline.begin_action("navigate", "url=https://example.com", "about:blank");
        assert_eq!(id, 1);

        let entry = timeline.get(id).unwrap();
        assert_eq!(entry.tool, "navigate");
        assert_eq!(entry.status, "running");

        timeline.finish_action(id, "https://example.com", "ok", "");
        let entry = timeline.get(id).unwrap();
        assert_eq!(entry.status, "ok");
        assert_eq!(entry.after_url, "https://example.com");
        assert!(entry.finished_at > 0.0);
    }

    #[test]
    fn action_timeline_fifo_cap() {
        let mut timeline = ActionTimeline::new();
        for i in 0..70 {
            timeline.begin_action("test", &format!("i={i}"), "");
        }
        let snap = timeline.snapshot();
        assert_eq!(snap.len(), MAX_TIMELINE_ENTRIES);
        // Oldest should be evicted — first remaining should be id 11
        assert_eq!(snap[0].id, 11);
        assert_eq!(snap.last().unwrap().id, 70);
    }

    #[test]
    fn daemon_state_construction() {
        let state = DaemonState::new();
        let timeline = state.timeline.lock().unwrap();
        assert_eq!(timeline.snapshot().len(), 0);
    }
}
