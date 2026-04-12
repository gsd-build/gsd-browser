//! Navigation command handlers: navigate, back, forward, reload.
//!
//! Each handler receives a shared `Page` reference, performs the CDP action,
//! settles the DOM, captures compact state, and returns a JSON result.

use crate::daemon::capture::capture_compact_page_state;
use crate::daemon::settle::{ensure_mutation_counter, settle_after_action};
use chromiumoxide::cdp::browser_protocol::page::{
    GetNavigationHistoryParams, NavigateToHistoryEntryParams,
};
use chromiumoxide::Page;
use gsd_browser_common::types::SettleOptions;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;
use tracing::debug;

/// Navigate to a URL. Expects params: `{ "url": "..." }`.
pub async fn handle_navigate(page: &Page, params: &Value) -> Result<Value, String> {
    let url = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: url".to_string())?;

    debug!("navigate: url={url}");

    // Navigate with 30s timeout — page.goto handles domcontentloaded wait
    timeout(Duration::from_secs(30), page.goto(url))
        .await
        .map_err(|_| format!("navigation timed out after 30s for: {url}"))?
        .map_err(|e| format!("navigation failed: {e}"))?;

    // Brief pause for initial paint, then settle
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Re-inject mutation counter (new document clears it)
    ensure_mutation_counter(page).await;

    let settle_opts = SettleOptions {
        timeout_ms: 2000,
        ..SettleOptions::default()
    };
    let settle = settle_after_action(page, &settle_opts).await;

    // Capture page state
    let state = capture_compact_page_state(page, true).await;

    Ok(json!({
        "url": state.url,
        "title": state.title,
        "settle": settle,
        "state": state,
    }))
}

/// Go back in browser history.
pub async fn handle_back(page: &Page) -> Result<Value, String> {
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
    let state = capture_compact_page_state(page, true).await;

    Ok(json!({
        "url": state.url,
        "title": state.title,
        "settle": settle,
        "state": state,
    }))
}

/// Go forward in browser history.
pub async fn handle_forward(page: &Page) -> Result<Value, String> {
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
    let state = capture_compact_page_state(page, true).await;

    Ok(json!({
        "url": state.url,
        "title": state.title,
        "settle": settle,
        "state": state,
    }))
}

/// Reload the current page.
pub async fn handle_reload(page: &Page) -> Result<Value, String> {
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
    let state = capture_compact_page_state(page, true).await;

    Ok(json!({
        "url": state.url,
        "title": state.title,
        "settle": settle,
        "state": state,
    }))
}
