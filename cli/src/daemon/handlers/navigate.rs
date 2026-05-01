//! Navigation command handlers: navigate, back, forward, reload.
//!
//! Each handler receives a shared `Page` reference and `DaemonState`, performs
//! the CDP action, settles the DOM, captures compact state, updates the
//! PageRegistry metadata, and returns a JSON result.

use crate::daemon::capture::capture_compact_page_state;
use crate::daemon::narration::events::{now_ms, ActionKind, NarrationEvent};
use crate::daemon::settle::{ensure_mutation_counter, settle_after_action};
use crate::daemon::state::DaemonState;
use chromiumoxide::cdp::browser_protocol::page::{
    GetNavigationHistoryParams, NavigateToHistoryEntryParams,
};
use chromiumoxide::Page;
use gsd_browser_common::types::SettleOptions;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;
use tracing::debug;

/// Update the PageRegistry metadata for the active page after navigation.
fn sync_page_registry(state: &DaemonState, title: &str, url: &str) {
    let mut pages = state.pages.lock().unwrap();
    let active_id = pages.active_page_id;
    pages.update_metadata(active_id, title.to_string(), url.to_string());
}

/// Navigate to a URL. Expects params: `{ "url": "..." }`.
pub async fn handle_navigate(
    page: &Page,
    params: &Value,
    state: &DaemonState,
) -> Result<Value, String> {
    let url = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: url".to_string())?;

    debug!("navigate: url={url}");

    let probe = state
        .narrator
        .probe_action(page, ActionKind::Navigate, None, Some(url))
        .await;
    state
        .narrator
        .emit_pre(&probe)
        .await
        .map_err(|_| "aborted".to_string())?;
    state.narrator.sleep_lead(&probe).await;

    let result = async {
        timeout(Duration::from_secs(30), page.goto(url))
            .await
            .map_err(|_| format!("navigation timed out after 30s for: {url}"))?
            .map_err(|e| format!("navigation failed: {e}"))?;

        tokio::time::sleep(Duration::from_millis(300)).await;
        ensure_mutation_counter(page).await;

        let settle_opts = SettleOptions {
            timeout_ms: 2000,
            ..SettleOptions::default()
        };
        let settle = settle_after_action(page, &settle_opts).await;

        let page_state = capture_compact_page_state(page, true).await;
        sync_page_registry(state, &page_state.title, &page_state.url);

        let _ = state.narrator.bus.send(NarrationEvent::TabChanged {
            url: page_state.url.clone(),
            target_id: String::new(),
            timestamp_ms: now_ms(),
        });

        Ok(json!({
            "url": page_state.url,
            "title": page_state.title,
            "settle": settle,
            "state": page_state,
        }))
    }
    .await;
    state.narrator.emit_post(&probe, &result).await;
    result
}

/// Go back in browser history.
pub async fn handle_back(page: &Page, state: &DaemonState) -> Result<Value, String> {
    debug!("back: navigating to previous page");

    // Get navigation history to check if back is possible
    let history = timeout(
        Duration::from_secs(5),
        page.execute(GetNavigationHistoryParams::default()),
    )
    .await
    .map_err(|_| "timeout getting navigation history".to_string())?
    .map_err(|e| format!("failed to get navigation history: {e}"))?;

    if history.current_index <= 0 {
        return Err("No previous page in history".to_string());
    }

    // Navigate to previous entry
    let target_index = (history.current_index - 1) as usize;
    let target_entry = &history.entries[target_index];
    let nav_params = NavigateToHistoryEntryParams::new(target_entry.id);

    timeout(Duration::from_secs(10), page.execute(nav_params))
        .await
        .map_err(|_| "timeout navigating back".to_string())?
        .map_err(|e| format!("failed to navigate back: {e}"))?;

    // Wait for navigation to settle
    tokio::time::sleep(Duration::from_millis(500)).await;
    ensure_mutation_counter(page).await;

    let settle_opts = SettleOptions {
        timeout_ms: 2000,
        ..SettleOptions::default()
    };
    let settle = settle_after_action(page, &settle_opts).await;
    let page_state = capture_compact_page_state(page, true).await;
    sync_page_registry(state, &page_state.title, &page_state.url);

    Ok(json!({
        "url": page_state.url,
        "title": page_state.title,
        "settle": settle,
        "state": page_state,
    }))
}

/// Go forward in browser history.
pub async fn handle_forward(page: &Page, state: &DaemonState) -> Result<Value, String> {
    debug!("forward: navigating to next page");

    let history = timeout(
        Duration::from_secs(5),
        page.execute(GetNavigationHistoryParams::default()),
    )
    .await
    .map_err(|_| "timeout getting navigation history".to_string())?
    .map_err(|e| format!("failed to get navigation history: {e}"))?;

    let max_index = (history.entries.len() as i64) - 1;
    if history.current_index >= max_index {
        return Err("No forward page in history".to_string());
    }

    let target_index = (history.current_index + 1) as usize;
    let target_entry = &history.entries[target_index];
    let nav_params = NavigateToHistoryEntryParams::new(target_entry.id);

    timeout(Duration::from_secs(10), page.execute(nav_params))
        .await
        .map_err(|_| "timeout navigating forward".to_string())?
        .map_err(|e| format!("failed to navigate forward: {e}"))?;

    tokio::time::sleep(Duration::from_millis(500)).await;
    ensure_mutation_counter(page).await;

    let settle_opts = SettleOptions {
        timeout_ms: 2000,
        ..SettleOptions::default()
    };
    let settle = settle_after_action(page, &settle_opts).await;
    let page_state = capture_compact_page_state(page, true).await;
    sync_page_registry(state, &page_state.title, &page_state.url);

    Ok(json!({
        "url": page_state.url,
        "title": page_state.title,
        "settle": settle,
        "state": page_state,
    }))
}

/// Reload the current page.
pub async fn handle_reload(page: &Page, state: &DaemonState) -> Result<Value, String> {
    debug!("reload: refreshing current page");

    // page.reload() handles the CDP reload + wait_for_navigation
    timeout(Duration::from_secs(30), page.reload())
        .await
        .map_err(|_| "reload timed out after 30s".to_string())?
        .map_err(|e| format!("reload failed: {e}"))?;

    tokio::time::sleep(Duration::from_millis(300)).await;
    ensure_mutation_counter(page).await;

    let settle_opts = SettleOptions {
        timeout_ms: 2000,
        ..SettleOptions::default()
    };
    let settle = settle_after_action(page, &settle_opts).await;
    let page_state = capture_compact_page_state(page, true).await;
    sync_page_registry(state, &page_state.title, &page_state.url);

    Ok(json!({
        "url": page_state.url,
        "title": page_state.title,
        "settle": settle,
        "state": page_state,
    }))
}
