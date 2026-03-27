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
