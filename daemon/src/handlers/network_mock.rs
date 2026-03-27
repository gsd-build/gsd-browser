//! Network mocking handlers: mock_route, block_urls, clear_routes.
//!
//! Uses the CDP Fetch domain to intercept requests and fulfill/fail them
//! based on stored route patterns with glob matching.

use crate::state::{DaemonState, MockRoute, MockType};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chromiumoxide::cdp::browser_protocol::fetch::{
    ContinueRequestParams, EnableParams as FetchEnableParams, EventRequestPaused,
    FailRequestParams, FulfillRequestParams, HeaderEntry,
};
use chromiumoxide::cdp::browser_protocol::network::ErrorReason;
use chromiumoxide::Page;
use futures::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, warn};

/// Simple glob match: `*` matches any chars within a segment, `**` matches anything.
fn glob_matches(pattern: &str, url: &str) -> bool {
    // Normalize: treat `**` as "match everything" segments
    let parts: Vec<&str> = pattern.split("**").collect();
    if parts.len() == 1 {
        // No ** — simple * matching
        return simple_glob(pattern, url);
    }

    // Multi-segment glob: each part between ** must appear in order
    let mut remaining = url;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // First segment must match from start
            if !simple_glob_prefix(part, remaining) {
                return false;
            }
            // Advance past the matched prefix
            remaining = advance_past_glob(part, remaining);
        } else {
            // Find the part somewhere in remaining
            match find_glob_match(part, remaining) {
                Some(after) => remaining = after,
                None => return false,
            }
        }
    }
    true
}

/// Match `*` as "any chars" (greedy within segments).
fn simple_glob(pattern: &str, text: &str) -> bool {
    let re = glob_to_regex(pattern);
    regex_lite::Regex::new(&re)
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

fn simple_glob_prefix(pattern: &str, text: &str) -> bool {
    let re = format!("^{}", glob_to_regex_inner(pattern));
    regex_lite::Regex::new(&re)
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

fn advance_past_glob<'a>(pattern: &str, text: &'a str) -> &'a str {
    let re = format!("^{}", glob_to_regex_inner(pattern));
    if let Ok(r) = regex_lite::Regex::new(&re) {
        if let Some(m) = r.find(text) {
            return &text[m.end()..];
        }
    }
    text
}

fn find_glob_match<'a>(pattern: &str, text: &'a str) -> Option<&'a str> {
    let re = glob_to_regex_inner(pattern);
    if let Ok(r) = regex_lite::Regex::new(&re) {
        if let Some(m) = r.find(text) {
            return Some(&text[m.end()..]);
        }
    }
    None
}

fn glob_to_regex(pattern: &str) -> String {
    format!("^{}$", glob_to_regex_inner(pattern))
}

fn glob_to_regex_inner(pattern: &str) -> String {
    let mut result = String::new();
    for ch in pattern.chars() {
        match ch {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            '.' | '+' | '^' | '$' | '(' | ')' | '{' | '}' | '[' | ']' | '|' | '\\' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

/// Enable the CDP Fetch domain if not already enabled, and spawn the event listener.
async fn ensure_fetch_enabled(page: &Page, state: &DaemonState) -> Result<(), String> {
    let needs_enable;
    let needs_listener;
    {
        let store = state.mock_routes.lock().unwrap();
        needs_enable = !store.fetch_enabled;
        needs_listener = !store.listener_spawned;
    }

    if needs_enable {
        // Enable Fetch domain — intercept all requests
        let params = FetchEnableParams::builder().build();
        page.execute(params)
            .await
            .map_err(|e| format!("Fetch.enable failed: {e}"))?;

        let mut store = state.mock_routes.lock().unwrap();
        store.fetch_enabled = true;
        debug!("network_mock: Fetch domain enabled");
    }

    if needs_listener {
        {
            let mut store = state.mock_routes.lock().unwrap();
            store.listener_spawned = true;
        }
        spawn_fetch_listener(page, &state.mock_routes).await;
    }

    Ok(())
}

/// Spawn a background task that consumes `Fetch.requestPaused` events and
/// matches against stored routes to fulfill/fail/continue requests.
async fn spawn_fetch_listener(page: &Page, store: &Mutex<crate::state::MockRouteStore>) {
    let mut stream = match page.event_listener::<EventRequestPaused>().await {
        Ok(s) => s,
        Err(e) => {
            warn!("spawn_fetch_listener: failed to create listener: {e}");
            return;
        }
    };

    // We need a clone of the store Arc and a page handle for async use
    let page = page.clone();
    let store_ptr = store as *const Mutex<crate::state::MockRouteStore>;
    // SAFETY: The mock_routes Mutex lives inside DaemonState which is Arc-wrapped and
    // lives for the entire daemon lifetime. The listener task will be cancelled when
    // the daemon shuts down. We convert to a raw pointer to get around lifetime issues,
    // but the Arc<DaemonState> guarantees the data lives long enough.
    let store_ref: &'static Mutex<crate::state::MockRouteStore> = unsafe { &*store_ptr };

    debug!("spawn_fetch_listener: listener spawned");

    tokio::spawn(async move {
        while let Some(event) = stream.next().await {
            let url = &event.request.url;
            let request_id = event.request_id.clone();

            // Find matching route
            let matched_route: Option<MockRoute> = {
                let store = store_ref.lock().unwrap();
                store
                    .routes
                    .iter()
                    .find(|r| glob_matches(&r.pattern, url))
                    .cloned()
            };

            match matched_route {
                Some(route) if route.route_type == MockType::Block => {
                    // Block the request
                    if let Err(e) = page
                        .execute(FailRequestParams::new(
                            request_id,
                            ErrorReason::BlockedByClient,
                        ))
                        .await
                    {
                        warn!("fetch_listener: FailRequest error: {e}");
                    }
                }
                Some(route) if route.route_type == MockType::Mock => {
                    // Apply optional delay
                    if route.delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(route.delay_ms)).await;
                    }

                    // Build response headers
                    let mut headers: Vec<HeaderEntry> = route
                        .headers
                        .iter()
                        .map(|(k, v)| HeaderEntry::new(k.clone(), v.clone()))
                        .collect();

                    // Add Content-Type if not already present
                    if !headers.iter().any(|h| h.name.eq_ignore_ascii_case("content-type")) {
                        headers.push(HeaderEntry::new(
                            "Content-Type",
                            if route.content_type.is_empty() {
                                "application/json"
                            } else {
                                &route.content_type
                            },
                        ));
                    }

                    let body_b64 = BASE64.encode(route.body.as_bytes());

                    let fulfill = FulfillRequestParams::builder()
                        .request_id(request_id)
                        .response_code(route.status as i64)
                        .response_headers(headers)
                        .body(body_b64)
                        .build();

                    match fulfill {
                        Ok(params) => {
                            if let Err(e) = page.execute(params).await {
                                warn!("fetch_listener: FulfillRequest error: {e}");
                            }
                        }
                        Err(e) => warn!("fetch_listener: FulfillRequest build error: {e}"),
                    }
                }
                _ => {
                    // No match — continue the request normally
                    if let Err(e) = page
                        .execute(ContinueRequestParams::new(request_id))
                        .await
                    {
                        warn!("fetch_listener: ContinueRequest error: {e}");
                    }
                }
            }
        }
        warn!("spawn_fetch_listener: event stream closed");
    });
}

/// Handle `mock_route` command — add a mock route and return its ID.
/// Params: { url: string, status?: u16, body?: string, headers?: object,
///           delay?: u64, content_type?: string }
pub async fn handle_mock_route(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let url_pattern = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: url".to_string())?;
    let status = params
        .get("status")
        .and_then(|v| v.as_u64())
        .unwrap_or(200) as u16;
    let body = params
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let delay_ms = params
        .get("delay")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let content_type = params
        .get("content_type")
        .or_else(|| params.get("contentType"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Parse headers from JSON object
    let headers: HashMap<String, String> = params
        .get("headers")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let route_id;
    {
        let mut store = state.mock_routes.lock().unwrap();
        route_id = store.next_id;
        store.next_id += 1;
        store.routes.push(MockRoute {
            id: route_id,
            pattern: url_pattern.to_string(),
            route_type: MockType::Mock,
            status,
            body,
            headers,
            delay_ms,
            content_type,
        });
    }

    ensure_fetch_enabled(page, state).await?;

    debug!("mock_route: added route id={route_id} pattern={url_pattern}");

    Ok(json!({
        "route_id": route_id,
        "pattern": url_pattern,
        "status": status,
    }))
}

/// Handle `block_urls` command — add block-type routes from patterns array.
/// Params: { patterns: [string] }
pub async fn handle_block_urls(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let patterns = params
        .get("patterns")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing required parameter: patterns (array)".to_string())?;

    let pattern_strs: Vec<String> = patterns
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    if pattern_strs.is_empty() {
        return Err("patterns array cannot be empty".to_string());
    }

    let count = pattern_strs.len();
    {
        let mut store = state.mock_routes.lock().unwrap();
        for pat in &pattern_strs {
            let id = store.next_id;
            store.next_id += 1;
            store.routes.push(MockRoute {
                id,
                pattern: pat.clone(),
                route_type: MockType::Block,
                status: 0,
                body: String::new(),
                headers: HashMap::new(),
                delay_ms: 0,
                content_type: String::new(),
            });
        }
    }

    ensure_fetch_enabled(page, state).await?;

    debug!("block_urls: added {count} block patterns");

    Ok(json!({
        "blocked": count,
        "patterns": pattern_strs,
    }))
}

/// Handle `clear_routes` command — remove all routes and disable Fetch domain.
/// Params: {}
pub async fn handle_clear_routes(
    _page: &Page,
    state: &DaemonState,
    _params: &Value,
) -> Result<Value, String> {
    let cleared;
    {
        let mut store = state.mock_routes.lock().unwrap();
        cleared = store.routes.len();
        store.routes.clear();
        // Note: we don't disable the Fetch domain or kill the listener since
        // they are designed to be long-lived. With no routes, the listener
        // just calls ContinueRequest for everything (no-op pass-through).
    }

    debug!("clear_routes: cleared {cleared} routes");

    Ok(json!({
        "cleared": cleared,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_exact_match() {
        assert!(glob_matches("https://example.com/api/users", "https://example.com/api/users"));
        assert!(!glob_matches("https://example.com/api/users", "https://example.com/api/posts"));
    }

    #[test]
    fn glob_star_match() {
        assert!(glob_matches("**/api/users*", "https://example.com/api/users?page=1"));
        assert!(glob_matches("**/api/*", "https://example.com/api/anything"));
        assert!(glob_matches("https://example.com/*", "https://example.com/anything"));
    }

    #[test]
    fn glob_double_star_match() {
        assert!(glob_matches("**/analytics**", "https://example.com/analytics/track?id=1"));
        assert!(glob_matches("**analytics**", "https://example.com/analytics"));
        assert!(!glob_matches("**/analytics**", "https://example.com/api/users"));
    }

    #[test]
    fn glob_no_match() {
        assert!(!glob_matches("**/ads*", "https://example.com/api/data"));
    }
}
