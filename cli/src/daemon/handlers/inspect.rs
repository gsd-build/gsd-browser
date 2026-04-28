//! Inspection command handlers: console, network, dialog, eval, accessibility-tree, find, page-source.
//!
//! Console, network, and dialog handlers read from the shared log buffers.
//! The eval, accessibility-tree, find, and page-source handlers execute via CDP.

use crate::daemon::inspection;
use crate::daemon::logs::DaemonLogs;
use crate::daemon::state::DaemonState;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;
use tracing::debug;

/// Maximum size for eval results (64KB).
const EVAL_MAX_RESULT_BYTES: usize = 64 * 1024;

/// Handle `console` command — returns buffered console log entries.
///
/// Params:
/// - `clear` (bool, default true): if true, drains the buffer; if false, snapshots.
pub fn handle_console(logs: &DaemonLogs, params: &Value) -> Result<Value, String> {
    let clear = params
        .get("clear")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let entries = if clear {
        logs.console.drain()
    } else {
        logs.console.snapshot()
    };

    let count = entries.len();
    debug!("handle_console: returning {count} entries (clear={clear})");

    Ok(json!({
        "entries": entries,
        "count": count,
    }))
}

/// Handle `network` command — returns buffered network log entries.
///
/// Params:
/// - `clear` (bool, default true): if true, drains the buffer; if false, snapshots.
/// - `filter` (string, default "all"): "all", "errors", or "fetch-xhr".
pub fn handle_network(logs: &DaemonLogs, params: &Value) -> Result<Value, String> {
    let clear = params
        .get("clear")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let filter = params
        .get("filter")
        .and_then(|v| v.as_str())
        .unwrap_or("all");

    let entries = if clear {
        logs.network.drain()
    } else {
        logs.network.snapshot()
    };

    let filtered: Vec<_> = match filter {
        "errors" => entries
            .into_iter()
            .filter(|e| e.failed || e.status >= 400)
            .collect(),
        "fetch-xhr" => entries
            .into_iter()
            .filter(|e| {
                let rt = e.resource_type.to_lowercase();
                rt.contains("xhr") || rt.contains("fetch")
            })
            .collect(),
        _ => entries, // "all" or anything else
    };

    let count = filtered.len();
    debug!("handle_network: returning {count} entries (clear={clear}, filter={filter})");

    Ok(json!({
        "entries": filtered,
        "count": count,
    }))
}

/// Handle `dialog` command — returns buffered dialog events.
///
/// Params:
/// - `clear` (bool, default true): if true, drains the buffer; if false, snapshots.
pub fn handle_dialog(logs: &DaemonLogs, params: &Value) -> Result<Value, String> {
    let clear = params
        .get("clear")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let entries = if clear {
        logs.dialog.drain()
    } else {
        logs.dialog.snapshot()
    };

    let count = entries.len();
    debug!("handle_dialog: returning {count} entries (clear={clear})");

    Ok(json!({
        "entries": entries,
        "count": count,
    }))
}

/// Handle `eval` command — executes a JS expression and returns the result.
///
/// Params:
/// - `expression` (string, required): JS expression to evaluate.
pub async fn handle_eval(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let expression = params
        .get("expression")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: expression".to_string())?;

    if expression.trim().is_empty() {
        return Err("expression cannot be empty".to_string());
    }

    debug!(
        "handle_eval: expression={}",
        &expression[..expression.len().min(100)]
    );

    let eval_result = inspection::eval_expression(page, state, expression).await?;
    let ok = eval_result
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !ok {
        return Err(eval_result
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("eval failed")
            .to_string());
    }

    let value = eval_result.get("value").cloned().unwrap_or(Value::Null);
    let mut result_str = if let Some(s) = value.as_str() {
        s.to_string()
    } else {
        serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string())
    };

    // Truncate to 64KB
    if result_str.len() > EVAL_MAX_RESULT_BYTES {
        result_str.truncate(EVAL_MAX_RESULT_BYTES);
        result_str.push_str("... [truncated]");
    }

    Ok(json!({
        "result": result_str,
    }))
}

/// Maximum page-source size (200KB).
const PAGE_SOURCE_MAX_BYTES: usize = 200 * 1024;

/// Handle `accessibility_tree` command — walks the DOM and returns an indented role/name tree.
///
/// Params:
/// - `selector` (string, optional): CSS selector to scope the tree.
/// - `max_depth` (u32, default 10): max tree depth.
/// - `max_count` (u32, default 100): max elements to include.
pub async fn handle_accessibility_tree(page: &Page, params: &Value) -> Result<Value, String> {
    let selector = params
        .get("selector")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let max_depth = params
        .get("max_depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as u32;
    let max_count = params
        .get("max_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as u32;

    debug!(
        "handle_accessibility_tree: selector={:?}, max_depth={}, max_count={}",
        selector, max_depth, max_count
    );

    let js = format!(
        r#"(() => {{
  const pi = window.__pi || {{}};
  const root = {root_expr};
  if (!root) return JSON.stringify({{ tree: "", nodeCount: 0, truncated: false, error: "selector not found" }});

  const lines = [];
  let count = 0;
  let truncated = false;
  const maxDepth = {max_depth};
  const maxCount = {max_count};

  function walk(el, depth) {{
    if (count >= maxCount) {{ truncated = true; return; }}
    if (depth > maxDepth) return;

    const tag = el.tagName ? el.tagName.toLowerCase() : "";
    if (!tag) return;

    // Skip invisible elements unless they have an explicit role
    const visible = pi.isVisible ? pi.isVisible(el) : true;
    const role = pi.inferRole ? pi.inferRole(el) : "";
    const hasExplicitRole = el.getAttribute && el.getAttribute("role");
    if (!visible && !hasExplicitRole) return;

    // Build name
    const name = pi.accessibleName ? pi.accessibleName(el) : (el.textContent || "").trim().slice(0, 80);
    const enabled = pi.isEnabled ? pi.isEnabled(el) : true;

    // Determine display role — use inferred role if present, otherwise tag
    const displayRole = role || tag;

    // Build state indicators
    const states = [];
    if (!enabled) states.push("disabled");
    if (!visible) states.push("hidden");

    const indent = "  ".repeat(depth);
    let line = indent + displayRole;
    if (name) line += ' "' + name.replace(/"/g, '\\"') + '"';
    if (states.length > 0) line += " [" + states.join(", ") + "]";

    lines.push(line);
    count++;

    // Recurse children
    const children = el.children || [];
    for (let i = 0; i < children.length; i++) {{
      if (count >= maxCount) {{ truncated = true; break; }}
      walk(children[i], depth + 1);
    }}
  }}

  walk(root, 0);
  return JSON.stringify({{ tree: lines.join("\n"), nodeCount: count, truncated: truncated }});
}})()"#,
        root_expr = if selector.is_empty() {
            "document.body".to_string()
        } else {
            format!(
                "document.querySelector('{}')",
                selector.replace('\'', "\\'")
            )
        },
        max_depth = max_depth,
        max_count = max_count,
    );

    let result = timeout(Duration::from_secs(30), page.evaluate_expression(&js))
        .await
        .map_err(|_| "accessibility_tree timed out after 30s".to_string())?
        .map_err(|e| format!("accessibility_tree error: {}", super::clean_cdp_error(&e)))?;

    let value = result.value().cloned().unwrap_or(Value::Null);
    let json_str = value.as_str().unwrap_or("{}");
    let parsed: Value =
        serde_json::from_str(json_str).map_err(|e| format!("failed to parse tree result: {e}"))?;

    Ok(parsed)
}

/// Handle `find` command — searches for elements by role, text content, and/or CSS selector.
///
/// Params:
/// - `role` (string, optional): ARIA role to match.
/// - `text` (string, optional): text content to match (case-insensitive contains).
/// - `selector` (string, optional): CSS selector to scope search.
/// - `limit` (u32, default 20): max elements to return.
pub async fn handle_find(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let role = params.get("role").and_then(|v| v.as_str()).unwrap_or("");
    let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let selector = params
        .get("selector")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as u32;

    if role.is_empty() && text.is_empty() && selector.is_empty() {
        return Err("at least one of role, text, or selector is required".to_string());
    }

    debug!(
        "handle_find: role={:?}, text={:?}, selector={:?}, limit={}",
        role, text, selector, limit
    );

    inspection::find_elements(page, state, role, text, selector, limit).await
}

/// Handle `page_source` command — returns raw HTML of the page or a scoped element.
///
/// Params:
/// - `selector` (string, optional): CSS selector to scope the source.
pub async fn handle_page_source(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let selector = params
        .get("selector")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    debug!("handle_page_source: selector={:?}", selector);

    let source = inspection::page_source(
        page,
        state,
        if selector.is_empty() {
            None
        } else {
            Some(selector)
        },
    )
    .await?;
    let ok = source
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !ok {
        return Err(source
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("page_source failed")
            .to_string());
    }
    let mut html = source
        .get("html")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();

    let length = html.len();
    let truncated = length > PAGE_SOURCE_MAX_BYTES;
    if truncated {
        html.truncate(PAGE_SOURCE_MAX_BYTES);
    }

    Ok(json!({
        "html": html,
        "length": length,
        "truncated": truncated,
        "selector": if selector.is_empty() { Value::Null } else { Value::String(selector.to_string()) },
        "frameLabel": source.get("frameLabel").cloned().unwrap_or(Value::Null),
        "frameUrl": source.get("frameUrl").cloned().unwrap_or(Value::Null),
        "boundaries": source.get("boundaries").cloned().unwrap_or_else(|| json!([])),
    }))
}
