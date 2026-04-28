//! Wait-for handler: polls for 11 different conditions with configurable timeout.

use crate::daemon::inspection;
use crate::daemon::logs::DaemonLogs;
use crate::daemon::state::DaemonState;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::debug;

const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const POLL_INTERVAL_MS: u64 = 100;

/// Handle a `wait_for` request. Polls the given condition until it's met or timeout.
pub async fn handle_wait_for(
    page: &Page,
    logs: &DaemonLogs,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let condition = params
        .get("condition")
        .and_then(|v| v.as_str())
        .ok_or("missing 'condition' parameter")?;

    let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("");

    let threshold = params
        .get("threshold")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let timeout_ms = params
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TIMEOUT_MS);

    let start = Instant::now();
    let deadline = Duration::from_millis(timeout_ms);
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

    debug!("wait_for: condition={condition} value={value} threshold={threshold} timeout={timeout_ms}ms");

    let met = match condition {
        "delay" => {
            let delay_ms: u64 = value.parse().unwrap_or(1000);
            let delay_dur = Duration::from_millis(delay_ms).min(deadline);
            sleep(delay_dur).await;
            true
        }
        "selector_visible" => {
            poll_until(deadline, poll_interval, || async {
                inspection::selector_query(page, state, value, true)
                    .await
                    .ok()
                    .and_then(|result| result.get("first").cloned())
                    .and_then(|first| first.get("visible").and_then(|visible| visible.as_bool()))
                    .unwrap_or(false)
            })
            .await
        }
        "selector_hidden" => {
            poll_until(deadline, poll_interval, || async {
                match inspection::selector_query(page, state, value, true).await {
                    Ok(result) => {
                        let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                        let visible = result
                            .get("first")
                            .and_then(|first| first.get("visible"))
                            .and_then(|visible| visible.as_bool())
                            .unwrap_or(false);
                        count == 0 || !visible
                    }
                    Err(_) => false,
                }
            })
            .await
        }
        "url_contains" => {
            poll_until(deadline, poll_interval, || async {
                inspection::target_url(page, state)
                    .await
                    .ok()
                    .and_then(|result| {
                        result
                            .get("url")
                            .and_then(|url| url.as_str())
                            .map(str::to_string)
                    })
                    .map(|url| url.contains(value))
                    .unwrap_or(false)
            })
            .await
        }
        "text_visible" => {
            poll_until(deadline, poll_interval, || async {
                inspection::text_query(page, state, value, true)
                    .await
                    .ok()
                    .and_then(|result| result.get("found").and_then(|found| found.as_bool()))
                    .unwrap_or(false)
            })
            .await
        }
        "text_hidden" => {
            poll_until(deadline, poll_interval, || async {
                !inspection::text_query(page, state, value, true)
                    .await
                    .ok()
                    .and_then(|result| result.get("found").and_then(|found| found.as_bool()))
                    .unwrap_or(false)
            })
            .await
        }
        "network_idle" => {
            // Wait until no new network entries appear within a 500ms window
            let idle_window = Duration::from_millis(500);
            let mut last_count = logs.network.len();
            let mut stable_since = Instant::now();
            let start_loop = Instant::now();
            loop {
                let current = logs.network.len();
                if current != last_count {
                    last_count = current;
                    stable_since = Instant::now();
                } else if stable_since.elapsed() >= idle_window {
                    break true;
                }
                if start_loop.elapsed() >= deadline {
                    break false;
                }
                sleep(poll_interval).await;
            }
        }
        "request_completed" => {
            poll_until(deadline, poll_interval, || async {
                let entries = logs.network.snapshot();
                entries.iter().any(|e| e.url.contains(value))
            })
            .await
        }
        "console_message" => {
            poll_until(deadline, poll_interval, || async {
                let entries = logs.console.snapshot();
                entries.iter().any(|e| e.text.contains(value))
            })
            .await
        }
        "element_count" => {
            let (op, target) = parse_threshold(threshold);
            let selector_for_count = value;
            poll_until(deadline, poll_interval, || async {
                let count = inspection::selector_query(page, state, selector_for_count, true)
                    .await
                    .ok()
                    .and_then(|result| result.get("count").and_then(|count| count.as_u64()))
                    .unwrap_or(0);
                compare_threshold(count, &op, target)
            })
            .await
        }
        "region_stable" => {
            // Poll innerHTML hash; require 2 consecutive identical hashes
            let mut prev_hash: Option<u64> = None;
            let start_loop = Instant::now();
            loop {
                let html = inspection::region_signature(page, state, value)
                    .await
                    .ok()
                    .and_then(|result| {
                        result
                            .get("html")
                            .and_then(|html| html.as_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
                let mut hasher = DefaultHasher::new();
                html.hash(&mut hasher);
                let h = hasher.finish();
                if prev_hash == Some(h) {
                    break true;
                }
                prev_hash = Some(h);
                if start_loop.elapsed() >= deadline {
                    break false;
                }
                sleep(poll_interval).await;
            }
        }
        unknown => {
            return Err(format!("unknown wait condition: {unknown}"));
        }
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;

    Ok(json!({
        "condition": condition,
        "met": met,
        "elapsed_ms": elapsed_ms,
        "timeout_ms": timeout_ms,
        "value": value,
    }))
}

/// Poll a condition function until it returns true or deadline is hit.
async fn poll_until<F, Fut>(deadline: Duration, interval: Duration, mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = Instant::now();
    loop {
        if check().await {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        sleep(interval).await;
    }
}

/// Escape a string for embedding as a JS string literal.
#[cfg(test)]
fn js_string(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("'{escaped}'")
}

/// Parse a threshold string like ">=3", "==0", "<5", or bare "3" (defaults to >=).
fn parse_threshold(input: &str) -> (String, u64) {
    let s = input.trim();
    if s.is_empty() {
        return (">=".to_string(), 0);
    }
    for prefix in [">=", "<=", "==", "!=", ">", "<"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            let val = rest.trim().parse::<u64>().unwrap_or(0);
            return (prefix.to_string(), val);
        }
    }
    // Bare number defaults to >=
    let val = s.parse::<u64>().unwrap_or(0);
    (">=".to_string(), val)
}

/// Compare a count against a parsed threshold.
fn compare_threshold(count: u64, op: &str, target: u64) -> bool {
    match op {
        ">=" => count >= target,
        "<=" => count <= target,
        "==" => count == target,
        "!=" => count != target,
        ">" => count > target,
        "<" => count < target,
        _ => count >= target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_threshold_operators() {
        assert_eq!(parse_threshold(">=3"), (">=".to_string(), 3));
        assert_eq!(parse_threshold("==0"), ("==".to_string(), 0));
        assert_eq!(parse_threshold("<5"), ("<".to_string(), 5));
        assert_eq!(parse_threshold("3"), (">=".to_string(), 3));
        assert_eq!(parse_threshold(""), (">=".to_string(), 0));
    }

    #[test]
    fn compare_threshold_ops() {
        assert!(compare_threshold(5, ">=", 3));
        assert!(!compare_threshold(2, ">=", 3));
        assert!(compare_threshold(0, "==", 0));
        assert!(compare_threshold(4, "<", 5));
        assert!(!compare_threshold(5, "<", 5));
    }

    #[test]
    fn js_string_escapes() {
        assert_eq!(js_string("hello"), "'hello'");
        assert_eq!(js_string("it's"), "'it\\'s'");
        assert_eq!(js_string("a\\b"), "'a\\\\b'");
    }
}
