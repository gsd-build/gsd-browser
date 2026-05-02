use crate::daemon::narration::events::ControlState;
use crate::daemon::state::DaemonState;
use serde_json::{json, Value};

pub async fn handle_goal(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let clear = params
        .get("clear")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if clear {
        state.narrator.set_goal(None).await;
        return Ok(json!({"goal": null}));
    }

    let text = params
        .get("text")
        .and_then(|value| value.as_str())
        .ok_or("missing 'text' or 'clear: true'")?;
    state.narrator.set_goal(Some(text.to_string())).await;
    Ok(json!({"goal": text}))
}

pub async fn handle_pause(state: &DaemonState) -> Result<Value, String> {
    state.narrator.set_control(ControlState::Paused).await;
    let control = state.view_control.lock().await.pause("paused by command")?;
    serde_json::to_value(control).map_err(|err| err.to_string())
}

pub async fn handle_resume(state: &DaemonState) -> Result<Value, String> {
    state.narrator.set_control(ControlState::Running).await;
    let control = state
        .view_control
        .lock()
        .await
        .release("resumed by command")?;
    serde_json::to_value(control).map_err(|err| err.to_string())
}

pub async fn handle_step(state: &DaemonState) -> Result<Value, String> {
    state.narrator.set_control(ControlState::Step).await;
    let control = state.view_control.lock().await.step("step by command")?;
    serde_json::to_value(control).map_err(|err| err.to_string())
}

pub async fn handle_abort(state: &DaemonState) -> Result<Value, String> {
    state.narrator.set_control(ControlState::Aborted).await;
    let control = state
        .view_control
        .lock()
        .await
        .abort("aborted by command")?;
    serde_json::to_value(control).map_err(|err| err.to_string())
}

pub async fn handle_view_status(state: &DaemonState) -> Result<Value, String> {
    let goal = state.narrator.current_goal().await;
    let control = state.view_control.lock().await.snapshot();
    Ok(json!({
        "goal": goal,
        "control": control,
    }))
}

pub async fn handle_control_state(state: &DaemonState) -> Result<Value, String> {
    let control = state.view_control.lock().await.snapshot();
    serde_json::to_value(control).map_err(|err| err.to_string())
}

pub async fn handle_takeover(state: &DaemonState) -> Result<Value, String> {
    let control = state
        .view_control
        .lock()
        .await
        .takeover("takeover by command")?;
    serde_json::to_value(control).map_err(|err| err.to_string())
}

pub async fn handle_release_control(state: &DaemonState) -> Result<Value, String> {
    state.narrator.set_control(ControlState::Running).await;
    let control = state
        .view_control
        .lock()
        .await
        .release("released by command")?;
    serde_json::to_value(control).map_err(|err| err.to_string())
}

pub async fn handle_sensitive_on(state: &DaemonState) -> Result<Value, String> {
    let control = state
        .view_control
        .lock()
        .await
        .sensitive_on("sensitive mode enabled")?;
    serde_json::to_value(control).map_err(|err| err.to_string())
}

pub async fn handle_sensitive_off(state: &DaemonState) -> Result<Value, String> {
    let control = state
        .view_control
        .lock()
        .await
        .sensitive_off("sensitive mode disabled")?;
    serde_json::to_value(control).map_err(|err| err.to_string())
}

pub async fn handle_view(
    state: &std::sync::Arc<DaemonState>,
    page: &chromiumoxide::Page,
    browser: &std::sync::Arc<tokio::sync::Mutex<chromiumoxide::Browser>>,
) -> Result<Value, String> {
    let mut guard = state.view_server.lock().await;
    if let Some(handle) = guard.as_ref() {
        return Ok(json!({"url": handle.url, "port": handle.port, "started": false}));
    }

    let handle = crate::daemon::view::start_for_session(
        state.clone(),
        state.narrator.clone(),
        std::sync::Arc::new(page.clone()),
        browser.clone(),
    )
    .await?;
    let url = handle.url.clone();
    let port = handle.port;
    *guard = Some(handle);
    Ok(json!({"url": url, "port": port, "started": true}))
}

pub async fn handle_annotations(state: &DaemonState) -> Result<Value, String> {
    let annotations = state.annotations.lock().await.list();
    Ok(json!({ "annotations": annotations }))
}

pub async fn handle_annotation_get(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or("annotation_get requires id")?;
    let annotation = state
        .annotations
        .lock()
        .await
        .get(id)
        .ok_or_else(|| format!("annotation not found: {id}"))?;
    serde_json::to_value(annotation).map_err(|err| err.to_string())
}

pub async fn handle_annotation_clear(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let all = params.get("all").and_then(Value::as_bool).unwrap_or(false);
    let mut store = state.annotations.lock().await;
    if all {
        store.clear_all();
        return Ok(json!({ "cleared": "all" }));
    }
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or("annotation_clear requires id or all")?;
    Ok(json!({ "cleared": store.clear(id), "id": id }))
}

pub async fn handle_annotation_resolve(
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or("annotation_resolve requires id")?;
    let annotation = state
        .annotations
        .lock()
        .await
        .set_status(id, gsd_browser_common::viewer::AnnotationStatus::Resolved)?;
    serde_json::to_value(annotation).map_err(|err| err.to_string())
}

pub async fn handle_annotation_export(
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let output = params
        .get("output")
        .and_then(Value::as_str)
        .ok_or("annotation_export requires output")?;
    let annotations = state.annotations.lock().await.list();
    let json = serde_json::to_string_pretty(&annotations).map_err(|err| err.to_string())?;
    std::fs::write(output, json).map_err(|err| format!("failed to write annotations: {err}"))?;
    Ok(json!({ "output": output, "count": annotations.len() }))
}

pub async fn handle_annotation_request(
    _state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let note = params
        .get("note")
        .and_then(Value::as_str)
        .ok_or("annotation_request requires note")?;
    Ok(json!({ "pending": true, "note": note }))
}

pub async fn handle_record_start(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("recording");
    let mut store = state.recordings.lock().await;
    let session = store.start(name)?;
    store.record_event(crate::daemon::view::recording::RecordingEventInput {
        source: "cli".to_string(),
        owner: "agent".to_string(),
        kind: "recording.start".to_string(),
        url: String::new(),
        title: name.to_string(),
        redacted: false,
    })?;
    serde_json::to_value(session).map_err(|err| err.to_string())
}

pub async fn handle_record_stop(state: &DaemonState) -> Result<Value, String> {
    let mut store = state.recordings.lock().await;
    let id = store.active_id().ok_or("no active recording")?;
    store.record_event(crate::daemon::view::recording::RecordingEventInput {
        source: "cli".to_string(),
        owner: "agent".to_string(),
        kind: "recording.stop".to_string(),
        url: String::new(),
        title: String::new(),
        redacted: false,
    })?;
    let manifest = store.stop(&id)?;
    serde_json::to_value(manifest).map_err(|err| err.to_string())
}

pub async fn handle_record_pause(state: &DaemonState) -> Result<Value, String> {
    let mut store = state.recordings.lock().await;
    let id = store.active_id().ok_or("no active recording")?;
    let session = store.pause(&id)?;
    serde_json::to_value(session).map_err(|err| err.to_string())
}

pub async fn handle_record_resume(state: &DaemonState) -> Result<Value, String> {
    let mut store = state.recordings.lock().await;
    let id = store.active_id().ok_or("no active recording")?;
    let session = store.resume(&id)?;
    serde_json::to_value(session).map_err(|err| err.to_string())
}

pub async fn handle_recordings(state: &DaemonState) -> Result<Value, String> {
    let recordings = state.recordings.lock().await.list();
    Ok(json!({ "recordings": recordings }))
}

pub async fn handle_recording_get(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or("recording_get requires id")?;
    let manifest = state
        .recordings
        .lock()
        .await
        .get(id)
        .ok_or_else(|| format!("recording not found: {id}"))?;
    serde_json::to_value(manifest).map_err(|err| err.to_string())
}

pub async fn handle_recording_discard(
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or("recording_discard requires id")?;
    let discarded = state.recordings.lock().await.discard(id)?;
    Ok(json!({ "discarded": discarded, "id": id }))
}

pub async fn handle_recording_export(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or("recording_export requires id")?;
    let output = params
        .get("output")
        .and_then(Value::as_str)
        .ok_or("recording_export requires output")?;
    let path = state
        .recordings
        .lock()
        .await
        .export(id, std::path::Path::new(output))?;
    Ok(json!({ "path": path }))
}

pub async fn handle_recording_validate(params: &Value) -> Result<Value, String> {
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or("recording_validate requires path")?;
    let supplied = std::path::Path::new(path);
    let resolved = if supplied.exists() {
        supplied.to_path_buf()
    } else {
        gsd_browser_common::state_dir()
            .join("recordings")
            .join(path)
    };
    crate::daemon::view::recording::validate_recording_bundle(&resolved)
}
