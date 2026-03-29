//! Handlers for multi-page management and frame switching.
//!
//! These handlers operate on the PageRegistry in DaemonState, not directly on
//! a single Page reference. This is the only handler module that accesses
//! the registry — all other handlers receive a resolved `&Page`.

use crate::state::DaemonState;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// List all open pages with their id, title, url, and active status.
pub fn handle_list_pages(state: &DaemonState) -> Result<Value, String> {
    let pages = state.pages.lock().unwrap();
    let active_id = pages.active_page_id;

    let entries: Vec<Value> = pages
        .entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "title": e.title,
                "url": e.url,
                "isActive": e.id == active_id,
            })
        })
        .collect();

    Ok(json!({
        "pages": entries,
        "count": entries.len(),
        "activePageId": active_id,
    }))
}

/// Switch the active page. Clears selected_frame. Re-injects helpers on the new active page.
pub async fn handle_switch_page(
    state: &DaemonState,
    params: &Value,
) -> Result<(Value, Arc<Page>), String> {
    let id = params
        .get("id")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing required parameter 'id'".to_string())?;

    // Switch active page in registry
    let new_page = {
        let mut pages = state.pages.lock().unwrap();
        if !pages.set_active(id) {
            return Err(format!("page id {id} not found"));
        }
        pages
            .active_page()
            .ok_or_else(|| "failed to resolve active page".to_string())?
    };

    // Clear selected frame
    {
        let mut frame = state.selected_frame.lock().unwrap();
        *frame = None;
    }

    // Re-inject helpers and mutation counter on the newly active page
    crate::helpers::inject_helpers(&new_page).await;
    crate::settle::ensure_mutation_counter(&new_page).await;

    // Read current title/url from the page
    let url = new_page
        .url()
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let title = new_page
        .evaluate("document.title")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
        .unwrap_or_default();

    // Update stored metadata
    {
        let mut pages = state.pages.lock().unwrap();
        pages.update_metadata(id, title.clone(), url.clone());
    }

    info!("[pages] switched to page {id}: {url}");

    Ok((
        json!({
            "switched": true,
            "id": id,
            "title": title,
            "url": url,
        }),
        new_page,
    ))
}

/// Close a page by ID. Cannot close the last remaining page.
/// Falls back active to another page if the closed page was active.
pub async fn handle_close_page(
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let id = params
        .get("id")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing required parameter 'id'".to_string())?;

    let (removed_page, new_active_id) = {
        let mut pages = state.pages.lock().unwrap();
        let removed = pages.remove(id)?;
        let new_active = pages.active_page_id;
        (removed, new_active)
    };

    // Clear selected frame since page context changed
    {
        let mut frame = state.selected_frame.lock().unwrap();
        *frame = None;
    }

    // Close the CDP page — best-effort, don't fail if it errors
    // Arc::try_unwrap may fail if there are still references held
    match Arc::try_unwrap(removed_page) {
        Ok(page) => {
            if let Err(e) = page.close().await {
                warn!("[pages] close page {id} CDP error (non-fatal): {e}");
            }
        }
        Err(_arc) => {
            // Other references exist — just drop and let them clean up
            warn!("[pages] close page {id}: could not unwrap Arc, dropping reference");
        }
    }

    info!("[pages] closed page {id}, active now: {new_active_id}");

    Ok(json!({
        "closed": true,
        "id": id,
        "activePageId": new_active_id,
    }))
}

/// List all frames in the active page by walking window.frames recursively via JS.
pub async fn handle_list_frames(page: &Page) -> Result<Value, String> {
    let js = r#"(function() {
        var results = [];
        function walk(win, parentName, depth) {
            if (depth > 10) return;
            try {
                var name = '';
                try { name = win.name || ''; } catch(e) {}
                var url = '';
                try { url = win.location.href || ''; } catch(e) { url = '(cross-origin)'; }
                var isMain = (win === window.top);
                results.push({
                    index: results.length,
                    name: name,
                    url: url,
                    isMain: isMain,
                    parentName: parentName
                });
                for (var i = 0; i < win.frames.length; i++) {
                    try {
                        walk(win.frames[i], name || ('frame-' + results.length), depth + 1);
                    } catch(e) {}
                }
            } catch(e) {}
        }
        walk(window.top, '', 0);
        return JSON.stringify(results);
    })()"#;

    let raw = page
        .evaluate(js)
        .await
        .map_err(|e| format!("list_frames JS eval failed: {}", super::clean_cdp_error(&e)))?;

    let json_str = raw
        .into_value::<String>()
        .map_err(|e| format!("list_frames parse error: {e}"))?;

    let frames: Value =
        serde_json::from_str(&json_str).map_err(|e| format!("list_frames JSON parse: {e}"))?;

    let count = frames.as_array().map(|a| a.len()).unwrap_or(0);

    Ok(json!({
        "frames": frames,
        "count": count,
    }))
}

/// Select a frame for subsequent JS evaluations.
/// Pass name="main" or null to reset to the main frame.
pub fn handle_select_frame(
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let name = params.get("name").and_then(|v| v.as_str());
    let index = params.get("index").and_then(|v| v.as_u64());
    let url_pattern = params.get("urlPattern").and_then(|v| v.as_str());

    // Determine the frame identifier to store
    let frame_id = if let Some(n) = name {
        if n == "main" || n == "null" || n.is_empty() {
            None // Reset to main frame
        } else {
            Some(format!("name:{n}"))
        }
    } else if let Some(idx) = index {
        Some(format!("index:{idx}"))
    } else if let Some(pat) = url_pattern {
        Some(format!("url:{pat}"))
    } else {
        // No params = reset to main
        None
    };

    let selected = frame_id.is_some();
    let label = frame_id.clone().unwrap_or_else(|| "main".to_string());

    {
        let mut frame = state.selected_frame.lock().unwrap();
        *frame = frame_id;
    }

    debug!("[pages] selected frame: {label}");

    Ok(json!({
        "selected": selected,
        "frame": label,
    }))
}
