use serde::{Deserialize, Serialize};

// ── Compact Page State ──

/// Compact representation of the current page DOM state.
/// Matches the structure returned by the JS `captureCompactPageState` function.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompactPageState {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub focus: String,
    #[serde(default)]
    pub headings: Vec<String>,
    #[serde(default)]
    pub body_text: String,
    #[serde(default)]
    pub counts: ElementCounts,
    #[serde(default)]
    pub dialog: DialogState,
}

/// Counts of interactive elements on the page.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ElementCounts {
    #[serde(default)]
    pub landmarks: u32,
    #[serde(default)]
    pub buttons: u32,
    #[serde(default)]
    pub links: u32,
    #[serde(default)]
    pub inputs: u32,
}

/// State of any open dialog on the page.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DialogState {
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub title: String,
}

// ── Settle Result ──

/// Result of the adaptive DOM settling algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettleResult {
    pub settle_mode: String,
    pub settle_ms: u64,
    pub settle_reason: String,
    pub settle_polls: u32,
}

impl Default for SettleResult {
    fn default() -> Self {
        Self {
            settle_mode: "adaptive".to_string(),
            settle_ms: 0,
            settle_reason: "timeout_fallback".to_string(),
            settle_polls: 0,
        }
    }
}

// ── Settle Options ──

/// Options controlling the adaptive settle poll loop.
#[derive(Debug, Clone)]
pub struct SettleOptions {
    pub timeout_ms: u64,
    pub poll_ms: u64,
    pub quiet_window_ms: u64,
    pub check_focus_stability: bool,
}

impl Default for SettleOptions {
    fn default() -> Self {
        Self {
            timeout_ms: 500,
            poll_ms: 40,
            quiet_window_ms: 100,
            check_focus_stability: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_page_state_default_roundtrip() {
        let state = CompactPageState::default();
        let json = serde_json::to_string(&state).unwrap();
        let parsed: CompactPageState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.url, "");
        assert_eq!(parsed.title, "");
        assert_eq!(parsed.counts.buttons, 0);
        assert_eq!(parsed.dialog.count, 0);
    }

    #[test]
    fn compact_page_state_deserialize_from_js_output() {
        let js_json = r#"{
            "url": "https://example.com",
            "title": "Example",
            "focus": "input#search",
            "headings": ["Hello World"],
            "bodyText": "Hello World content",
            "counts": {"landmarks": 3, "buttons": 2, "links": 5, "inputs": 1},
            "dialog": {"count": 0, "title": ""}
        }"#;
        let state: CompactPageState = serde_json::from_str(js_json).unwrap();
        assert_eq!(state.url, "https://example.com");
        assert_eq!(state.title, "Example");
        assert_eq!(state.headings.len(), 1);
        assert_eq!(state.counts.landmarks, 3);
        assert_eq!(state.body_text, "Hello World content");
    }

    #[test]
    fn compact_page_state_deserialize_with_missing_fields() {
        // JS might return partial data — all fields should default gracefully
        let partial = r#"{"url": "about:blank"}"#;
        let state: CompactPageState = serde_json::from_str(partial).unwrap();
        assert_eq!(state.url, "about:blank");
        assert_eq!(state.title, "");
        assert!(state.headings.is_empty());
        assert_eq!(state.counts.buttons, 0);
    }

    #[test]
    fn settle_result_default() {
        let r = SettleResult::default();
        assert_eq!(r.settle_mode, "adaptive");
        assert_eq!(r.settle_reason, "timeout_fallback");
    }

    #[test]
    fn settle_result_roundtrip() {
        let r = SettleResult {
            settle_mode: "adaptive".into(),
            settle_ms: 42,
            settle_reason: "zero_mutation_shortcut".into(),
            settle_polls: 1,
        };
        let json = serde_json::to_string(&r).unwrap();
        let parsed: SettleResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.settle_ms, 42);
        assert_eq!(parsed.settle_reason, "zero_mutation_shortcut");
    }
}
