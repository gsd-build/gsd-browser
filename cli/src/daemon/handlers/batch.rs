//! Batch execution handler: runs a sequence of steps by dispatching to
//! existing handlers. Supports stop-on-failure and summary-only modes.

use crate::daemon::capture::capture_compact_page_state;
use crate::daemon::handlers;
use crate::daemon::logs::DaemonLogs;
use crate::daemon::state::DaemonState;
use chromiumoxide::Page;
use serde_json::{json, Value};
use tracing::debug;

/// Handle a `batch` request. Parses a steps array, dispatches each step
/// to the appropriate handler, and collects results.
pub async fn handle_batch(
    page: &Page,
    logs: &DaemonLogs,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let steps = params
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or("missing 'steps' array")?;

    let stop_on_failure = params
        .get("stop_on_failure")
        .or_else(|| params.get("stopOnFailure"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let summary_only = params
        .get("summary_only")
        .or_else(|| params.get("finalSummaryOnly"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let total_steps = steps.len();
    let mut step_results: Vec<Value> = Vec::with_capacity(total_steps);
    let mut passed = 0usize;
    let mut failed_step: Option<Value> = None;

    for (index, step) in steps.iter().enumerate() {
        let action = step
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        debug!("[batch] step {index}: action={action}");

        let result = dispatch_step(page, logs, state, step, action).await;

        match result {
            Ok(value) => {
                passed += 1;
                step_results.push(json!({
                    "action": action,
                    "index": index,
                    "status": "pass",
                    "result": value,
                }));
            }
            Err(err) => {
                let step_info = json!({
                    "action": action,
                    "index": index,
                    "status": "fail",
                    "error": err,
                });
                step_results.push(step_info.clone());
                if stop_on_failure {
                    failed_step = Some(step_info);
                    break;
                }
            }
        }
    }

    let final_state = capture_compact_page_state(page, false).await;

    let mut response = json!({
        "totalSteps": total_steps,
        "passedSteps": passed,
        "finalSummary": final_state,
    });

    if !summary_only {
        response["steps"] = json!(step_results);
    }

    if let Some(fs) = failed_step {
        response["failedStep"] = fs;
    }

    Ok(response)
}

/// Dispatch a single batch step to the appropriate handler.
/// Some handlers (assert, wait_for) return Ok with a "soft failure" flag
/// (verified: false, met: false). We convert those to Err for batch.
async fn dispatch_step(
    page: &Page,
    logs: &DaemonLogs,
    state: &DaemonState,
    step: &Value,
    action: &str,
) -> Result<Value, String> {
    let result = match action {
        "navigate" => {
            handlers::navigate::handle_navigate(page, step).await
        }
        "click" => {
            handlers::interaction::handle_click(page, step).await
        }
        "type" => {
            handlers::interaction::handle_type_text(page, step).await
        }
        "key_press" => {
            handlers::interaction::handle_press(page, step).await
        }
        "wait_for" => {
            handlers::wait::handle_wait_for(page, logs, step).await
        }
        "assert" => {
            handlers::assert_cmd::handle_assert(page, logs, state, step).await
        }
        "click_ref" => {
            handlers::refs::handle_click_ref(page, state, step).await
        }
        "fill_ref" => {
            handlers::refs::handle_fill_ref(page, state, step).await
        }
        "hover" => {
            handlers::interaction::handle_hover(page, step).await
        }
        "hover_ref" => {
            handlers::refs::handle_hover_ref(page, state, step).await
        }
        "scroll" => {
            handlers::interaction::handle_scroll(page, step).await
        }
        "press" => {
            handlers::interaction::handle_press(page, step).await
        }
        "snapshot" => {
            handlers::refs::handle_snapshot(page, state, step).await
        }
        "diff" => {
            handlers::assert_cmd::handle_diff(page, state, step).await
        }
        _ => Err(format!("unknown batch action: {action}")),
    }?;

    // Check for soft failures: assert with verified=false, wait_for with met=false
    if action == "assert" {
        if let Some(false) = result.get("verified").and_then(|v| v.as_bool()) {
            let summary = result
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("assertion failed");
            return Err(format!("assertion failed: {summary}"));
        }
    }
    if action == "wait_for" {
        if let Some(false) = result.get("met").and_then(|v| v.as_bool()) {
            let condition = result
                .get("condition")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            return Err(format!("wait timeout: condition '{condition}' not met"));
        }
    }

    Ok(result)
}
