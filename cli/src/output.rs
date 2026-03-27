//! Text and JSON output formatting for CLI commands.
//!
//! Ports `formatCompactStateSummary` from the reference `utils.js` to Rust,
//! and provides per-command text formatters plus JSON/error formatting.

use browser_tools_common::RpcError;
use serde_json::Value;

/// Format compact page state summary — matches the reference formatCompactStateSummary.
///
/// Output format:
/// ```text
/// Title: Example Domain
/// URL: https://example.com
/// Elements: 3 landmarks, 0 buttons, 1 links, 0 inputs
/// Headings: H1 "Example Domain"
/// Focused: input#search
/// Active dialog: "Confirm?"
/// ```
fn format_compact_summary(state: &Value) -> String {
    let mut lines = Vec::new();

    let title = state
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let url = state.get("url").and_then(|v| v.as_str()).unwrap_or("");

    lines.push(format!("Title: {title}"));
    lines.push(format!("URL: {url}"));

    // Element counts
    if let Some(counts) = state.get("counts") {
        let landmarks = counts.get("landmarks").and_then(|v| v.as_u64()).unwrap_or(0);
        let buttons = counts.get("buttons").and_then(|v| v.as_u64()).unwrap_or(0);
        let links = counts.get("links").and_then(|v| v.as_u64()).unwrap_or(0);
        let inputs = counts.get("inputs").and_then(|v| v.as_u64()).unwrap_or(0);
        lines.push(format!(
            "Elements: {landmarks} landmarks, {buttons} buttons, {links} links, {inputs} inputs"
        ));
    }

    // Headings
    if let Some(headings) = state.get("headings").and_then(|v| v.as_array()) {
        if !headings.is_empty() {
            let heading_strs: Vec<String> = headings
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let text = h.as_str().unwrap_or("");
                    format!("H{} \"{}\"", i + 1, text)
                })
                .collect();
            lines.push(format!("Headings: {}", heading_strs.join(", ")));
        }
    }

    // Focus
    if let Some(focus) = state.get("focus").and_then(|v| v.as_str()) {
        if !focus.is_empty() {
            lines.push(format!("Focused: {focus}"));
        }
    }

    // Dialog
    if let Some(dialog) = state.get("dialog") {
        if let Some(dialog_title) = dialog.get("title").and_then(|v| v.as_str()) {
            if !dialog_title.is_empty() {
                lines.push(format!("Active dialog: \"{dialog_title}\""));
            }
        }
    }

    lines.join("\n")
}

/// Format navigate command output in text mode.
pub fn format_text_navigate(result: &Value) -> String {
    let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("");

    let summary = if let Some(state) = result.get("state") {
        format_compact_summary(state)
    } else {
        format_compact_summary(result)
    };

    format!("Navigated to: {url}\nTitle: {title}\n\nPage summary:\n{summary}")
}

/// Format back command output in text mode.
pub fn format_text_back(result: &Value) -> String {
    let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("");

    let summary = if let Some(state) = result.get("state") {
        format_compact_summary(state)
    } else {
        format_compact_summary(result)
    };

    format!("Navigated back to: {url}\nTitle: {title}\n\nPage summary:\n{summary}")
}

/// Format forward command output in text mode.
pub fn format_text_forward(result: &Value) -> String {
    let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("");

    let summary = if let Some(state) = result.get("state") {
        format_compact_summary(state)
    } else {
        format_compact_summary(result)
    };

    format!("Navigated forward to: {url}\nTitle: {title}\n\nPage summary:\n{summary}")
}

/// Format reload command output in text mode.
pub fn format_text_reload(result: &Value) -> String {
    let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("");

    let summary = if let Some(state) = result.get("state") {
        format_compact_summary(state)
    } else {
        format_compact_summary(result)
    };

    format!("Reloaded: {url}\nTitle: {title}\n\nPage summary:\n{summary}")
}

/// Format console log entries in text mode.
pub fn format_text_console(result: &Value) -> String {
    let entries = result.get("entries").and_then(|v| v.as_array());
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

    match entries {
        Some(entries) if !entries.is_empty() => {
            let lines: Vec<String> = entries
                .iter()
                .map(|e| {
                    let ts = e.get("timestamp").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let log_type = e.get("logType").and_then(|v| v.as_str()).unwrap_or("log");
                    let text = e.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    let url = e.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    if url.is_empty() {
                        format!("[{ts:.0}] [{log_type}] {text}")
                    } else {
                        format!("[{ts:.0}] [{log_type}] {text} ({url})")
                    }
                })
                .collect();
            format!("{}\n\n{count} entries", lines.join("\n"))
        }
        _ => format!("No console entries ({count} total)"),
    }
}

/// Format network log entries in text mode.
pub fn format_text_network(result: &Value) -> String {
    let entries = result.get("entries").and_then(|v| v.as_array());
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

    match entries {
        Some(entries) if !entries.is_empty() => {
            let lines: Vec<String> = entries
                .iter()
                .map(|e| {
                    let status = e.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
                    let method = e.get("method").and_then(|v| v.as_str()).unwrap_or("???");
                    let url = e.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let rtype = e
                        .get("resourceType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let failed = e.get("failed").and_then(|v| v.as_bool()).unwrap_or(false);
                    let failure = e
                        .get("failureText")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if failed && !failure.is_empty() {
                        format!("[FAILED] {method} {url} ({rtype}) — {failure}")
                    } else {
                        format!("[{status}] {method} {url} ({rtype})")
                    }
                })
                .collect();
            format!("{}\n\n{count} entries", lines.join("\n"))
        }
        _ => format!("No network entries ({count} total)"),
    }
}

/// Format dialog entries in text mode.
pub fn format_text_dialog(result: &Value) -> String {
    let entries = result.get("entries").and_then(|v| v.as_array());
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

    match entries {
        Some(entries) if !entries.is_empty() => {
            let lines: Vec<String> = entries
                .iter()
                .map(|e| {
                    let dtype = e
                        .get("dialogType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let message = e.get("message").and_then(|v| v.as_str()).unwrap_or("");
                    let url = e.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    if url.is_empty() {
                        format!("[{dtype}] {message}")
                    } else {
                        format!("[{dtype}] {message} ({url})")
                    }
                })
                .collect();
            format!("{}\n\n{count} entries", lines.join("\n"))
        }
        _ => format!("No dialog entries ({count} total)"),
    }
}

/// Format eval result in text mode.
pub fn format_text_eval(result: &Value) -> String {
    result
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Generic formatter for interaction commands (click, type, press, hover, etc.).
/// Shows the action taken plus a compact page summary.
pub fn format_text_interaction(result: &Value) -> String {
    let mut lines = Vec::new();

    // Show what action happened based on the response keys
    if let Some(clicked) = result.get("clicked") {
        if let Some(sel) = clicked.get("selector").and_then(|v| v.as_str()) {
            lines.push(format!("Clicked: {sel}"));
        } else if let (Some(x), Some(y)) = (
            clicked.get("x").and_then(|v| v.as_f64()),
            clicked.get("y").and_then(|v| v.as_f64()),
        ) {
            lines.push(format!("Clicked: ({x}, {y})"));
        }
    }
    if let Some(typed) = result.get("typed") {
        let sel = typed
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let len = typed
            .get("text_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let submitted = typed
            .get("submitted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let slowly = typed
            .get("slowly")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mode = if slowly { "slowly" } else { "atomic" };
        lines.push(format!("Typed: {len} chars into {sel} ({mode})"));
        if submitted {
            lines.push("Submitted: Enter pressed".to_string());
        }
    }
    if let Some(key) = result.get("pressed").and_then(|v| v.as_str()) {
        lines.push(format!("Pressed: {key}"));
    }
    if let Some(sel) = result.get("hovered").and_then(|v| v.as_str()) {
        lines.push(format!("Hovered: {sel}"));
    }
    if let Some(selected) = result.get("selected") {
        let sel = selected
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let opt = selected
            .get("option")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        lines.push(format!("Selected: \"{opt}\" in {sel}"));
    }
    if let Some(checked) = result.get("checked") {
        let sel = checked
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let val = checked
            .get("value")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        lines.push(format!("Checked: {sel} = {val}"));
    }
    if let Some(dragged) = result.get("dragged") {
        let src = dragged
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tgt = dragged
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        lines.push(format!("Dragged: {src} → {tgt}"));
    }
    if let Some(uploaded) = result.get("uploaded") {
        let sel = uploaded
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let files = uploaded
            .get("files")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        lines.push(format!("Uploaded: {files} file(s) to {sel}"));
    }

    // Show page summary
    if let Some(state) = result.get("state") {
        lines.push(String::new());
        lines.push("Page summary:".to_string());
        lines.push(format_compact_summary(state));
    }

    lines.join("\n")
}

/// Format scroll command output in text mode.
pub fn format_text_scroll(result: &Value) -> String {
    let mut lines = Vec::new();

    if let Some(scroll) = result.get("scroll") {
        let y = scroll.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let pct = scroll
            .get("percentage")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let height = scroll
            .get("height")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let vp = scroll
            .get("viewport_height")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        lines.push(format!("Scroll position: {y}px ({pct}%)"));
        lines.push(format!(
            "Page height: {height}px, viewport: {vp}px"
        ));
    }

    if let Some(state) = result.get("state") {
        lines.push(String::new());
        lines.push("Page summary:".to_string());
        lines.push(format_compact_summary(state));
    }

    lines.join("\n")
}

/// Format set_viewport command output in text mode.
pub fn format_text_viewport(result: &Value) -> String {
    let width = result.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
    let height = result.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
    let preset = result
        .get("preset")
        .and_then(|v| v.as_str())
        .unwrap_or("custom");
    format!("Viewport: {width}x{height} ({preset})")
}

/// Format any result as pretty-printed JSON.
pub fn format_json(result: &Value) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".to_string())
}

/// Format an RPC error in text mode.
pub fn format_error_text(err: &RpcError) -> String {
    let mut out = format!("Error: {}", err.message);
    if let Some(data) = &err.data {
        if let Some(hint) = data.get("retryHint").and_then(|v| v.as_str()) {
            out.push_str(&format!("\nHint: {hint}"));
        }
    }
    out
}

/// Format an RPC error as JSON.
pub fn format_error_json(err: &RpcError) -> String {
    let mut obj = serde_json::json!({
        "error": {
            "code": err.code,
            "message": err.message,
        }
    });
    if let Some(data) = &err.data {
        obj["error"]["data"] = data.clone();
    }
    serde_json::to_string_pretty(&obj).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_compact_summary_full() {
        let state = serde_json::json!({
            "url": "https://example.com",
            "title": "Example Domain",
            "focus": "",
            "headings": ["Example Domain"],
            "counts": {"landmarks": 3, "buttons": 0, "links": 1, "inputs": 0},
            "dialog": {"count": 0, "title": ""},
        });
        let summary = format_compact_summary(&state);
        assert!(summary.contains("Title: Example Domain"));
        assert!(summary.contains("URL: https://example.com"));
        assert!(summary.contains("Elements: 3 landmarks, 0 buttons, 1 links, 0 inputs"));
        assert!(summary.contains("H1 \"Example Domain\""));
        // Empty focus should NOT appear
        assert!(!summary.contains("Focused:"));
    }

    #[test]
    fn test_format_compact_summary_with_focus_and_dialog() {
        let state = serde_json::json!({
            "url": "https://app.example.com",
            "title": "App",
            "focus": "input#search",
            "headings": ["Welcome", "About"],
            "counts": {"landmarks": 2, "buttons": 5, "links": 10, "inputs": 3},
            "dialog": {"count": 1, "title": "Confirm action?"},
        });
        let summary = format_compact_summary(&state);
        assert!(summary.contains("Focused: input#search"));
        assert!(summary.contains("Active dialog: \"Confirm action?\""));
        assert!(summary.contains("H1 \"Welcome\", H2 \"About\""));
    }

    #[test]
    fn test_format_text_navigate() {
        let result = serde_json::json!({
            "url": "https://example.com",
            "title": "Example Domain",
            "state": {
                "url": "https://example.com",
                "title": "Example Domain",
                "headings": ["Example Domain"],
                "counts": {"landmarks": 1, "buttons": 0, "links": 1, "inputs": 0},
                "focus": "",
                "dialog": {"count": 0, "title": ""},
            },
        });
        let text = format_text_navigate(&result);
        assert!(text.starts_with("Navigated to: https://example.com"));
        assert!(text.contains("Page summary:"));
    }

    #[test]
    fn test_format_error_text_with_hint() {
        let err = RpcError {
            code: -32603,
            message: "navigation timed out".to_string(),
            data: Some(serde_json::json!({"retryHint": "Check URL is valid"})),
        };
        let text = format_error_text(&err);
        assert!(text.contains("Error: navigation timed out"));
        assert!(text.contains("Hint: Check URL is valid"));
    }

    #[test]
    fn test_format_error_json() {
        let err = RpcError {
            code: -32603,
            message: "something broke".to_string(),
            data: None,
        };
        let json_str = format_error_json(&err);
        let parsed: Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["error"]["message"], "something broke");
    }
}
