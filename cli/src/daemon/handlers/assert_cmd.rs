//! Assertion and diff handlers.
//!
//! `handle_assert` evaluates 15+ assertion kinds against current page state,
//! console/network logs, and optional sinceActionId temporal scoping.
//! `handle_diff` compares current page state against stored snapshots.

use crate::daemon::capture::capture_compact_page_state;
use crate::daemon::inspection;
use crate::daemon::logs::DaemonLogs;
use crate::daemon::state::DaemonState;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::collections::HashMap;

// ── Threshold parsing ──

/// Comparison operator for threshold checks.
#[derive(Debug, Clone, Copy)]
enum ThresholdOp {
    Gte,
    Lte,
    Gt,
    Lt,
    Eq,
}

/// Parsed threshold: operator + integer value.
#[derive(Debug, Clone)]
struct Threshold {
    op: ThresholdOp,
    val: i64,
}

/// Parse a threshold string like ">=3", "==0", "<5", or bare "3" (defaults to >=).
fn parse_threshold(s: &str) -> Result<Threshold, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty threshold".into());
    }

    let (op, rest) = if let Some(r) = s.strip_prefix(">=") {
        (ThresholdOp::Gte, r)
    } else if let Some(r) = s.strip_prefix("<=") {
        (ThresholdOp::Lte, r)
    } else if let Some(r) = s.strip_prefix("==") {
        (ThresholdOp::Eq, r)
    } else if let Some(r) = s.strip_prefix('>') {
        (ThresholdOp::Gt, r)
    } else if let Some(r) = s.strip_prefix('<') {
        (ThresholdOp::Lt, r)
    } else {
        // Bare number defaults to >=
        (ThresholdOp::Gte, s)
    };

    let val: i64 = rest
        .trim()
        .parse()
        .map_err(|_| format!("invalid threshold number: '{rest}'"))?;
    Ok(Threshold { op, val })
}

fn threshold_met(actual: i64, threshold: &Threshold) -> bool {
    match threshold.op {
        ThresholdOp::Gte => actual >= threshold.val,
        ThresholdOp::Lte => actual <= threshold.val,
        ThresholdOp::Gt => actual > threshold.val,
        ThresholdOp::Lt => actual < threshold.val,
        ThresholdOp::Eq => actual == threshold.val,
    }
}

fn threshold_display(threshold: &Threshold) -> String {
    let op = match threshold.op {
        ThresholdOp::Gte => ">=",
        ThresholdOp::Lte => "<=",
        ThresholdOp::Gt => ">",
        ThresholdOp::Lt => "<",
        ThresholdOp::Eq => "==",
    };
    format!("{op}{}", threshold.val)
}

// ── Assert handler ──

pub async fn handle_assert(
    page: &Page,
    logs: &DaemonLogs,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let checks = params
        .get("checks")
        .and_then(|v| v.as_array())
        .ok_or("missing 'checks' array")?;

    if checks.is_empty() {
        return Err("'checks' array is empty".into());
    }

    // Snapshot console/network logs (non-destructive)
    let console_entries = logs.console.snapshot();
    let network_entries = logs.network.snapshot();
    let mut selector_cache: HashMap<String, Value> = HashMap::new();
    let mut text_cache: HashMap<String, Value> = HashMap::new();
    let target_url = inspection::target_url(page, state)
        .await
        .unwrap_or_else(|err| json!({"ok": false, "error": err}));

    // Evaluate each check
    let mut results: Vec<Value> = Vec::new();
    let mut all_passed = true;

    for check in checks {
        let kind = check.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let text = check.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let selector = check.get("selector").and_then(|v| v.as_str()).unwrap_or("");
        let value = check.get("value").and_then(|v| v.as_str()).unwrap_or("");
        let threshold_str = check
            .get("threshold")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let since_action_id = check.get("sinceActionId").and_then(|v| v.as_u64());
        let checked_expected = check
            .get("checked")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Get timestamp filter from sinceActionId
        let since_ts: Option<f64> = since_action_id.and_then(|id| {
            let timeline = state.timeline.lock().unwrap();
            timeline.get(id).map(|e| e.started_at)
        });

        let (passed, expected, actual) = match kind {
            "url_contains" => {
                let actual_url = target_url
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let pass = actual_url.contains(text);
                (pass, format!("URL contains '{text}'"), actual_url.to_string())
            }
            "text_visible" => {
                if !text_cache.contains_key(text) {
                    let result = inspection::text_query(page, state, text, true)
                        .await
                        .unwrap_or_else(|err| json!({"ok": false, "error": err}));
                    text_cache.insert(text.to_string(), result);
                }
                let text_result = text_cache.get(text).cloned().unwrap_or_else(|| json!({}));
                let pass = text_result
                    .get("found")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let snippet = text_result
                    .get("matches")
                    .and_then(|v| v.as_array())
                    .and_then(|matches| matches.first())
                    .and_then(|first| first.get("snippet"))
                    .and_then(|snippet| snippet.as_str())
                    .unwrap_or("");
                (pass, format!("text '{text}' visible"), snippet.to_string())
            }
            "text_hidden" => {
                if !text_cache.contains_key(text) {
                    let result = inspection::text_query(page, state, text, true)
                        .await
                        .unwrap_or_else(|err| json!({"ok": false, "error": err}));
                    text_cache.insert(text.to_string(), result);
                }
                let text_result = text_cache.get(text).cloned().unwrap_or_else(|| json!({}));
                let pass = !text_result
                    .get("found")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let snippet = text_result
                    .get("matches")
                    .and_then(|v| v.as_array())
                    .and_then(|matches| matches.first())
                    .and_then(|first| first.get("snippet"))
                    .and_then(|snippet| snippet.as_str())
                    .unwrap_or("");
                (pass, format!("text '{text}' hidden"), snippet.to_string())
            }
            "selector_visible" => {
                if !selector_cache.contains_key(selector) {
                    let result = inspection::selector_query(page, state, selector, true)
                        .await
                        .unwrap_or_else(|err| json!({"ok": false, "error": err}));
                    selector_cache.insert(selector.to_string(), result);
                }
                let sel_state = selector_cache
                    .get(selector)
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let pass = sel_state
                    .get("first")
                    .and_then(|v| v.get("visible"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let exists = sel_state
                    .get("count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    > 0;
                let actual_str = if exists {
                    if pass {
                        "visible"
                    } else {
                        "hidden"
                    }
                } else {
                    "not found"
                };
                (
                    pass,
                    format!("'{selector}' visible"),
                    actual_str.to_string(),
                )
            }
            "selector_hidden" => {
                if !selector_cache.contains_key(selector) {
                    let result = inspection::selector_query(page, state, selector, true)
                        .await
                        .unwrap_or_else(|err| json!({"ok": false, "error": err}));
                    selector_cache.insert(selector.to_string(), result);
                }
                let sel_state = selector_cache
                    .get(selector)
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let is_visible = sel_state
                    .get("first")
                    .and_then(|v| v.get("visible"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let exists = sel_state
                    .get("count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    > 0;
                // Hidden means: doesn't exist OR not visible
                let pass = !exists || !is_visible;
                let actual_str = if !exists {
                    "not found"
                } else if is_visible {
                    "visible"
                } else {
                    "hidden"
                };
                (pass, format!("'{selector}' hidden"), actual_str.to_string())
            }
            "value_equals" => {
                if !selector_cache.contains_key(selector) {
                    let result = inspection::selector_query(page, state, selector, true)
                        .await
                        .unwrap_or_else(|err| json!({"ok": false, "error": err}));
                    selector_cache.insert(selector.to_string(), result);
                }
                let sel_state = selector_cache
                    .get(selector)
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let actual_val = sel_state
                    .get("first")
                    .and_then(|v| v.get("value"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let pass = actual_val == value;
                (pass, format!("value == '{value}'"), actual_val.to_string())
            }
            "checked" => {
                if !selector_cache.contains_key(selector) {
                    let result = inspection::selector_query(page, state, selector, true)
                        .await
                        .unwrap_or_else(|err| json!({"ok": false, "error": err}));
                    selector_cache.insert(selector.to_string(), result);
                }
                let sel_state = selector_cache
                    .get(selector)
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let actual_checked = sel_state
                    .get("first")
                    .and_then(|v| v.get("checked"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let pass = actual_checked == checked_expected;
                (
                    pass,
                    format!("checked == {checked_expected}"),
                    format!("{actual_checked}"),
                )
            }
            "no_console_errors" => {
                let errors: Vec<_> = console_entries
                    .iter()
                    .filter(|e| e.log_type == "error" || e.log_type == "pageerror")
                    .collect();
                let pass = errors.is_empty();
                (
                    pass,
                    "no console errors".into(),
                    format!("{} errors", errors.len()),
                )
            }
            "no_failed_requests" => {
                let failed: Vec<_> = network_entries
                    .iter()
                    .filter(|e| e.failed || e.status >= 400)
                    .collect();
                let pass = failed.is_empty();
                (
                    pass,
                    "no failed requests".into(),
                    format!("{} failures", failed.len()),
                )
            }
            "request_url_seen" => {
                let seen = network_entries.iter().any(|e| e.url.contains(text));
                (
                    seen,
                    format!("request URL contains '{text}'"),
                    if seen {
                        "found".into()
                    } else {
                        "not found".into()
                    },
                )
            }
            "response_status" => {
                let expected_status: u32 = value.parse().unwrap_or(200);
                let matching = network_entries
                    .iter()
                    .find(|e| e.url.contains(text) && e.status == expected_status);
                let pass = matching.is_some();
                let actual_str =
                    if let Some(entry) = network_entries.iter().find(|e| e.url.contains(text)) {
                        format!("{}", entry.status)
                    } else {
                        "no matching request".into()
                    };
                (
                    pass,
                    format!("response status {expected_status} for '{text}'"),
                    actual_str,
                )
            }
            "console_message_matches" => {
                let found = console_entries.iter().any(|e| e.text.contains(text));
                (
                    found,
                    format!("console message contains '{text}'"),
                    if found {
                        "found".into()
                    } else {
                        "not found".into()
                    },
                )
            }
            "network_count" => {
                let count = network_entries
                    .iter()
                    .filter(|e| text.is_empty() || e.url.contains(text))
                    .count() as i64;
                let threshold = parse_threshold(if threshold_str.is_empty() {
                    ">=0"
                } else {
                    threshold_str
                })
                .map_err(|e| format!("bad threshold: {e}"))?;
                let pass = threshold_met(count, &threshold);
                (
                    pass,
                    format!("network count {}", threshold_display(&threshold)),
                    format!("{count}"),
                )
            }
            "console_count" => {
                let count = console_entries
                    .iter()
                    .filter(|e| text.is_empty() || e.text.contains(text))
                    .count() as i64;
                let threshold = parse_threshold(if threshold_str.is_empty() {
                    ">=0"
                } else {
                    threshold_str
                })
                .map_err(|e| format!("bad threshold: {e}"))?;
                let pass = threshold_met(count, &threshold);
                (
                    pass,
                    format!("console count {}", threshold_display(&threshold)),
                    format!("{count}"),
                )
            }
            "element_count" => {
                if selector.is_empty() {
                    return Err("element_count requires 'selector'".into());
                }
                if !selector_cache.contains_key(selector) {
                    let result = inspection::selector_query(page, state, selector, true)
                        .await
                        .unwrap_or_else(|err| json!({"ok": false, "error": err}));
                    selector_cache.insert(selector.to_string(), result);
                }
                let sel_state = selector_cache
                    .get(selector)
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let count = sel_state
                    .get("count")
                    .and_then(|v| v.as_i64())
                    .or_else(|| {
                        sel_state
                            .get("count")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as i64)
                    })
                    .unwrap_or(0);

                let min = check.get("min").and_then(|v| v.as_i64());
                let max = check.get("max").and_then(|v| v.as_i64());
                let exact = check.get("exact").and_then(|v| v.as_i64());

                let (pass, expected_str) = if let Some(e) = exact {
                    (count == e, format!("exactly {e} elements"))
                } else if !threshold_str.is_empty() {
                    let threshold = parse_threshold(threshold_str)
                        .map_err(|e| format!("bad threshold: {e}"))?;
                    (
                        threshold_met(count, &threshold),
                        format!("count {}", threshold_display(&threshold)),
                    )
                } else {
                    let min_ok = min.map_or(true, |m| count >= m);
                    let max_ok = max.map_or(true, |m| count <= m);
                    let expected = match (min, max) {
                        (Some(mn), Some(mx)) => format!("{mn}..={mx} elements"),
                        (Some(mn), None) => format!(">={mn} elements"),
                        (None, Some(mx)) => format!("<={mx} elements"),
                        (None, None) => "any count".into(),
                    };
                    (min_ok && max_ok, expected)
                };

                (
                    pass,
                    format!("'{selector}' {expected_str}"),
                    format!("{count} found"),
                )
            }
            "no_console_errors_since" => {
                let errors: Vec<_> = console_entries
                    .iter()
                    .filter(|e| {
                        (e.log_type == "error" || e.log_type == "pageerror")
                            && since_ts.map_or(true, |ts| e.timestamp >= ts)
                    })
                    .collect();
                let pass = errors.is_empty();
                (
                    pass,
                    format!(
                        "no console errors since action {}",
                        since_action_id.unwrap_or(0)
                    ),
                    format!("{} errors", errors.len()),
                )
            }
            "no_failed_requests_since" => {
                let failed: Vec<_> = network_entries
                    .iter()
                    .filter(|e| {
                        (e.failed || e.status >= 400)
                            && since_ts.map_or(true, |ts| e.timestamp >= ts)
                    })
                    .collect();
                let pass = failed.is_empty();
                (
                    pass,
                    format!(
                        "no failed requests since action {}",
                        since_action_id.unwrap_or(0)
                    ),
                    format!("{} failures", failed.len()),
                )
            }
            _ => {
                return Err(format!("unknown assertion kind: '{kind}'"));
            }
        };

        if !passed {
            all_passed = false;
        }

        let mut check_result = json!({
            "kind": kind,
            "passed": passed,
            "expected": expected,
            "actual": actual,
        });
        if !selector.is_empty() {
            check_result["selector"] = json!(selector);
        }
        results.push(check_result);
    }

    let passed_count = results
        .iter()
        .filter(|r| r["passed"].as_bool().unwrap_or(false))
        .count();
    let total = results.len();
    let summary = format!("{passed_count}/{total} checks passed");

    let mut response = json!({
        "verified": all_passed,
        "checks": results,
        "summary": summary,
    });

    if !all_passed {
        let failed_kinds: Vec<String> = results
            .iter()
            .filter(|r| !r["passed"].as_bool().unwrap_or(true))
            .filter_map(|r| r["kind"].as_str().map(String::from))
            .collect();
        response["agentHint"] = json!(format!(
            "Failed checks: {}. Verify page state and retry.",
            failed_kinds.join(", ")
        ));
    }

    Ok(response)
}

// ── Diff handler ──

pub async fn handle_diff(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let current = capture_compact_page_state(page, true).await;

    // Get the stored state to compare against
    let since_action_id = params.get("sinceActionId").and_then(|v| v.as_u64());

    let previous = {
        let diff_state = state.diff.lock().unwrap();
        if since_action_id.is_some() {
            // Use the before-state if available
            diff_state.before.clone()
        } else {
            // Default: compare against the most recent after-state
            diff_state
                .after
                .clone()
                .or_else(|| diff_state.before.clone())
        }
    };

    let previous = match previous {
        Some(p) => p,
        None => {
            // No stored state — store current and return "no baseline"
            let mut diff_state = state.diff.lock().unwrap();
            diff_state.before = Some(current.clone());
            diff_state.after = Some(current);
            return Ok(json!({
                "changed": false,
                "changes": [],
                "summary": "No previous state to compare — current state stored as baseline.",
            }));
        }
    };

    // Compare fields
    let mut changes: Vec<Value> = Vec::new();

    if current.url != previous.url {
        changes.push(json!({
            "field": "url",
            "before": previous.url,
            "after": current.url,
        }));
    }
    if current.title != previous.title {
        changes.push(json!({
            "field": "title",
            "before": previous.title,
            "after": current.title,
        }));
    }
    if current.focus != previous.focus {
        changes.push(json!({
            "field": "focus",
            "before": previous.focus,
            "after": current.focus,
        }));
    }
    if current.dialog.count != previous.dialog.count
        || current.dialog.title != previous.dialog.title
    {
        changes.push(json!({
            "field": "dialog",
            "before": format!("count={}, title=\"{}\"", previous.dialog.count, previous.dialog.title),
            "after": format!("count={}, title=\"{}\"", current.dialog.count, current.dialog.title),
        }));
    }
    if current.headings != previous.headings {
        changes.push(json!({
            "field": "headings",
            "before": previous.headings,
            "after": current.headings,
        }));
    }

    // Element count changes
    let count_fields = [
        (
            "landmarks",
            current.counts.landmarks,
            previous.counts.landmarks,
        ),
        ("buttons", current.counts.buttons, previous.counts.buttons),
        ("links", current.counts.links, previous.counts.links),
        ("inputs", current.counts.inputs, previous.counts.inputs),
    ];
    for (name, cur, prev) in &count_fields {
        if cur != prev {
            changes.push(json!({
                "field": format!("counts.{name}"),
                "before": prev,
                "after": cur,
            }));
        }
    }

    // Body text similarity (simple: check if different)
    if current.body_text != previous.body_text {
        let cur_len = current.body_text.len();
        let prev_len = previous.body_text.len();
        changes.push(json!({
            "field": "bodyText",
            "before": format!("{prev_len} chars"),
            "after": format!("{cur_len} chars"),
        }));
    }

    let changed = !changes.is_empty();
    let summary = if changed {
        let fields: Vec<&str> = changes.iter().filter_map(|c| c["field"].as_str()).collect();
        format!("{} fields changed: {}", fields.len(), fields.join(", "))
    } else {
        "No changes detected.".into()
    };

    // Update stored state
    {
        let mut diff_state = state.diff.lock().unwrap();
        diff_state.before = diff_state.after.take();
        diff_state.after = Some(current);
    }

    Ok(json!({
        "changed": changed,
        "changes": changes,
        "summary": summary,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_threshold_gte() {
        let t = parse_threshold(">=3").unwrap();
        assert!(threshold_met(3, &t));
        assert!(threshold_met(5, &t));
        assert!(!threshold_met(2, &t));
    }

    #[test]
    fn test_parse_threshold_bare() {
        let t = parse_threshold("3").unwrap();
        // Bare number defaults to >=
        assert!(threshold_met(3, &t));
        assert!(threshold_met(5, &t));
        assert!(!threshold_met(2, &t));
    }

    #[test]
    fn test_parse_threshold_eq() {
        let t = parse_threshold("==0").unwrap();
        assert!(threshold_met(0, &t));
        assert!(!threshold_met(1, &t));
    }

    #[test]
    fn test_parse_threshold_lt() {
        let t = parse_threshold("<5").unwrap();
        assert!(threshold_met(4, &t));
        assert!(!threshold_met(5, &t));
    }

    #[test]
    fn test_parse_threshold_lte() {
        let t = parse_threshold("<=5").unwrap();
        assert!(threshold_met(5, &t));
        assert!(!threshold_met(6, &t));
    }

    #[test]
    fn test_parse_threshold_gt() {
        let t = parse_threshold(">0").unwrap();
        assert!(threshold_met(1, &t));
        assert!(!threshold_met(0, &t));
    }

    #[test]
    fn test_parse_threshold_empty_err() {
        assert!(parse_threshold("").is_err());
    }

    #[test]
    fn test_parse_threshold_bad_number() {
        assert!(parse_threshold(">=abc").is_err());
    }

    #[test]
    fn test_threshold_display() {
        let t = parse_threshold(">=3").unwrap();
        assert_eq!(threshold_display(&t), ">=3");
        let t = parse_threshold("==0").unwrap();
        assert_eq!(threshold_display(&t), "==0");
    }

    // element_count threshold logic (pure, no page needed)
    #[test]
    fn test_element_count_min_pass() {
        let t = parse_threshold(">=5").unwrap();
        assert!(threshold_met(5, &t));
        assert!(threshold_met(10, &t));
        assert!(!threshold_met(4, &t));
    }

    #[test]
    fn test_element_count_exact() {
        // exact == threshold with ==
        let t = parse_threshold("==3").unwrap();
        assert!(threshold_met(3, &t));
        assert!(!threshold_met(2, &t));
        assert!(!threshold_met(4, &t));
    }

    #[test]
    fn test_element_count_max() {
        let t = parse_threshold("<=10").unwrap();
        assert!(threshold_met(0, &t));
        assert!(threshold_met(10, &t));
        assert!(!threshold_met(11, &t));
    }
}
