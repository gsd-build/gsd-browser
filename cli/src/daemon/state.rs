//! Daemon-side state management for action timeline, versioned refs, diff snapshots,
//! page registry, and frame selection.

use chromiumoxide::Page;
use gsd_browser_common::types::{ActionEntry, CompactPageState};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Mock Route Types ──

/// Whether a route mocks or blocks requests.
#[derive(Debug, Clone, PartialEq)]
pub enum MockType {
    Mock,
    Block,
}

/// A single mock/block route entry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MockRoute {
    pub id: u64,
    pub pattern: String,
    pub route_type: MockType,
    pub status: u16,
    pub body: String,
    pub headers: HashMap<String, String>,
    pub delay_ms: u64,
    pub content_type: String,
}

/// Thread-safe store for active mock/block routes.
pub struct MockRouteStore {
    pub routes: Vec<MockRoute>,
    pub next_id: u64,
    pub fetch_enabled: bool,
    pub listener_spawned: bool,
}

impl MockRouteStore {
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            next_id: 1,
            fetch_enabled: false,
            listener_spawned: false,
        }
    }
}

/// Maximum number of action entries kept in the timeline FIFO.
const MAX_TIMELINE_ENTRIES: usize = 60;

/// Returns the current time as seconds since UNIX_EPOCH (f64).
fn now_epoch_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// An entry in the page registry — one per open browser page/tab.
pub struct PageEntry {
    pub id: u64,
    pub target_id: String,
    pub page: Arc<Page>,
    pub title: String,
    pub url: String,
}

/// Registry tracking all open pages and the currently active one.
pub struct PageRegistry {
    pub entries: Vec<PageEntry>,
    pub active_page_id: u64,
    next_id: u64,
}

impl PageRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            active_page_id: 0,
            next_id: 1,
        }
    }

    /// Register a new page and return its assigned ID.
    pub fn register(&mut self, page: Arc<Page>, title: String, url: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let target_id = page.target_id().as_ref().to_string();
        self.entries.push(PageEntry {
            id,
            target_id,
            page,
            title,
            url,
        });
        // If this is the first page, make it active
        if self.entries.len() == 1 {
            self.active_page_id = id;
        }
        id
    }

    pub fn find_by_target_id(&self, target_id: &str) -> Option<u64> {
        self.entries
            .iter()
            .find(|entry| entry.target_id == target_id)
            .map(|entry| entry.id)
    }

    /// Get the active page (cloned Arc).
    pub fn active_page(&self) -> Option<Arc<Page>> {
        self.entries
            .iter()
            .find(|e| e.id == self.active_page_id)
            .map(|e| Arc::clone(&e.page))
    }

    /// Set the active page ID. Returns false if the ID doesn't exist.
    pub fn set_active(&mut self, id: u64) -> bool {
        if self.entries.iter().any(|e| e.id == id) {
            self.active_page_id = id;
            true
        } else {
            false
        }
    }

    /// Remove a page by ID. Returns Err if it's the last page or ID not found.
    pub fn remove(&mut self, id: u64) -> Result<Arc<Page>, String> {
        if self.entries.len() <= 1 {
            return Err("cannot close the last page".to_string());
        }
        let pos = self
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| format!("page id {id} not found"))?;
        let removed = self.entries.remove(pos);
        // If we removed the active page, fall back to the first remaining page
        if self.active_page_id == id {
            self.active_page_id = self.entries[0].id;
        }
        Ok(removed.page)
    }

    /// Update stored title/url for a page.
    pub fn update_metadata(&mut self, id: u64, title: String, url: String) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.title = title;
            entry.url = url;
        }
    }

    /// Number of open pages.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

// ── Action Cache Types ──

/// A cached selector resolution for a given page structure + intent.
#[derive(Debug, Clone)]
pub struct CachedAction {
    pub selector: String,
    pub score: f64,
    pub cached_at: f64,
}

/// In-memory cache mapping `url_hash:intent` → resolved selector.
pub struct ActionCache {
    pub entries: HashMap<String, CachedAction>,
    pub hits: u64,
    pub misses: u64,
}

impl ActionCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }
}

// ── Trace State ──

/// Tracks whether a CDP trace is currently active.
pub struct TraceState {
    pub active: bool,
    pub name: Option<String>,
    pub started_at: f64,
}

impl TraceState {
    pub fn new() -> Self {
        Self {
            active: false,
            name: None,
            started_at: 0.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionRuntime {
    pub session_name: Option<String>,
    pub launch_mode: String,
    pub cdp_url: Option<String>,
    pub websocket_url: Option<String>,
    pub browser_pid: Option<u32>,
    pub browser_user_data_dir: Option<String>,
    pub identity_scope: Option<gsd_browser_common::identity::IdentityScope>,
    pub identity_project_id: Option<String>,
    pub identity_key: Option<String>,
    pub socket_path: String,
}

/// Top-level daemon state shared across all connections.
pub struct DaemonState {
    pub session: SessionRuntime,
    pub timeline: Mutex<ActionTimeline>,
    pub refs: Mutex<RefStore>,
    pub diff: Mutex<DiffState>,
    pub pages: Mutex<PageRegistry>,
    pub selected_frame: Mutex<Option<String>>,
    pub mock_routes: Mutex<MockRouteStore>,
    pub action_cache: Mutex<ActionCache>,
    pub trace_state: Mutex<TraceState>,
    pub narrator: Arc<crate::daemon::narration::Narrator>,
    pub view_server: tokio::sync::Mutex<Option<crate::daemon::view::ViewServerHandle>>,
    pub view_control: tokio::sync::Mutex<crate::daemon::view::control::SharedControlStore>,
}

impl DaemonState {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::new_with_session(SessionRuntime::default())
    }

    #[cfg(test)]
    pub fn new_with_session(session: SessionRuntime) -> Self {
        Self::new_with_session_and_options(session, false)
    }

    pub fn new_with_session_and_options(session: SessionRuntime, no_narration_delay: bool) -> Self {
        Self {
            session,
            timeline: Mutex::new(ActionTimeline::new()),
            refs: Mutex::new(RefStore::new()),
            diff: Mutex::new(DiffState::new()),
            pages: Mutex::new(PageRegistry::new()),
            selected_frame: Mutex::new(None),
            mock_routes: Mutex::new(MockRouteStore::new()),
            action_cache: Mutex::new(ActionCache::new()),
            trace_state: Mutex::new(TraceState::new()),
            narrator: crate::daemon::narration::Narrator::new(no_narration_delay),
            view_server: tokio::sync::Mutex::new(None),
            view_control: tokio::sync::Mutex::new(
                crate::daemon::view::control::SharedControlStore::new(),
            ),
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
        assert!(state.session.session_name.is_none());
        let timeline = state.timeline.lock().unwrap();
        assert_eq!(timeline.snapshot().len(), 0);
        let pages = state.pages.lock().unwrap();
        assert_eq!(pages.len(), 0);
        let frame = state.selected_frame.lock().unwrap();
        assert!(frame.is_none());
    }

    #[test]
    fn page_registry_register_and_active() {
        // We can't construct a real Page in unit tests, but we can test
        // the registry logic separately with the ID/metadata operations.
        let registry = PageRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.active_page().is_none());
    }

    #[test]
    fn page_registry_set_active_invalid() {
        let mut registry = PageRegistry::new();
        assert!(!registry.set_active(999));
    }
}
