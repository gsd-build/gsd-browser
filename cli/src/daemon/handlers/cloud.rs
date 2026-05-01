use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams,
    DispatchMouseEventPointerType, DispatchMouseEventType, InsertTextParams, MouseButton,
};
use chromiumoxide::keys;
use chromiumoxide::Page;
use gsd_browser_common::cloud::{CloudFrame, CloudSessionStatus, CloudToolRequest, CloudUserInput};
use gsd_browser_common::identity::{
    identity_metadata_path, identity_profile_dir, BrowserIdentity, IdentityScope,
};
use image::{GenericImageView, ImageReader};
use serde_json::{json, Value};
use std::fs;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;

use crate::daemon::{handlers, logs::DaemonLogs, state::DaemonState};

static FRAME_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const INPUT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
struct ViewportMetrics {
    width: u32,
    height: u32,
    device_pixel_ratio: f64,
}

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

fn image_dimensions_from_base64(data: &str) -> Option<(u32, u32)> {
    let bytes = BASE64.decode(data).ok()?;
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(image.dimensions())
}

async fn viewport_center(page: &Page) -> (f64, f64) {
    let value = page
        .evaluate_expression(
            r#"(() => ({
                x: Math.max(0, Math.round(window.innerWidth / 2)),
                y: Math.max(0, Math.round(window.innerHeight / 2))
            }))()"#,
        )
        .await
        .ok()
        .and_then(|result| result.into_value().ok())
        .unwrap_or_else(|| json!({"x": 0, "y": 0}));
    (
        value.get("x").and_then(Value::as_f64).unwrap_or_default(),
        value.get("y").and_then(Value::as_f64).unwrap_or_default(),
    )
}

async fn viewport_metrics(
    page: &Page,
    fallback_width: u32,
    fallback_height: u32,
) -> ViewportMetrics {
    let value = page
        .evaluate_expression(
            r#"(() => ({
                width: Math.max(0, Math.round(window.innerWidth || 0)),
                height: Math.max(0, Math.round(window.innerHeight || 0)),
                devicePixelRatio: Number(window.devicePixelRatio || 1)
            }))()"#,
        )
        .await
        .ok()
        .and_then(|result| result.into_value().ok())
        .unwrap_or_else(|| json!({}));

    let width = value
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    let height = value
        .get("height")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;

    ViewportMetrics {
        width: if width == 0 { fallback_width } else { width },
        height: if height == 0 { fallback_height } else { height },
        device_pixel_ratio: value
            .get("devicePixelRatio")
            .and_then(Value::as_f64)
            .unwrap_or(1.0),
    }
}

async fn scroll_info(page: &Page) -> Value {
    page.evaluate_expression(
        r#"(() => ({
            x: Math.round(window.scrollX),
            y: Math.round(window.scrollY),
            height: document.documentElement.scrollHeight,
            viewportHeight: window.innerHeight
        }))()"#,
    )
    .await
    .ok()
    .and_then(|result| result.into_value().ok())
    .unwrap_or_else(|| json!({}))
}

fn modifier_mask(modifiers: Option<&[String]>) -> Result<i64, String> {
    let Some(modifiers) = modifiers else {
        return Ok(0);
    };

    let mut mask = 0;
    for modifier in modifiers {
        match modifier.to_ascii_lowercase().as_str() {
            "alt" | "option" => mask |= 1,
            "control" | "ctrl" => mask |= 2,
            "meta" | "command" | "cmd" => mask |= 4,
            "shift" => mask |= 8,
            other => return Err(format!("unsupported modifier: {other}")),
        }
    }
    Ok(mask)
}

fn mouse_button(button: Option<&str>) -> Result<MouseButton, String> {
    match button.unwrap_or("left").to_ascii_lowercase().as_str() {
        "none" => Ok(MouseButton::None),
        "left" => Ok(MouseButton::Left),
        "middle" => Ok(MouseButton::Middle),
        "right" => Ok(MouseButton::Right),
        "back" => Ok(MouseButton::Back),
        "forward" => Ok(MouseButton::Forward),
        other => Err(format!("unsupported mouse button: {other}")),
    }
}

fn mouse_buttons_mask(button: &MouseButton) -> i64 {
    match button {
        MouseButton::None => 0,
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
        MouseButton::Back => 8,
        MouseButton::Forward => 16,
    }
}

fn validate_coordinate_space(coordinate_space: Option<&str>) -> Result<(), String> {
    match coordinate_space {
        None | Some("") => Ok(()),
        Some("viewport") | Some("viewport_css_pixels") | Some("viewport-css-px") => Ok(()),
        Some("frame_css_pixels") => Ok(()),
        Some(other) => Err(format!("unsupported coordinate space: {other}")),
    }
}

async fn dispatch_mouse(
    page: &Page,
    event_type: DispatchMouseEventType,
    x: f64,
    y: f64,
    button: MouseButton,
    buttons: i64,
    click_count: i64,
    modifiers: i64,
    delta_x: Option<f64>,
    delta_y: Option<f64>,
) -> Result<(), String> {
    let mut params = DispatchMouseEventParams::builder()
        .r#type(event_type)
        .x(x)
        .y(y)
        .button(button)
        .buttons(buttons)
        .click_count(click_count)
        .modifiers(modifiers)
        .pointer_type(DispatchMouseEventPointerType::Mouse);

    if let Some(delta_x) = delta_x {
        params = params.delta_x(delta_x);
    }
    if let Some(delta_y) = delta_y {
        params = params.delta_y(delta_y);
    }

    let params = params.build().map_err(|err| err.to_string())?;
    timeout(INPUT_TIMEOUT, page.execute(params))
        .await
        .map_err(|_| "mouse input timed out".to_string())?
        .map_err(|err| format!("mouse input failed: {err}"))?;
    Ok(())
}

fn key_event_params(
    key: &str,
    event_type: DispatchKeyEventType,
    modifiers: i64,
) -> Result<DispatchKeyEventParams, String> {
    let mut command = DispatchKeyEventParams::builder()
        .r#type(event_type.clone())
        .modifiers(modifiers);

    if let Some(key_definition) = keys::get_key_definition(key) {
        command = command
            .key(key_definition.key)
            .code(key_definition.code)
            .windows_virtual_key_code(key_definition.key_code)
            .native_virtual_key_code(key_definition.key_code);

        if matches!(event_type, DispatchKeyEventType::KeyDown) {
            if let Some(text) = key_definition.text {
                command = command.text(text);
            } else if key_definition.key.len() == 1 {
                command = command.text(key_definition.key);
            }
        }
    } else {
        command = command.key(key).code(key);
        if matches!(event_type, DispatchKeyEventType::KeyDown) && key.chars().count() == 1 {
            command = command.text(key);
        }
    }

    command.build().map_err(|err| err.to_string())
}

async fn dispatch_key(
    page: &Page,
    key: &str,
    event_type: DispatchKeyEventType,
    modifiers: i64,
) -> Result<(), String> {
    let event = key_event_params(key, event_type.clone(), modifiers)?;
    timeout(INPUT_TIMEOUT, page.execute(event))
        .await
        .map_err(|_| "key input timed out".to_string())?
        .map_err(|err| format!("key input failed: {err}"))?;
    Ok(())
}

async fn dispatch_text(page: &Page, text: &str) -> Result<(), String> {
    for character in text.chars() {
        let key = character.to_string();
        let Some(key_definition) = keys::get_key_definition(&key) else {
            page.execute(InsertTextParams::new(key))
                .await
                .map_err(|err| format!("insert text failed: {err}"))?;
            continue;
        };

        let mut command = DispatchKeyEventParams::builder()
            .key(key_definition.key)
            .code(key_definition.code)
            .windows_virtual_key_code(key_definition.key_code)
            .native_virtual_key_code(key_definition.key_code);

        let key_down_event_type = if let Some(text) = key_definition.text {
            command = command.text(text);
            DispatchKeyEventType::KeyDown
        } else if key_definition.key.len() == 1 {
            command = command.text(key_definition.key);
            DispatchKeyEventType::KeyDown
        } else {
            DispatchKeyEventType::RawKeyDown
        };

        let key_down = command
            .clone()
            .r#type(key_down_event_type)
            .build()
            .map_err(|err| err.to_string())?;
        page.execute(key_down)
            .await
            .map_err(|err| format!("key down failed: {err}"))?;

        let key_up = command
            .r#type(DispatchKeyEventType::KeyUp)
            .build()
            .map_err(|err| err.to_string())?;
        page.execute(key_up)
            .await
            .map_err(|err| format!("key up failed: {err}"))?;
    }
    Ok(())
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
    let mut width = shot
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    let mut height = shot
        .get("height")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    if (width == 0 || height == 0) && !data.is_empty() {
        if let Some((decoded_width, decoded_height)) = image_dimensions_from_base64(&data) {
            width = decoded_width;
            height = decoded_height;
        }
    }
    let viewport = viewport_metrics(page, width, height).await;
    let frame = CloudFrame {
        sequence: FRAME_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        content_type: "image/jpeg".to_string(),
        data_base64: data,
        width,
        height,
        viewport_width: viewport.width,
        viewport_height: viewport.height,
        device_pixel_ratio: viewport.device_pixel_ratio,
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
        "press" => handlers::interaction::handle_press(page, state, &req.params).await,
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
    _state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let input: CloudUserInput =
        serde_json::from_value(params.clone()).map_err(|err| err.to_string())?;
    validate_coordinate_space(input.coordinate_space.as_deref())?;
    let modifiers = modifier_mask(input.modifiers.as_deref())?;

    match input.kind.as_str() {
        "click" => {
            let x = input.x.ok_or("click requires x")?;
            let y = input.y.ok_or("click requires y")?;
            let button = mouse_button(input.button.as_deref())?;
            let buttons = mouse_buttons_mask(&button);
            dispatch_mouse(
                page,
                DispatchMouseEventType::MouseMoved,
                x,
                y,
                MouseButton::None,
                0,
                0,
                modifiers,
                None,
                None,
            )
            .await?;
            dispatch_mouse(
                page,
                DispatchMouseEventType::MousePressed,
                x,
                y,
                button.clone(),
                buttons,
                1,
                modifiers,
                None,
                None,
            )
            .await?;
            dispatch_mouse(
                page,
                DispatchMouseEventType::MouseReleased,
                x,
                y,
                button,
                0,
                1,
                modifiers,
                None,
                None,
            )
            .await?;
            Ok(json!({ "clicked": { "x": x, "y": y } }))
        }
        "pointer_move" => {
            let x = input.x.ok_or("pointer_move requires x")?;
            let y = input.y.ok_or("pointer_move requires y")?;
            dispatch_mouse(
                page,
                DispatchMouseEventType::MouseMoved,
                x,
                y,
                MouseButton::None,
                0,
                0,
                modifiers,
                None,
                None,
            )
            .await?;
            Ok(json!({ "pointer": { "kind": "pointer_move", "x": x, "y": y } }))
        }
        "pointer_down" => {
            let x = input.x.ok_or("pointer_down requires x")?;
            let y = input.y.ok_or("pointer_down requires y")?;
            let button = mouse_button(input.button.as_deref())?;
            let buttons = mouse_buttons_mask(&button);
            dispatch_mouse(
                page,
                DispatchMouseEventType::MousePressed,
                x,
                y,
                button,
                buttons,
                1,
                modifiers,
                None,
                None,
            )
            .await?;
            Ok(json!({ "pointer": { "kind": "pointer_down", "x": x, "y": y } }))
        }
        "pointer_up" => {
            let x = input.x.ok_or("pointer_up requires x")?;
            let y = input.y.ok_or("pointer_up requires y")?;
            let button = mouse_button(input.button.as_deref())?;
            dispatch_mouse(
                page,
                DispatchMouseEventType::MouseReleased,
                x,
                y,
                button,
                0,
                1,
                modifiers,
                None,
                None,
            )
            .await?;
            Ok(json!({ "pointer": { "kind": "pointer_up", "x": x, "y": y } }))
        }
        "key_down" => {
            let key = input.key.ok_or("key_down requires key")?;
            dispatch_key(page, &key, DispatchKeyEventType::KeyDown, modifiers).await?;
            Ok(json!({ "key": { "kind": "key_down", "key": key } }))
        }
        "key_up" => {
            let key = input.key.ok_or("key_up requires key")?;
            dispatch_key(page, &key, DispatchKeyEventType::KeyUp, modifiers).await?;
            Ok(json!({ "key": { "kind": "key_up", "key": key } }))
        }
        "press" => {
            let key = input.key.ok_or("press requires key")?;
            dispatch_key(page, &key, DispatchKeyEventType::KeyDown, modifiers).await?;
            dispatch_key(page, &key, DispatchKeyEventType::KeyUp, modifiers).await?;
            Ok(json!({ "pressed": key }))
        }
        "text" | "type" => {
            let text = input
                .text
                .ok_or_else(|| format!("{} requires text", input.kind))?;
            timeout(INPUT_TIMEOUT, dispatch_text(page, &text))
                .await
                .map_err(|_| "text timed out".to_string())?
                .map_err(|err| format!("text failed: {err}"))?;
            Ok(json!({ "typed": text.len() }))
        }
        "wheel" => {
            let delta_x = input.delta_x.unwrap_or_default();
            let delta_y = input.delta_y.unwrap_or_default();
            let (default_x, default_y) = viewport_center(page).await;
            let x = input.x.unwrap_or(default_x);
            let y = input.y.unwrap_or(default_y);
            let params = DispatchMouseEventParams::builder()
                .r#type(DispatchMouseEventType::MouseWheel)
                .x(x)
                .y(y)
                .delta_x(delta_x)
                .delta_y(delta_y)
                .button(MouseButton::None)
                .buttons(0)
                .modifiers(modifiers)
                .pointer_type(DispatchMouseEventPointerType::Mouse)
                .build()
                .map_err(|err| err.to_string())?;
            timeout(INPUT_TIMEOUT, page.execute(params))
                .await
                .map_err(|_| "wheel timed out".to_string())?
                .map_err(|err| format!("wheel failed: {err}"))?;
            Ok(json!({
                "wheel": {
                    "x": x,
                    "y": y,
                    "deltaX": delta_x,
                    "deltaY": delta_y,
                },
                "scroll": scroll_info(page).await,
            }))
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
    if matches!(scope, Some(IdentityScope::Project)) && project_id.is_none() {
        return Err("project identity requires projectId".to_string());
    }
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
        let roots = match scope {
            IdentityScope::Project => {
                let project_root = gsd_browser_common::state_dir()
                    .join("identities")
                    .join(scope.as_dir());
                if let Some(project_id) = project_id {
                    vec![project_root.join(gsd_browser_common::sanitize_filename(project_id)?)]
                } else {
                    match fs::read_dir(project_root) {
                        Ok(entries) => entries.flatten().map(|entry| entry.path()).collect(),
                        Err(_) => Vec::new(),
                    }
                }
            }
            _ => vec![gsd_browser_common::state_dir()
                .join("identities")
                .join(scope.as_dir())],
        };
        for root in roots {
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
