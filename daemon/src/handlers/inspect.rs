//! Inspection command handlers: console, network, dialog, eval.
//!
//! Console, network, and dialog handlers read from the shared log buffers.
//! The eval handler executes arbitrary JS expressions via CDP.

use crate::logs::DaemonLogs;
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
pub async fn handle_eval(page: &Page, params: &Value) -> Result<Value, String> {
    let expression = params
        .get("expression")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: expression".to_string())?;

    if expression.trim().is_empty() {
        return Err("expression cannot be empty".to_string());
    }

    debug!("handle_eval: expression={}", &expression[..expression.len().min(100)]);

    let result = timeout(
        Duration::from_secs(30),
        page.evaluate_expression(expression),
    )
    .await
    .map_err(|_| "eval timed out after 30s".to_string())?
    .map_err(|e| format!("eval error: {e}"))?;

    // Serialize the result to a JSON string
    let value = result.value().cloned().unwrap_or(Value::Null);
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
