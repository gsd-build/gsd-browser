use chromiumoxide::Page;
use gsd_browser_common::cloud::{CloudFrame, CloudSessionStatus, CloudToolRequest, CloudUserInput};
use gsd_browser_common::identity::{
    identity_metadata_path, identity_profile_dir, BrowserIdentity, IdentityScope,
};
use serde_json::{json, Value};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::daemon::{handlers, logs::DaemonLogs, state::DaemonState};

static FRAME_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn page_url(page: &Page) -> String {
    page.url().await.unwrap_or_default().unwrap_or_default()
}

async fn page_title(page: &Page) -> String {
    page.get_title()
        .await
        .unwrap_or_default()
        .unwrap_or_default()
}

fn runtime_identity(state: &DaemonState) -> Option<BrowserIdentity> {
    let scope = state.session.identity_scope?;
    let key = state.session.identity_key.clone()?;
    Some(BrowserIdentity {
        scope,
        project_id: state.session.identity_project_id.clone(),
        display_name: key.clone(),
        key,
    })
}

pub async fn handle_cloud_session_status(
    page: &Page,
    state: &DaemonState,
) -> Result<Value, String> {
    let status = CloudSessionStatus {
        session_name: state.session.session_name.clone(),
        active_url: page_url(page).await,
        active_title: page_title(page).await,
        identity: runtime_identity(state),
        control_owner: "agent".to_string(),
    };
    serde_json::to_value(status).map_err(|err| err.to_string())
}

pub async fn handle_cloud_frame(page: &Page, params: &Value) -> Result<Value, String> {
    let quality = params.get("quality").and_then(Value::as_u64).unwrap_or(70) as u32;
    let shot = handlers::screenshot::handle_screenshot(
        page,
        &json!({"format": "jpeg", "quality": quality, "full_page": false}),
    )
    .await?;
    let data = shot
        .get("data")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let width = shot
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    let height = shot
        .get("height")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    let frame = CloudFrame {
        sequence: FRAME_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        content_type: "image/jpeg".to_string(),
        data_base64: data,
        width,
        height,
        captured_at_ms: now_ms(),
        url: page_url(page).await,
        title: page_title(page).await,
    };
    serde_json::to_value(frame).map_err(|err| err.to_string())
}

pub async fn handle_cloud_tool(
    page: &Page,
    logs: &DaemonLogs,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let req: CloudToolRequest =
        serde_json::from_value(params.clone()).map_err(|err| err.to_string())?;
    match req.method.as_str() {
        "navigate" => handlers::navigate::handle_navigate(page, &req.params, state).await,
        "back" => handlers::navigate::handle_back(page, state).await,
        "forward" => handlers::navigate::handle_forward(page, state).await,
        "reload" => handlers::navigate::handle_reload(page, state).await,
        "click" => handlers::interaction::handle_click(page, state, &req.params).await,
        "type" => handlers::interaction::handle_type_text(page, state, &req.params).await,
        "press" => handlers::interaction::handle_press(page, &req.params).await,
        "hover" => handlers::interaction::handle_hover(page, state, &req.params).await,
        "scroll" => handlers::interaction::handle_scroll(page, &req.params).await,
        "snapshot" => handlers::refs::handle_snapshot(page, state, &req.params).await,
        "get_ref" => handlers::refs::handle_get_ref(state, &req.params),
        "click_ref" => handlers::refs::handle_click_ref(page, state, &req.params).await,
        "hover_ref" => handlers::refs::handle_hover_ref(page, state, &req.params).await,
        "fill_ref" => handlers::refs::handle_fill_ref(page, state, &req.params).await,
        "wait_for" => handlers::wait::handle_wait_for(page, logs, state, &req.params).await,
        "extract" => handlers::extract::handle_extract(page, &req.params).await,
        "assert" => handlers::assert_cmd::handle_assert(page, logs, state, &req.params).await,
        "screenshot" => handlers::screenshot::handle_screenshot(page, &req.params).await,
        "console" => handlers::inspect::handle_console(logs, &req.params),
        "network" => handlers::inspect::handle_network(logs, &req.params),
        "dialog" => handlers::inspect::handle_dialog(logs, &req.params),
        _ => Err(format!("unsupported cloud tool method: {}", req.method)),
    }
}

pub async fn handle_cloud_user_input(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let input: CloudUserInput =
        serde_json::from_value(params.clone()).map_err(|err| err.to_string())?;
    match input.kind.as_str() {
        "click" => {
            let x = input.x.ok_or("click requires x")?;
            let y = input.y.ok_or("click requires y")?;
            handlers::interaction::handle_click(page, state, &json!({"x": x, "y": y})).await
        }
        "press" => {
            let key = input.key.ok_or("press requires key")?;
            handlers::interaction::handle_press(page, &json!({"key": key})).await
        }
        "type" => {
            let text = input.text.ok_or("type requires text")?;
            let script = format!(
                r#"(() => {{
                    const text = {text_json};
                    const target = document.activeElement;
                    if (!target) return {{ typed: 0, active: false }};
                    if ("value" in target) {{
                        target.value = `${{target.value ?? ""}}${{text}}`;
                        target.dispatchEvent(new InputEvent("input", {{ bubbles: true, data: text, inputType: "insertText" }}));
                        target.dispatchEvent(new Event("change", {{ bubbles: true }}));
                    }} else {{
                        target.textContent = `${{target.textContent ?? ""}}${{text}}`;
                        target.dispatchEvent(new InputEvent("input", {{ bubbles: true, data: text, inputType: "insertText" }}));
                    }}
                    return {{ typed: text.length, active: true }};
                }})()"#,
                text_json = serde_json::to_string(&text).map_err(|err| err.to_string())?
            );
            let value = page
                .evaluate_expression(script)
                .await
                .map_err(|err| err.to_string())?
                .into_value()
                .unwrap_or_else(|_| json!({"typed": text.len()}));
            Ok(value)
        }
        "wheel" => {
            let delta_x = input.delta_x.unwrap_or_default();
            let delta_y = input.delta_y.unwrap_or_default();
            handlers::interaction::handle_scroll(
                page,
                &json!({
                    "direction": if delta_y < 0.0 { "up" } else { "down" },
                    "amount": delta_y.abs().max(delta_x.abs()) as i32,
                }),
            )
            .await
        }
        _ => Err(format!("unsupported user input kind: {}", input.kind)),
    }
}

fn read_identity(path: std::path::PathBuf) -> Option<BrowserIdentity> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn handle_cloud_identity_list(params: &Value) -> Result<Value, String> {
    let scope = params
        .get("scope")
        .and_then(Value::as_str)
        .map(IdentityScope::parse)
        .transpose()?;
    let project_id = params.get("projectId").and_then(Value::as_str);
    let mut identities = Vec::new();
    let scopes: Vec<IdentityScope> = match scope {
        Some(scope) => vec![scope],
        None => vec![
            IdentityScope::Session,
            IdentityScope::Project,
            IdentityScope::Global,
        ],
    };
    for scope in scopes {
        let root = match scope {
            IdentityScope::Project => {
                let Some(project_id) = project_id else {
                    continue;
                };
                gsd_browser_common::state_dir()
                    .join("identities")
                    .join(scope.as_dir())
                    .join(gsd_browser_common::sanitize_filename(project_id)?)
            }
            _ => gsd_browser_common::state_dir()
                .join("identities")
                .join(scope.as_dir()),
        };
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path().join("identity.json");
            if let Some(identity) = read_identity(path) {
                identities.push(identity);
            }
        }
    }
    Ok(json!({ "identities": identities }))
}

pub fn handle_cloud_identity_save(params: &Value) -> Result<Value, String> {
    let identity: BrowserIdentity =
        serde_json::from_value(params.clone()).map_err(|err| err.to_string())?;
    let profile_dir = identity_profile_dir(
        identity.scope,
        identity.project_id.as_deref(),
        &identity.key,
    )?;
    fs::create_dir_all(&profile_dir)
        .map_err(|err| format!("failed to create identity profile dir: {err}"))?;
    let metadata_path = identity_metadata_path(
        identity.scope,
        identity.project_id.as_deref(),
        &identity.key,
    )?;
    let data = serde_json::to_string_pretty(&identity).map_err(|err| err.to_string())?;
    fs::write(&metadata_path, data)
        .map_err(|err| format!("failed to write identity metadata: {err}"))?;
    serde_json::to_value(identity).map_err(|err| err.to_string())
}

pub fn handle_cloud_identity_revoke(params: &Value) -> Result<Value, String> {
    let scope = params
        .get("scope")
        .and_then(Value::as_str)
        .ok_or_else(|| "revoke requires scope".to_string())
        .and_then(IdentityScope::parse)?;
    let project_id = params.get("projectId").and_then(Value::as_str);
    let key = params
        .get("key")
        .and_then(Value::as_str)
        .ok_or("revoke requires key")?;
    let profile_dir = identity_profile_dir(scope, project_id, key)?;
    let identity_dir = profile_dir
        .parent()
        .ok_or("identity profile path has no parent")?;
    if identity_dir.exists() {
        fs::remove_dir_all(identity_dir)
            .map_err(|err| format!("failed to remove identity: {err}"))?;
    }
    Ok(json!({ "revoked": true }))
}
