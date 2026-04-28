//! Session diagnostics: session-summary and debug-bundle handlers.
//!
//! `debug_bundle` collects all diagnostic artifacts into a timestamped directory.

use crate::daemon::logs::DaemonLogs;
use crate::daemon::state::DaemonState;
use base64::Engine as _;
use chromiumoxide::Page;
use chrono::Local;
use gsd_browser_common::session::{
    load_session_manifest, manifest_path_for, now_epoch_secs, save_session_manifest,
    SessionHealthStatus, SessionManifest,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process;
use tracing::{debug, warn};

/// Maximum timeline entries cap (must match state.rs MAX_TIMELINE_ENTRIES).
const MAX_TIMELINE_ENTRIES: usize = 60;

fn build_summary_aggregation(logs: &DaemonLogs, state: &DaemonState) -> Value {
    debug!("handle_session_summary: aggregating state");

    // Timeline aggregation
    let timeline = state.timeline.lock().unwrap();
    let entries = timeline.snapshot();
    let total_actions = entries.len();
    let mut status_ok = 0u64;
    let mut status_error = 0u64;
    let mut status_running = 0u64;
    let mut wait_count = 0u64;
    let mut assert_count = 0u64;

    for entry in &entries {
        match entry.status.as_str() {
            "ok" => status_ok += 1,
            "error" => status_error += 1,
            "running" => status_running += 1,
            _ => {}
        }
        match entry.tool.as_str() {
            "wait_for" => wait_count += 1,
            "assert" => assert_count += 1,
            _ => {}
        }
    }

    let bounded_history = total_actions >= MAX_TIMELINE_ENTRIES;
    drop(timeline);

    // Log aggregation (snapshot, not drain — non-destructive)
    let console_entries = logs.console.snapshot();
    let console_error_count = console_entries
        .iter()
        .filter(|e| e.log_type == "error" || e.log_type == "pageerror")
        .count();
    let console_total = console_entries.len();

    let network_entries = logs.network.snapshot();
    let failed_network_count = network_entries
        .iter()
        .filter(|e| e.failed || e.status >= 400)
        .count();
    let network_total = network_entries.len();

    let dialog_entries = logs.dialog.snapshot();
    let dialog_count = dialog_entries.len();

    // Active page info
    let pages = state.pages.lock().unwrap();
    let (active_url, active_title, page_count, active_page_id) = {
        let active = pages.entries.iter().find(|e| e.id == pages.active_page_id);
        match active {
            Some(entry) => (
                entry.url.clone(),
                entry.title.clone(),
                pages.entries.len(),
                pages.active_page_id,
            ),
            None => (String::new(), String::new(), pages.entries.len(), 0),
        }
    };
    drop(pages);

    // Selected frame
    let selected_frame = state
        .selected_frame
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();

    json!({
        "actions": {
            "total": total_actions,
            "ok": status_ok,
            "error": status_error,
            "running": status_running,
            "waitCount": wait_count,
            "assertCount": assert_count,
        },
        "console": {
            "total": console_total,
            "errors": console_error_count,
        },
        "network": {
            "total": network_total,
            "failed": failed_network_count,
        },
        "dialog": {
            "total": dialog_count,
        },
        "activePage": {
            "id": active_page_id,
            "url": active_url,
            "title": active_title,
        },
        "pageCount": page_count,
        "selectedFrame": if selected_frame.is_empty() { Value::Null } else { Value::String(selected_frame) },
        "boundedHistory": bounded_history,
        "boundedHistoryCaveat": if bounded_history {
            format!("Timeline capped at {MAX_TIMELINE_ENTRIES} entries — oldest actions evicted")
        } else {
            String::new()
        },
    })
}

async fn probe_active_page(page: &Page) -> (bool, String, String) {
    let url = match page.url().await {
        Ok(Some(url)) => url,
        Ok(None) => String::new(),
        Err(err) => {
            return (
                false,
                String::new(),
                format!("page URL probe failed: {err}"),
            );
        }
    };

    let title = page
        .evaluate_expression("document.title")
        .await
        .ok()
        .and_then(|value| value.into_value::<String>().ok())
        .unwrap_or_default();

    (true, url, title)
}

fn current_active_page(state: &DaemonState) -> (u64, usize, String, String) {
    let pages = state.pages.lock().unwrap();
    let active = pages
        .entries
        .iter()
        .find(|entry| entry.id == pages.active_page_id);
    match active {
        Some(entry) => (
            entry.id,
            pages.entries.len(),
            entry.url.clone(),
            entry.title.clone(),
        ),
        None => (0, pages.entries.len(), String::new(), String::new()),
    }
}

fn set_active_page_metadata(state: &DaemonState, id: u64, url: &str, title: &str) {
    let mut pages = state.pages.lock().unwrap();
    if id != 0 {
        pages.update_metadata(id, title.to_string(), url.to_string());
    }
}

fn health_status_from_browser_probe(
    browser_connected: bool,
    active_url: &str,
    reason: &str,
) -> (SessionHealthStatus, String) {
    if !browser_connected {
        return (SessionHealthStatus::Degraded, reason.to_string());
    }
    if active_url.is_empty() {
        return (
            SessionHealthStatus::Degraded,
            "browser is connected but active page URL is unavailable".to_string(),
        );
    }
    (SessionHealthStatus::Healthy, String::new())
}

fn build_and_persist_manifest(
    state: &DaemonState,
    health: SessionHealthStatus,
    reason: String,
    active_page_id: u64,
    active_page_url: String,
    active_page_title: String,
) -> Result<SessionManifest, String> {
    let now = now_epoch_secs();
    let runtime = &state.session;
    let mut manifest = load_session_manifest(runtime.session_name.as_deref())?.unwrap_or_default();
    manifest.manifest_version = 1;
    manifest.session_name = runtime.session_name.clone();
    manifest.daemon_pid = Some(process::id() as i32);
    manifest.browser_pid = runtime.browser_pid;
    manifest.socket_path = runtime.socket_path.clone();
    manifest.daemon_started_at = manifest.daemon_started_at.or(Some(now));
    manifest.browser_started_at = manifest.browser_started_at.or(Some(now));
    manifest.daemon_version = env!("CARGO_PKG_VERSION").to_string();
    manifest.launch_mode = runtime.launch_mode.clone();
    manifest.cdp_url = runtime.cdp_url.clone();
    manifest.websocket_url = runtime.websocket_url.clone();
    manifest.browser_user_data_dir = runtime.browser_user_data_dir.clone();
    manifest.health = health;
    manifest.health_reason = reason;
    manifest.last_heartbeat_at = Some(now);
    manifest.last_updated_at = Some(now);
    manifest.active_page_id = if active_page_id == 0 {
        None
    } else {
        Some(active_page_id)
    };
    manifest.active_page_url = active_page_url;
    manifest.active_page_title = active_page_title;
    save_session_manifest(runtime.session_name.as_deref(), &manifest)?;
    Ok(manifest)
}

fn session_identity_json(state: &DaemonState, manifest: &SessionManifest) -> Value {
    let browser_connected = matches!(
        manifest.health,
        SessionHealthStatus::Healthy | SessionHealthStatus::Recovering
    );
    json!({
        "name": manifest.session_name,
        "status": manifest.health.as_str(),
        "reason": manifest.health_reason,
        "daemonPid": manifest.daemon_pid,
        "browserPid": manifest.browser_pid,
        "socketPath": manifest.socket_path,
        "manifestPath": manifest_path_for(state.session.session_name.as_deref()).to_string_lossy().to_string(),
        "launchMode": manifest.launch_mode,
        "cdpUrl": manifest.cdp_url,
        "websocketUrl": manifest.websocket_url,
        "browserUserDataDir": manifest.browser_user_data_dir,
        "lastHeartbeatAt": manifest.last_heartbeat_at,
        "lastUpdatedAt": manifest.last_updated_at,
        "browserConnected": browser_connected,
        "daemonAlive": true,
        "socketConnected": true,
    })
}

pub async fn sync_session_manifest(
    page: &Page,
    state: &DaemonState,
    health_override: Option<SessionHealthStatus>,
    reason_override: Option<String>,
) -> Result<SessionManifest, String> {
    let (active_page_id, _page_count, registry_url, registry_title) = current_active_page(state);
    let (browser_connected, live_url, probe_reason) = probe_active_page(page).await;
    let live_title = page
        .evaluate_expression("document.title")
        .await
        .ok()
        .and_then(|value| value.into_value::<String>().ok())
        .unwrap_or_else(|| registry_title.clone());
    let final_url = if live_url.is_empty() {
        registry_url
    } else {
        live_url
    };
    let final_title = if live_title.is_empty() {
        registry_title
    } else {
        live_title
    };
    set_active_page_metadata(state, active_page_id, &final_url, &final_title);
    let (derived_health, derived_reason) =
        health_status_from_browser_probe(browser_connected, &final_url, &probe_reason);
    build_and_persist_manifest(
        state,
        health_override.unwrap_or(derived_health),
        reason_override.unwrap_or(derived_reason),
        active_page_id,
        final_url,
        final_title,
    )
}

pub async fn mark_session_stopped(state: &DaemonState, reason: &str) -> Result<(), String> {
    let (active_page_id, _page_count, active_page_url, active_page_title) =
        current_active_page(state);
    let _ = build_and_persist_manifest(
        state,
        SessionHealthStatus::Stopped,
        reason.to_string(),
        active_page_id,
        active_page_url,
        active_page_title,
    )?;
    Ok(())
}

pub async fn handle_health(page: &Page, state: &DaemonState) -> Result<Value, String> {
    let manifest = sync_session_manifest(page, state, None, None).await?;
    Ok(json!({
        "session": session_identity_json(state, &manifest),
        "activePage": {
            "id": manifest.active_page_id.unwrap_or(0),
            "url": manifest.active_page_url,
            "title": manifest.active_page_title,
        },
    }))
}

/// Handle `session_summary` — manifest-backed session health plus daemon logs/state.
pub async fn handle_session_summary(
    page: &Page,
    logs: &DaemonLogs,
    state: &DaemonState,
) -> Result<Value, String> {
    let mut summary = build_summary_aggregation(logs, state);
    let manifest = sync_session_manifest(page, state, None, None).await?;
    summary["session"] = session_identity_json(state, &manifest);
    summary["activePage"] = json!({
        "id": manifest.active_page_id.unwrap_or(0),
        "url": manifest.active_page_url,
        "title": manifest.active_page_title,
    });
    Ok(summary)
}

/// Handle `debug_bundle` — collects all diagnostic artifacts into a timestamped directory.
///
/// Writes: screenshot.jpg, console.json, network.json, dialog.json, timeline.json,
/// session-summary.json, accessibility-tree.md
///
/// Returns `{path, files: [...]}` listing what was written.
pub async fn handle_debug_bundle(
    page: &Page,
    logs: &DaemonLogs,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let custom_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");

    // Determine artifact root
    let artifact_root = std::env::var("GSD_BROWSER_ARTIFACT_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".gsd-browser")
                .join("artifacts")
        });

    // Create timestamped directory
    let timestamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let dir_name = if custom_name.is_empty() {
        format!("debug-{timestamp}")
    } else {
        format!("debug-{timestamp}-{custom_name}")
    };
    let bundle_dir = artifact_root.join(&dir_name);

    fs::create_dir_all(&bundle_dir)
        .map_err(|e| format!("failed to create debug bundle directory: {e}"))?;

    debug!("handle_debug_bundle: writing to {}", bundle_dir.display());

    let mut files_written: Vec<String> = Vec::new();

    // 1. Screenshot
    match super::screenshot::handle_screenshot(page, &json!({"format": "jpeg", "quality": 80}))
        .await
    {
        Ok(screenshot_result) => {
            if let Some(data_b64) = screenshot_result.get("data").and_then(|v| v.as_str()) {
                match base64::engine::general_purpose::STANDARD.decode(data_b64) {
                    Ok(bytes) => {
                        let path = bundle_dir.join("screenshot.jpg");
                        if let Err(e) = fs::write(&path, &bytes) {
                            warn!("debug_bundle: failed to write screenshot: {e}");
                        } else {
                            files_written.push("screenshot.jpg".to_string());
                        }
                    }
                    Err(e) => warn!("debug_bundle: failed to decode screenshot base64: {e}"),
                }
            }
        }
        Err(e) => warn!("debug_bundle: screenshot failed: {e}"),
    }

    // 2. Console logs
    {
        let entries = logs.console.snapshot();
        let json_str = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string());
        let path = bundle_dir.join("console.json");
        if let Err(e) = fs::write(&path, &json_str) {
            warn!("debug_bundle: failed to write console.json: {e}");
        } else {
            files_written.push("console.json".to_string());
        }
    }

    // 3. Network logs
    {
        let entries = logs.network.snapshot();
        let json_str = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string());
        let path = bundle_dir.join("network.json");
        if let Err(e) = fs::write(&path, &json_str) {
            warn!("debug_bundle: failed to write network.json: {e}");
        } else {
            files_written.push("network.json".to_string());
        }
    }

    // 4. Dialog logs
    {
        let entries = logs.dialog.snapshot();
        let json_str = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string());
        let path = bundle_dir.join("dialog.json");
        if let Err(e) = fs::write(&path, &json_str) {
            warn!("debug_bundle: failed to write dialog.json: {e}");
        } else {
            files_written.push("dialog.json".to_string());
        }
    }

    // 5. Timeline
    {
        let timeline = state.timeline.lock().unwrap();
        let entries = timeline.snapshot();
        let json_str = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string());
        let path = bundle_dir.join("timeline.json");
        if let Err(e) = fs::write(&path, &json_str) {
            warn!("debug_bundle: failed to write timeline.json: {e}");
        } else {
            files_written.push("timeline.json".to_string());
        }
    }

    // 6. Session summary
    match handle_session_summary(page, logs, state).await {
        Ok(summary) => {
            let json_str =
                serde_json::to_string_pretty(&summary).unwrap_or_else(|_| "{}".to_string());
            let path = bundle_dir.join("session-summary.json");
            if let Err(e) = fs::write(&path, &json_str) {
                warn!("debug_bundle: failed to write session-summary.json: {e}");
            } else {
                files_written.push("session-summary.json".to_string());
            }
        }
        Err(e) => warn!("debug_bundle: session summary failed: {e}"),
    }

    // 7. Accessibility tree
    match super::inspect::handle_accessibility_tree(
        page,
        &json!({"max_depth": 10, "max_count": 200}),
    )
    .await
    {
        Ok(tree_result) => {
            let tree_text = tree_result
                .get("tree")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let node_count = tree_result
                .get("nodeCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let truncated = tree_result
                .get("truncated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let md = format!(
                "# Accessibility Tree\n\n{}\n\n---\n{} nodes{}",
                tree_text,
                node_count,
                if truncated { " (truncated)" } else { "" }
            );
            let path = bundle_dir.join("accessibility-tree.md");
            if let Err(e) = fs::write(&path, &md) {
                warn!("debug_bundle: failed to write accessibility-tree.md: {e}");
            } else {
                files_written.push("accessibility-tree.md".to_string());
            }
        }
        Err(e) => warn!("debug_bundle: accessibility tree failed: {e}"),
    }

    let bundle_path = bundle_dir.to_string_lossy().to_string();
    debug!(
        "handle_debug_bundle: wrote {} files to {}",
        files_written.len(),
        bundle_path
    );

    Ok(json!({
        "path": bundle_path,
        "files": files_written,
        "fileCount": files_written.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::logs::DaemonLogs;
    use crate::daemon::state::DaemonState;
    use gsd_browser_common::types::ConsoleLogEntry;

    #[test]
    fn session_summary_empty_state() {
        let logs = DaemonLogs::new();
        let state = DaemonState::new();
        let result = build_summary_aggregation(&logs, &state);
        assert_eq!(result["actions"]["total"], 0);
        assert_eq!(result["console"]["total"], 0);
        assert_eq!(result["network"]["total"], 0);
        assert_eq!(result["dialog"]["total"], 0);
        assert_eq!(result["boundedHistory"], false);
        assert_eq!(result["pageCount"], 0);
    }

    #[test]
    fn session_summary_with_actions_and_logs() {
        let logs = DaemonLogs::new();
        let state = DaemonState::new();

        // Add some timeline entries
        {
            let mut timeline = state.timeline.lock().unwrap();
            let id = timeline.begin_action("navigate", "url=...", "about:blank");
            timeline.finish_action(id, "https://example.com", "ok", "");
            let id2 = timeline.begin_action("wait_for", "text_visible", "https://example.com");
            timeline.finish_action(id2, "https://example.com", "ok", "");
            let id3 = timeline.begin_action("assert", "checks=[...]", "https://example.com");
            timeline.finish_action(id3, "https://example.com", "error", "failed");
        }

        // Add console errors
        logs.console.push(ConsoleLogEntry {
            log_type: "error".to_string(),
            text: "something broke".to_string(),
            timestamp: 1.0,
            url: String::new(),
        });
        logs.console.push(ConsoleLogEntry {
            log_type: "log".to_string(),
            text: "info msg".to_string(),
            timestamp: 2.0,
            url: String::new(),
        });

        let result = build_summary_aggregation(&logs, &state);
        assert_eq!(result["actions"]["total"], 3);
        assert_eq!(result["actions"]["ok"], 2);
        assert_eq!(result["actions"]["error"], 1);
        assert_eq!(result["actions"]["waitCount"], 1);
        assert_eq!(result["actions"]["assertCount"], 1);
        assert_eq!(result["console"]["total"], 2);
        assert_eq!(result["console"]["errors"], 1);
        assert_eq!(result["boundedHistory"], false);
    }

    #[test]
    fn session_summary_bounded_history() {
        let logs = DaemonLogs::new();
        let state = DaemonState::new();

        // Fill timeline to capacity
        {
            let mut timeline = state.timeline.lock().unwrap();
            for i in 0..MAX_TIMELINE_ENTRIES {
                let id = timeline.begin_action("test", &format!("i={i}"), "");
                timeline.finish_action(id, "", "ok", "");
            }
        }

        let result = build_summary_aggregation(&logs, &state);
        assert_eq!(result["boundedHistory"], true);
        assert!(result["boundedHistoryCaveat"]
            .as_str()
            .unwrap()
            .contains("capped"));
    }
}
