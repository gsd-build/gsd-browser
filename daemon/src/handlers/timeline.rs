//! Timeline query handler — returns action timeline entries as JSON.

use crate::state::DaemonState;
use serde_json::{json, Value};
use tracing::debug;

/// Handle a `timeline` request. Returns the action timeline entries.
pub fn handle_timeline(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let write_to_disk = params
        .get("write_to_disk")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let timeline = state.timeline.lock().map_err(|e| format!("lock error: {e}"))?;
    let entries = timeline.snapshot();

    debug!("timeline: returning {} entries", entries.len());

    let result = json!({
        "entries": entries,
        "count": entries.len(),
    });

    if write_to_disk {
        if let Ok(json_str) = serde_json::to_string_pretty(&result) {
            let path = browser_tools_common::state_dir().join("timeline.json");
            if let Err(e) = std::fs::write(&path, json_str) {
                return Err(format!("failed to write timeline to disk: {e}"));
            }
            debug!("timeline: written to {:?}", path);
        }
    }

    Ok(result)
}
