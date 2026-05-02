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
