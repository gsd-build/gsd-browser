//! CDP Tracing domain — start/stop performance traces.

use crate::state::DaemonState;
use browser_tools_common::state_dir;
use chromiumoxide::cdp::browser_protocol::tracing::{
    EndParams as TracingEndParams, EventDataCollected, EventTracingComplete,
    StartParams as TracingStartParams,
};
use chromiumoxide::Page;
use futures::StreamExt;
use serde_json::{json, Value};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// Start a CDP trace session.
pub async fn handle_trace_start(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Check if a trace is already active
    {
        let ts = state.trace_state.lock().unwrap();
        if ts.active {
            return Err("trace already active — call trace_stop first".to_string());
        }
    }

    // Start tracing via CDP — use ReportEvents transfer mode so we get dataCollected events
    let start_params = TracingStartParams::builder().build();

    page.execute(start_params)
        .await
        .map_err(|e| format!("Tracing.start failed: {e}"))?;

    // Update state
    {
        let mut ts = state.trace_state.lock().unwrap();
        ts.active = true;
        ts.name = name.clone();
        ts.started_at = now_secs();
    }

    Ok(json!({
        "started": true,
        "name": name,
    }))
}

/// Stop a CDP trace session, collect data, and write to file.
pub async fn handle_trace_stop(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let output_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Check if trace is active
    let (trace_name, started_at) = {
        let ts = state.trace_state.lock().unwrap();
        if !ts.active {
            return Err("no active trace — call trace_start first".to_string());
        }
        (ts.name.clone(), ts.started_at)
    };

    // Set up data collector BEFORE sending Tracing.end
    let mut data_stream = page
        .event_listener::<EventDataCollected>()
        .await
        .map_err(|e| format!("failed to listen for Tracing.dataCollected: {e}"))?;

    let mut complete_stream = page
        .event_listener::<EventTracingComplete>()
        .await
        .map_err(|e| format!("failed to listen for Tracing.tracingComplete: {e}"))?;

    // Send Tracing.end
    page.execute(TracingEndParams::default())
        .await
        .map_err(|e| format!("Tracing.end failed: {e}"))?;

    // Collect trace data from dataCollected events until tracingComplete fires
    let mut trace_data: Vec<Value> = Vec::new();

    // Use a timeout to avoid hanging forever
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(30);

    loop {
        tokio::select! {
            Some(event) = data_stream.next() => {
                trace_data.extend(event.value.clone());
            }
            Some(_complete) = complete_stream.next() => {
                // Drain any remaining data events
                while let Ok(Some(event)) = tokio::time::timeout(
                    tokio::time::Duration::from_millis(100),
                    data_stream.next(),
                ).await {
                    trace_data.extend(event.value.clone());
                }
                break;
            }
            _ = tokio::time::sleep_until(deadline) => {
                // Timeout — use whatever we have
                tracing::warn!("trace_stop: timeout waiting for tracingComplete");
                break;
            }
        }
    }

    // Update state
    {
        let mut ts = state.trace_state.lock().unwrap();
        ts.active = false;
        ts.name = None;
        ts.started_at = 0.0;
    }

    let duration = now_secs() - started_at;

    // Write trace data to file
    let dir = state_dir().join("traces");
    let _ = fs::create_dir_all(&dir);

    let file_name = output_name
        .or(trace_name)
        .unwrap_or_else(|| "trace".to_string());

    let ts_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let path = dir
        .join(format!("{file_name}-{ts_suffix}.json"))
        .to_string_lossy()
        .to_string();

    let trace_json = json!({
        "traceEvents": trace_data,
    });

    let json_str = serde_json::to_string(&trace_json)
        .map_err(|e| format!("failed to serialize trace: {e}"))?;

    fs::write(&path, &json_str)
        .map_err(|e| format!("failed to write trace file: {e}"))?;

    Ok(json!({
        "path": path,
        "events": trace_data.len(),
        "duration": duration,
        "size": json_str.len(),
    }))
}
