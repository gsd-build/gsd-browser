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
    Ok(json!({"control": "paused"}))
}

pub async fn handle_resume(state: &DaemonState) -> Result<Value, String> {
    state.narrator.set_control(ControlState::Running).await;
    Ok(json!({"control": "running"}))
}

pub async fn handle_step(state: &DaemonState) -> Result<Value, String> {
    state.narrator.set_control(ControlState::Step).await;
    Ok(json!({"control": "step"}))
}

pub async fn handle_abort(state: &DaemonState) -> Result<Value, String> {
    state.narrator.set_control(ControlState::Aborted).await;
    Ok(json!({"control": "aborted"}))
}

pub async fn handle_view_status(state: &DaemonState) -> Result<Value, String> {
    let goal = state.narrator.current_goal().await;
    let control = state.narrator.control.get().await;
    Ok(json!({
        "goal": goal,
        "control": control,
    }))
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
