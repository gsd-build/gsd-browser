//! Text and JSON output formatting for CLI commands.
//!
//! Ports `formatCompactStateSummary` from the reference `utils.js` to Rust,
//! and provides per-command text formatters plus JSON/error formatting.

use gsd_browser_common::RpcError;
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

/// Format screenshot command output in text mode (metadata only, no base64 dump).
pub fn format_text_screenshot(result: &Value) -> String {
    let width = result.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
    let height = result.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
    let mime = result
        .get("mimeType")
        .and_then(|v| v.as_str())
        .unwrap_or("image/jpeg");
    let scope = result
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("viewport");
    let byte_len = result
        .get("byteLength")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut out = format!("Screenshot: {width}x{height} {mime} ({scope})");
    if byte_len > 0 {
        let kb = byte_len as f64 / 1024.0;
        out.push_str(&format!(" — {kb:.1} KB"));
    }
    out.push_str("\nUse --json to get base64 data, or --output <path> to save to file");
    out
}

/// Format accessibility tree in text mode — print the indented tree with a node count footer.
pub fn format_text_accessibility_tree(result: &Value) -> String {
    let tree = result
        .get("tree")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let node_count = result
        .get("nodeCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let truncated = result
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        return format!("Error: {error}");
    }

    let mut out = tree.to_string();
    out.push_str(&format!("\n\n{node_count} nodes"));
    if truncated {
        out.push_str(" (truncated — use --max-count to increase)");
    }
    out
}

/// Format find results in text mode — one element per line: `[role] name (selector_hint)`.
pub fn format_text_find(result: &Value) -> String {
    let elements = result.get("elements").and_then(|v| v.as_array());
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    let truncated = result
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        return format!("Error: {error}");
    }

    match elements {
        Some(elements) if !elements.is_empty() => {
            let lines: Vec<String> = elements
                .iter()
                .map(|e| {
                    let role = e.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    let name = e.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let hint = e
                        .get("selector_hint")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let tag = e.get("tag").and_then(|v| v.as_str()).unwrap_or("");
                    let visible = e
                        .get("visible")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);

                    let role_display = if role.is_empty() { tag } else { role };
                    let vis = if !visible { " [hidden]" } else { "" };

                    if hint.is_empty() {
                        format!("[{role_display}] {name}{vis}")
                    } else {
                        format!("[{role_display}] {name} ({hint}){vis}")
                    }
                })
                .collect();

            let mut out = lines.join("\n");
            out.push_str(&format!("\n\n{count} elements"));
            if truncated {
                out.push_str(" (truncated — use --limit to increase)");
            }
            out
        }
        _ => "No matching elements found".to_string(),
    }
}

/// Format page source in text mode — print HTML directly (truncated to 10KB in text mode).
pub fn format_text_page_source(result: &Value) -> String {
    let html = result.get("html").and_then(|v| v.as_str()).unwrap_or("");
    let length = result.get("length").and_then(|v| v.as_u64()).unwrap_or(0);
    let truncated = result
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // In text mode, further truncate to 10KB for terminal readability
    const TEXT_MAX: usize = 10 * 1024;
    if html.len() > TEXT_MAX {
        let truncated_html = &html[..TEXT_MAX];
        format!(
            "{truncated_html}\n\n... [truncated at 10KB of {length} bytes — use --json for full output]"
        )
    } else if truncated {
        format!("{html}\n\n... [truncated at 200KB of {length} bytes]")
    } else {
        html.to_string()
    }
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

/// Format wait-for result in text mode.
pub fn format_text_wait_for(result: &Value) -> String {
    let condition = result
        .get("condition")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let met = result.get("met").and_then(|v| v.as_bool()).unwrap_or(false);
    let elapsed = result
        .get("elapsed_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let timeout = result
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let value = result
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let status = if met { "✓ met" } else { "✗ timeout" };

    let mut out = format!("Wait: {condition} {status} ({elapsed}ms / {timeout}ms timeout)");
    if !value.is_empty() {
        out.push_str(&format!("\nValue: {value}"));
    }
    out
}

/// Format timeline result in text mode — table of action entries.
pub fn format_text_timeline(result: &Value) -> String {
    let entries = result.get("entries").and_then(|v| v.as_array());
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

    match entries {
        Some(entries) if !entries.is_empty() => {
            let mut lines = vec![format!(
                "{:<4} {:<14} {:<10} {:<30}",
                "ID", "Tool", "Status", "Params"
            )];
            lines.push("-".repeat(60));
            for e in entries {
                let id = e.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let tool = e
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let status = e
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let params = e
                    .get("paramsSummary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let display_params = if params.len() > 30 {
                    format!("{}…", &params[..29])
                } else {
                    params.to_string()
                };
                lines.push(format!(
                    "{:<4} {:<14} {:<10} {:<30}",
                    id, tool, status, display_params
                ));
            }
            lines.push(format!("\n{count} entries"));
            lines.join("\n")
        }
        _ => format!("No timeline entries ({count} total)"),
    }
}

/// Format snapshot result in text mode — version, count, per-ref summary.
pub fn format_text_snapshot(result: &Value) -> String {
    let version = result.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut lines = vec![format!("Snapshot v{version}: {count} elements")];

    if let Some(refs) = result.get("refs").and_then(|v| v.as_object()) {
        // Sort keys numerically (e1, e2, ...)
        let mut keys: Vec<&String> = refs.keys().collect();
        keys.sort_by(|a, b| {
            let a_num: u64 = a.trim_start_matches('e').parse().unwrap_or(0);
            let b_num: u64 = b.trim_start_matches('e').parse().unwrap_or(0);
            a_num.cmp(&b_num)
        });

        for key in keys {
            if let Some(node) = refs.get(key) {
                let tag = node.get("tag").and_then(|v| v.as_str()).unwrap_or("?");
                let role = node.get("role").and_then(|v| v.as_str()).unwrap_or("");
                let name = node.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let visible = node.get("visible").and_then(|v| v.as_bool()).unwrap_or(false);

                let role_display = if role.is_empty() {
                    tag.to_string()
                } else {
                    format!("{tag}[{role}]")
                };

                let vis = if !visible { " [hidden]" } else { "" };
                let name_display = if name.is_empty() {
                    String::new()
                } else {
                    let truncated = if name.len() > 40 {
                        format!("{}…", &name[..39])
                    } else {
                        name.to_string()
                    };
                    format!(" \"{truncated}\"")
                };

                lines.push(format!(
                    "  @v{version}:{key} {role_display}{name_display}{vis}"
                ));
            }
        }
    }

    lines.join("\n")
}

/// Format get-ref result in text mode — ref metadata.
pub fn format_text_get_ref(result: &Value) -> String {
    let ref_str = result.get("ref").and_then(|v| v.as_str()).unwrap_or("?");
    let tag = result.get("tag").and_then(|v| v.as_str()).unwrap_or("?");
    let role = result.get("role").and_then(|v| v.as_str()).unwrap_or("");
    let name = result.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let visible = result.get("visible").and_then(|v| v.as_bool()).unwrap_or(false);
    let enabled = result.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);

    let mut lines = vec![format!("Ref: {ref_str}")];
    lines.push(format!("Tag: {tag}"));
    if !role.is_empty() {
        lines.push(format!("Role: {role}"));
    }
    if !name.is_empty() {
        lines.push(format!("Name: {name}"));
    }
    lines.push(format!("Visible: {visible}"));
    lines.push(format!("Enabled: {enabled}"));

    // Show full node details if present
    if let Some(node) = result.get("node") {
        if let Some(hints) = node.get("selectorHints").and_then(|v| v.as_array()) {
            let hint_strs: Vec<&str> = hints.iter().filter_map(|v| v.as_str()).collect();
            if !hint_strs.is_empty() {
                lines.push(format!("Hints: {}", hint_strs.join(", ")));
            }
        }
        if let Some(heading) = node.get("nearestHeading").and_then(|v| v.as_str()) {
            if !heading.is_empty() {
                lines.push(format!("Nearest heading: {heading}"));
            }
        }
        if let Some(form) = node.get("formOwnership").and_then(|v| v.as_str()) {
            if !form.is_empty() {
                lines.push(format!("Form: {form}"));
            }
        }
    }

    lines.join("\n")
}

/// Format ref action result (click-ref, hover-ref, fill-ref) in text mode.
/// Shows ref resolution info plus the underlying interaction result.
pub fn format_text_ref_action(result: &Value) -> String {
    let mut lines = Vec::new();

    // Show ref resolution info
    if let Some(res) = result.get("ref_resolution") {
        let ref_str = res.get("ref").and_then(|v| v.as_str()).unwrap_or("?");
        let selector = res
            .get("resolved_selector")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let tier = res.get("tier").and_then(|v| v.as_u64()).unwrap_or(0);
        let tag = res.get("tag").and_then(|v| v.as_str()).unwrap_or("");
        let name = res.get("name").and_then(|v| v.as_str()).unwrap_or("");

        let tier_label = match tier {
            1 => "domPath",
            2 => "selectorHint",
            3 => "role+name",
            4 => "fingerprint",
            _ => "unknown",
        };

        lines.push(format!(
            "Ref: {ref_str} → {selector} (tier {tier}: {tier_label})"
        ));
        if !tag.is_empty() || !name.is_empty() {
            lines.push(format!("Element: <{tag}> \"{name}\""));
        }
    }

    // Delegate to interaction formatter for the rest
    let interaction_text = format_text_interaction(result);
    if !interaction_text.is_empty() {
        lines.push(interaction_text);
    }

    lines.join("\n")
}

/// Format assert result in text mode — verified/failed with per-check results.
pub fn format_text_assert(result: &Value) -> String {
    let verified = result
        .get("verified")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let summary = result
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let status = if verified { "✓ VERIFIED" } else { "✗ FAILED" };
    let mut lines = vec![format!("Assert: {status} — {summary}")];

    if let Some(checks) = result.get("checks").and_then(|v| v.as_array()) {
        for check in checks {
            let kind = check.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let passed = check
                .get("passed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let expected = check
                .get("expected")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let actual = check
                .get("actual")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mark = if passed { "✓" } else { "✗" };
            let selector = check
                .get("selector")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let sel_part = if selector.is_empty() {
                String::new()
            } else {
                format!(" [{selector}]")
            };
            lines.push(format!("  {mark} {kind}: expected={expected}, actual={actual}{sel_part}"));
        }
    }

    if let Some(hint) = result.get("agentHint").and_then(|v| v.as_str()) {
        lines.push(format!("\nHint: {hint}"));
    }

    lines.join("\n")
}

/// Format diff result in text mode — changed/unchanged with change list.
pub fn format_text_diff(result: &Value) -> String {
    let changed = result
        .get("changed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let summary = result
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let status = if changed { "CHANGED" } else { "UNCHANGED" };
    let mut lines = vec![format!("Diff: {status} — {summary}")];

    if let Some(changes) = result.get("changes").and_then(|v| v.as_array()) {
        for change in changes {
            let field = change
                .get("field")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let before = change.get("before").map(|v| {
                if let Some(s) = v.as_str() {
                    s.to_string()
                } else {
                    v.to_string()
                }
            }).unwrap_or_default();
            let after = change.get("after").map(|v| {
                if let Some(s) = v.as_str() {
                    s.to_string()
                } else {
                    v.to_string()
                }
            }).unwrap_or_default();
            let before_display = if before.len() > 50 {
                format!("{}…", &before[..49])
            } else {
                before
            };
            let after_display = if after.len() > 50 {
                format!("{}…", &after[..49])
            } else {
                after
            };
            lines.push(format!("  {field}: {before_display} → {after_display}"));
        }
    }

    lines.join("\n")
}

/// Format batch result in text mode — step-by-step results with summary.
pub fn format_text_batch(result: &Value) -> String {
    let total = result
        .get("totalSteps")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let passed = result
        .get("passedSteps")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut lines = vec![format!("Batch: {passed}/{total} steps passed")];

    // Per-step results
    if let Some(steps) = result.get("steps").and_then(|v| v.as_array()) {
        for step in steps {
            let index = step.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            let action = step
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let status = step
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let mark = if status == "pass" { "✓" } else { "✗" };

            let detail = if status == "fail" {
                step.get("error")
                    .and_then(|v| v.as_str())
                    .map(|e| format!(" — {e}"))
                    .unwrap_or_default()
            } else {
                String::new()
            };

            lines.push(format!("  {mark} [{index}] {action}{detail}"));
        }
    }

    // Failed step info
    if let Some(fs) = result.get("failedStep") {
        let action = fs.get("action").and_then(|v| v.as_str()).unwrap_or("?");
        let index = fs.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
        let error = fs.get("error").and_then(|v| v.as_str()).unwrap_or("");
        lines.push(format!("\n⚠ Stopped at step [{index}] {action}: {error}"));
    }

    // Final summary
    if let Some(summary) = result.get("finalSummary") {
        lines.push(String::new());
        lines.push("Final page state:".to_string());
        lines.push(format_compact_summary(summary));
    }

    lines.join("\n")
}

/// Format list-pages result in text mode — one line per page.
pub fn format_text_list_pages(result: &Value) -> String {
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    let active_id = result
        .get("activePageId")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let pages = result.get("pages").and_then(|v| v.as_array());
    match pages {
        Some(pages) if !pages.is_empty() => {
            let lines: Vec<String> = pages
                .iter()
                .map(|p| {
                    let id = p.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                    let title = p.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let url = p.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let marker = if id == active_id { " *" } else { "" };
                    let title_display = if title.is_empty() {
                        String::new()
                    } else {
                        format!(" \"{title}\"")
                    };
                    format!("  [{id}]{marker}{title_display} — {url}")
                })
                .collect();
            format!("Pages ({count}):\n{}", lines.join("\n"))
        }
        _ => "No pages open".to_string(),
    }
}

/// Format switch-page result in text mode.
pub fn format_text_switch_page(result: &Value) -> String {
    let id = result.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
    let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
    format!("Switched to page [{id}]: {url}\nTitle: {title}")
}

/// Format close-page result in text mode.
pub fn format_text_close_page(result: &Value) -> String {
    let id = result.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
    let active_id = result
        .get("activePageId")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    format!("Closed page [{id}]. Active page: [{active_id}]")
}

/// Format list-frames result in text mode — one line per frame.
pub fn format_text_list_frames(result: &Value) -> String {
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    let frames = result.get("frames").and_then(|v| v.as_array());

    match frames {
        Some(frames) if !frames.is_empty() => {
            let lines: Vec<String> = frames
                .iter()
                .map(|f| {
                    let index = f.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                    let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let url = f.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let is_main = f.get("isMain").and_then(|v| v.as_bool()).unwrap_or(false);
                    let parent = f
                        .get("parentName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let marker = if is_main { " [main]" } else { "" };
                    let name_display = if name.is_empty() {
                        String::new()
                    } else {
                        format!(" name=\"{name}\"")
                    };
                    let parent_display = if parent.is_empty() || is_main {
                        String::new()
                    } else {
                        format!(" parent=\"{parent}\"")
                    };
                    format!("  [{index}]{marker}{name_display} — {url}{parent_display}")
                })
                .collect();
            format!("Frames ({count}):\n{}", lines.join("\n"))
        }
        _ => "No frames found".to_string(),
    }
}

/// Format select-frame result in text mode.
pub fn format_text_select_frame(result: &Value) -> String {
    let selected = result
        .get("selected")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let frame = result
        .get("frame")
        .and_then(|v| v.as_str())
        .unwrap_or("main");
    if selected {
        format!("Selected frame: {frame}")
    } else {
        "Reset to main frame".to_string()
    }
}

/// Format analyze-form result — list fields with labels, types, and submit buttons.
pub fn format_text_analyze_form(result: &Value) -> String {
    let form_sel = result
        .get("formSelector")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let field_count = result
        .get("fieldCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut lines = vec![format!("Form: {form_sel} ({field_count} fields)")];

    if let Some(fields) = result.get("fields").and_then(|v| v.as_array()) {
        for f in fields {
            let label = f.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let ftype = f.get("type").and_then(|v| v.as_str()).unwrap_or("text");
            let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let required = f.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
            let hidden = f.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false);
            let disabled = f.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false);
            let value = f.get("value").and_then(|v| v.as_str()).unwrap_or("");

            let mut flags = Vec::new();
            if required {
                flags.push("required");
            }
            if hidden {
                flags.push("hidden");
            }
            if disabled {
                flags.push("disabled");
            }
            let flag_str = if flags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", flags.join(", "))
            };
            let name_str = if name.is_empty() {
                String::new()
            } else {
                format!(" name=\"{name}\"")
            };
            let val_str = if value.is_empty() {
                String::new()
            } else {
                format!(" value=\"{value}\"")
            };
            let label_display = if label.is_empty() {
                "(no label)".to_string()
            } else {
                format!("\"{label}\"")
            };

            lines.push(format!(
                "  {label_display} [{ftype}]{name_str}{val_str}{flag_str}"
            ));

            // Show select options
            if let Some(opts) = f.get("options").and_then(|v| v.as_array()) {
                let opt_strs: Vec<String> = opts
                    .iter()
                    .take(5)
                    .map(|o| {
                        let olabel = o.get("label").and_then(|v| v.as_str()).unwrap_or("");
                        let selected = o.get("selected").and_then(|v| v.as_bool()).unwrap_or(false);
                        if selected {
                            format!("*{olabel}")
                        } else {
                            olabel.to_string()
                        }
                    })
                    .collect();
                let more = if opts.len() > 5 {
                    format!(" +{} more", opts.len() - 5)
                } else {
                    String::new()
                };
                lines.push(format!("    options: {}{more}", opt_strs.join(", ")));
            }

            // Show checked state for checkbox/radio
            if let Some(checked) = f.get("checked").and_then(|v| v.as_bool()) {
                lines.push(format!("    checked: {checked}"));
            }
        }
    }

    if let Some(buttons) = result.get("submitButtons").and_then(|v| v.as_array()) {
        if !buttons.is_empty() {
            lines.push("Submit buttons:".to_string());
            for b in buttons {
                let text = b.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let btype = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let sel = b.get("selector").and_then(|v| v.as_str()).unwrap_or("");
                lines.push(format!("  \"{text}\" [{btype}] → {sel}"));
            }
        }
    }

    lines.join("\n")
}

/// Format fill-form result — show filled fields, errors, and submission status.
pub fn format_text_fill_form(result: &Value) -> String {
    let filled = result
        .get("filled")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let errors = result.get("errors").and_then(|v| v.as_array());
    let unresolved = result.get("unresolved").and_then(|v| v.as_array());
    let submitted = result
        .get("submitted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut lines = vec![format!("Filled {filled} field(s)")];

    if let Some(filled_fields) = result.get("filled").and_then(|v| v.as_array()) {
        for f in filled_fields {
            if let Some(name) = f.as_str() {
                lines.push(format!("  ✓ {name}"));
            }
        }
    }

    if let Some(errs) = errors {
        if !errs.is_empty() {
            lines.push("Fill errors:".to_string());
            for e in errs {
                if let Some(msg) = e.as_str() {
                    lines.push(format!("  ✗ {msg}"));
                }
            }
        }
    }

    if let Some(unres) = unresolved {
        if !unres.is_empty() {
            let names: Vec<&str> = unres.iter().filter_map(|v| v.as_str()).collect();
            lines.push(format!("Unresolved fields: {}", names.join(", ")));
        }
    }

    if submitted {
        lines.push("Form submitted".to_string());
    }

    // Append compact page state summary if available
    if let Some(state) = result.get("state") {
        lines.push(String::new());
        lines.push("Page summary:".to_string());
        lines.push(format_compact_summary(state));
    }

    lines.join("\n")
}

/// Format find-best result — show scored candidates for an intent.
pub fn format_text_find_best(result: &Value) -> String {
    let intent = result
        .get("intent")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let count = result
        .get("candidateCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let scope = result
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("document");

    let mut lines = vec![format!(
        "Intent: {intent} — {count} candidate(s) (scope: {scope})"
    )];

    if let Some(candidates) = result.get("candidates").and_then(|v| v.as_array()) {
        for (i, c) in candidates.iter().enumerate() {
            let score = c.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let selector = c.get("selector").and_then(|v| v.as_str()).unwrap_or("?");
            let tag = c.get("tag").and_then(|v| v.as_str()).unwrap_or("");
            let text = c
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(50)
                .collect::<String>();
            let reason = c.get("reason").and_then(|v| v.as_str()).unwrap_or("");

            let text_display = if text.is_empty() {
                String::new()
            } else {
                format!(" \"{text}\"")
            };
            lines.push(format!(
                "  {}. [{score:.3}] <{tag}>{text_display} → {selector}",
                i + 1
            ));
            if !reason.is_empty() {
                lines.push(format!("     reason: {reason}"));
            }
        }
    }

    lines.join("\n")
}

/// Format act result — show the action performed on the top intent candidate.
pub fn format_text_act(result: &Value) -> String {
    let intent = result
        .get("intent")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let action = result
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let score = result
        .get("score")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let mut lines = vec![format!(
        "Act: {action} for intent '{intent}' (score: {score:.3})"
    )];

    if let Some(candidate) = result.get("candidate") {
        let selector = candidate
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let tag = candidate
            .get("tag")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let text = candidate
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let reason = candidate
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        lines.push(format!("  Element: <{tag}> → {selector}"));
        if !text.is_empty() {
            lines.push(format!("  Text: \"{text}\""));
        }
        if !reason.is_empty() {
            lines.push(format!("  Reason: {reason}"));
        }
    }

    // Append compact page state summary if available
    if let Some(state) = result.get("state") {
        lines.push(String::new());
        lines.push("Page summary:".to_string());
        lines.push(format_compact_summary(state));
    }

    lines.join("\n")
}

/// Format session-summary result — structured diagnostic snapshot.
pub fn format_text_session_summary(result: &Value) -> String {
    let mut lines = Vec::new();

    // Active page
    if let Some(page) = result.get("activePage") {
        let url = page.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let title = page.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let id = page.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        lines.push(format!("Active page [{id}]: {url}"));
        if !title.is_empty() {
            lines.push(format!("  Title: {title}"));
        }
    }

    let page_count = result
        .get("pageCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    lines.push(format!("Pages: {page_count}"));

    // Selected frame
    if let Some(frame) = result.get("selectedFrame").and_then(|v| v.as_str()) {
        lines.push(format!("Selected frame: {frame}"));
    }

    // Actions
    if let Some(actions) = result.get("actions") {
        let total = actions.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        let ok = actions.get("ok").and_then(|v| v.as_u64()).unwrap_or(0);
        let error = actions.get("error").and_then(|v| v.as_u64()).unwrap_or(0);
        let running = actions.get("running").and_then(|v| v.as_u64()).unwrap_or(0);
        let waits = actions
            .get("waitCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let asserts = actions
            .get("assertCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        lines.push(format!(
            "Actions: {total} total ({ok} ok, {error} error, {running} running)"
        ));
        lines.push(format!("  Waits: {waits}, Asserts: {asserts}"));
    }

    // Console
    if let Some(console) = result.get("console") {
        let total = console.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        let errors = console
            .get("errors")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let error_flag = if errors > 0 { " ⚠" } else { "" };
        lines.push(format!("Console: {total} entries, {errors} errors{error_flag}"));
    }

    // Network
    if let Some(network) = result.get("network") {
        let total = network.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        let failed = network
            .get("failed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let fail_flag = if failed > 0 { " ⚠" } else { "" };
        lines.push(format!("Network: {total} requests, {failed} failed{fail_flag}"));
    }

    // Dialog
    if let Some(dialog) = result.get("dialog") {
        let total = dialog.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        if total > 0 {
            lines.push(format!("Dialogs: {total}"));
        }
    }

    // Bounded history caveat
    let bounded = result
        .get("boundedHistory")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if bounded {
        let caveat = result
            .get("boundedHistoryCaveat")
            .and_then(|v| v.as_str())
            .unwrap_or("Timeline history capped");
        lines.push(format!("⚠ {caveat}"));
    }

    lines.join("\n")
}

/// Format debug-bundle result — show path and files written.
pub fn format_text_debug_bundle(result: &Value) -> String {
    let path = result
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let file_count = result
        .get("fileCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut lines = vec![format!("Debug bundle: {path}")];
    lines.push(format!("Files: {file_count}"));

    if let Some(files) = result.get("files").and_then(|v| v.as_array()) {
        for f in files {
            if let Some(name) = f.as_str() {
                lines.push(format!("  • {name}"));
            }
        }
    }

    lines.join("\n")
}

/// Format visual diff result in text mode.
pub fn format_text_visual_diff(result: &Value) -> String {
    let status = result
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let similarity = result
        .get("similarity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let diff_count = result
        .get("diffPixelCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let baseline_path = result
        .get("baselinePath")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let mut lines = vec![format!("Visual diff: {status}")];
    lines.push(format!("Similarity: {:.2}%", similarity * 100.0));
    lines.push(format!("Diff pixels: {diff_count}"));
    lines.push(format!("Baseline: {baseline_path}"));

    if let Some(diff_path) = result.get("diffPath").and_then(|v| v.as_str()) {
        lines.push(format!("Diff image: {diff_path}"));
    }

    if let Some(w) = result.get("width").and_then(|v| v.as_u64()) {
        let h = result.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
        lines.push(format!("Size: {w}x{h}"));
    }

    lines.join("\n")
}

/// Format zoom region result in text mode.
pub fn format_text_zoom_region(result: &Value) -> String {
    let width = result
        .get("width")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let height = result
        .get("height")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let scale = result
        .get("scale")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let byte_len = result
        .get("byteLength")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut lines = vec![format!("Zoom region captured: {width}x{height} (scale: {scale}x)")];
    lines.push(format!("Size: {} bytes", byte_len));

    if let Some(region) = result.get("region") {
        let rx = region.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let ry = region.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let rw = region.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let rh = region.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
        lines.push(format!("Region: ({rx}, {ry}) {rw}x{rh}"));
    }

    // Include base64 data hint (truncated for readability)
    if let Some(data) = result.get("data").and_then(|v| v.as_str()) {
        if data.len() > 40 {
            lines.push(format!("Data: {}... ({} chars)", &data[..40], data.len()));
        } else {
            lines.push(format!("Data: {data}"));
        }
    }

    lines.join("\n")
}

/// Format save PDF result in text mode.
pub fn format_text_save_pdf(result: &Value) -> String {
    let path = result
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let format_str = result
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let byte_len = result
        .get("byteLength")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let paper_size = result
        .get("paperSize")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let kb = byte_len as f64 / 1024.0;
    format!("PDF saved: {path}\nFormat: {format_str} ({paper_size})\nSize: {kb:.1} KB ({byte_len} bytes)")
}

/// Format extract result in text mode.
pub fn format_text_extract(result: &Value) -> String {
    let multiple = result
        .get("multiple")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if multiple {
        let count = result
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mut lines = vec![format!("Extracted {count} items")];

        if let Some(data) = result.get("data").and_then(|v| v.as_array()) {
            for (i, item) in data.iter().enumerate().take(10) {
                lines.push(format!("  [{i}]: {}", serde_json::to_string(item).unwrap_or_default()));
            }
            if data.len() > 10 {
                lines.push(format!("  ... and {} more", data.len() - 10));
            }
        }

        lines.join("\n")
    } else {
        let field_count = result
            .get("fieldCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mut lines = vec![format!("Extracted {field_count} fields")];

        if let Some(data) = result.get("data").and_then(|v| v.as_object()) {
            for (key, val) in data {
                let val_str = match val {
                    Value::String(s) => {
                        if s.len() > 80 {
                            format!("\"{}...\"", &s[..77])
                        } else {
                            format!("\"{s}\"")
                        }
                    }
                    Value::Null => "null".to_string(),
                    _ => val.to_string(),
                };
                lines.push(format!("  {key}: {val_str}"));
            }
        }

        lines.join("\n")
    }
}

pub fn format_text_mock_route(result: &Value) -> String {
    let route_id = result
        .get("route_id")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let pattern = result
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let status = result
        .get("status")
        .and_then(|v| v.as_u64())
        .unwrap_or(200);
    format!("Mock route #{route_id} added: {pattern} → {status}")
}

pub fn format_text_block_urls(result: &Value) -> String {
    let count = result
        .get("blocked")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let patterns = result
        .get("patterns")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    format!("Blocked {count} URL pattern(s): {patterns}")
}

pub fn format_text_clear_routes(result: &Value) -> String {
    let cleared = result
        .get("cleared")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    format!("Cleared {cleared} route(s)")
}

pub fn format_text_emulate_device(result: &Value) -> String {
    // Handle "list" response
    if let Some(devices) = result.get("devices").and_then(|v| v.as_array()) {
        let mut lines = vec![format!("Available devices ({}):", devices.len())];
        for d in devices {
            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let w = d.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
            let h = d.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
            let scale = d.get("deviceScaleFactor").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let mobile = d.get("mobile").and_then(|v| v.as_bool()).unwrap_or(false);
            lines.push(format!(
                "  {name}: {w}x{h} @{scale}x{}",
                if mobile { " (mobile)" } else { "" }
            ));
        }
        return lines.join("\n");
    }

    let device = result.get("device").and_then(|v| v.as_str()).unwrap_or("?");
    let w = result.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
    let h = result.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
    let scale = result
        .get("deviceScaleFactor")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let mobile = result
        .get("mobile")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    format!(
        "Emulating: {device} ({w}x{h} @{scale}x{})",
        if mobile { ", mobile" } else { "" }
    )
}

pub fn format_text_save_state(result: &Value) -> String {
    let name = result.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let path = result.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    let cookies = result.get("cookies").and_then(|v| v.as_u64()).unwrap_or(0);
    let ls = result.get("localStorage").and_then(|v| v.as_u64()).unwrap_or(0);
    let ss = result.get("sessionStorage").and_then(|v| v.as_u64()).unwrap_or(0);
    format!(
        "State '{name}' saved → {path}\n  {cookies} cookies, {ls} localStorage, {ss} sessionStorage"
    )
}

pub fn format_text_restore_state(result: &Value) -> String {
    let name = result.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let cookies = result.get("cookies").and_then(|v| v.as_u64()).unwrap_or(0);
    let ls = result.get("localStorage").and_then(|v| v.as_u64()).unwrap_or(0);
    let ss = result.get("sessionStorage").and_then(|v| v.as_u64()).unwrap_or(0);
    format!(
        "State '{name}' restored: {cookies} cookies, {ls} localStorage, {ss} sessionStorage"
    )
}

pub fn format_text_vault_save(result: &Value) -> String {
    let profile = result.get("profile").and_then(|v| v.as_str()).unwrap_or("?");
    let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("?");
    let username = result.get("username").and_then(|v| v.as_str()).unwrap_or("?");
    format!("Vault profile '{profile}' saved (user: {username}, url: {url})")
}

pub fn format_text_vault_login(result: &Value) -> String {
    let profile = result.get("profile").and_then(|v| v.as_str()).unwrap_or("?");
    let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("?");
    let logged_in = result.get("logged_in").and_then(|v| v.as_bool()).unwrap_or(false);
    if logged_in {
        format!("Logged in with vault profile '{profile}' at {url}")
    } else {
        format!("Vault login failed for profile '{profile}' at {url}")
    }
}

pub fn format_text_vault_list(result: &Value) -> String {
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    let profiles = result
        .get("profiles")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    if count == 0 {
        "No vault profiles found.".to_string()
    } else {
        format!("Vault profiles ({count}): {profiles}")
    }
}

pub fn format_text_action_cache(result: &Value) -> String {
    // Stats response
    if let Some(entries) = result.get("entries").and_then(|v| v.as_u64()) {
        if result.get("hits").is_some() {
            let hits = result.get("hits").and_then(|v| v.as_u64()).unwrap_or(0);
            let misses = result.get("misses").and_then(|v| v.as_u64()).unwrap_or(0);
            let hit_rate = result.get("hitRate").and_then(|v| v.as_f64()).unwrap_or(0.0);
            return format!(
                "Action cache: {entries} entries, {hits} hits, {misses} misses ({:.0}% hit rate)",
                hit_rate * 100.0
            );
        }
    }
    // Get response
    if let Some(found) = result.get("found").and_then(|v| v.as_bool()) {
        if found {
            let selector = result.get("selector").and_then(|v| v.as_str()).unwrap_or("?");
            let score = result.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            return format!("Cache hit: {selector} (score: {score:.2})");
        } else {
            return "Cache miss".to_string();
        }
    }
    // Put response
    if result.get("stored").and_then(|v| v.as_bool()).unwrap_or(false) {
        let intent = result.get("intent").and_then(|v| v.as_str()).unwrap_or("?");
        let selector = result.get("selector").and_then(|v| v.as_str()).unwrap_or("?");
        return format!("Cached: {intent} → {selector}");
    }
    // Clear response
    if let Some(cleared) = result.get("cleared").and_then(|v| v.as_u64()) {
        return format!("Action cache cleared ({cleared} entries removed)");
    }
    format!("{}", serde_json::to_string_pretty(result).unwrap_or_default())
}

pub fn format_text_check_injection(result: &Value) -> String {
    let clean = result.get("clean").and_then(|v| v.as_bool()).unwrap_or(true);
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

    if clean {
        return "✅ No prompt injection patterns detected".to_string();
    }

    let mut lines = vec![format!("⚠️  {count} potential injection pattern(s) found:")];
    if let Some(findings) = result.get("findings").and_then(|v| v.as_array()) {
        for f in findings {
            let severity = f.get("severity").and_then(|v| v.as_str()).unwrap_or("?");
            let desc = f.get("description").and_then(|v| v.as_str()).unwrap_or("?");
            let source = f.get("source").and_then(|v| v.as_str()).unwrap_or("?");
            let fc = f.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            let icon = match severity {
                "high" => "🔴",
                "medium" => "🟡",
                _ => "🟢",
            };
            lines.push(format!("  {icon} [{severity}] {desc} ({fc}x, {source})"));
        }
    }
    lines.join("\n")
}

pub fn format_text_generate_test(result: &Value) -> String {
    let path = result.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    let actions = result.get("actions").and_then(|v| v.as_u64()).unwrap_or(0);
    let lines = result.get("lines").and_then(|v| v.as_u64()).unwrap_or(0);
    format!("Test generated: {path}\n  {actions} actions → {lines} lines")
}

pub fn format_text_har_export(result: &Value) -> String {
    let path = result.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    let entries = result.get("entries").and_then(|v| v.as_u64()).unwrap_or(0);
    let size = result.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
    let size_kb = size as f64 / 1024.0;
    format!("HAR exported: {path}\n  {entries} entries, {size_kb:.1} KB")
}

pub fn format_text_trace_start(result: &Value) -> String {
    let name = result
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("(unnamed)");
    format!("Trace started: {name}")
}

pub fn format_text_trace_stop(result: &Value) -> String {
    let path = result.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    let events = result.get("events").and_then(|v| v.as_u64()).unwrap_or(0);
    let duration = result.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let size = result.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
    let size_kb = size as f64 / 1024.0;
    format!(
        "Trace stopped: {path}\n  {events} events, {duration:.1}s, {size_kb:.1} KB"
    )
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
