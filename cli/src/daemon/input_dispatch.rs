use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams,
    DispatchMouseEventPointerType, DispatchMouseEventType, InsertTextParams, MouseButton,
};
use chromiumoxide::keys;
use chromiumoxide::Page;
use gsd_browser_common::viewer::{UserInputEventV1, UserInputKind};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;

use crate::daemon::{handlers, state::DaemonState};

const INPUT_TIMEOUT: Duration = Duration::from_secs(5);

pub fn modifier_mask(modifiers: Option<&[String]>) -> Result<i64, String> {
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

pub fn mouse_buttons_mask(button: &str) -> Result<i64, String> {
    Ok(mouse_buttons_mask_for_button(&mouse_button(Some(button))?))
}

fn mouse_buttons_mask_for_button(button: &MouseButton) -> i64 {
    match button {
        MouseButton::None => 0,
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
        MouseButton::Back => 8,
        MouseButton::Forward => 16,
    }
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

async fn dispatch_pointer(
    page: &Page,
    input: &UserInputEventV1,
    modifiers: i64,
) -> Result<Value, String> {
    let x = input.x.ok_or("pointer requires x")?;
    let y = input.y.ok_or("pointer requires y")?;
    let phase = input.phase.as_deref().ok_or("pointer requires phase")?;
    let button = if phase == "context_click" {
        MouseButton::Right
    } else {
        mouse_button(input.button.as_deref())?
    };
    let buttons = input
        .buttons
        .unwrap_or_else(|| mouse_buttons_mask_for_button(&button));
    let click_count = match phase {
        "double_click" => 2,
        _ => input.click_count.unwrap_or(1),
    };
    match phase {
        "move" => {
            dispatch_mouse(
                page,
                DispatchMouseEventType::MouseMoved,
                x,
                y,
                MouseButton::None,
                buttons,
                0,
                modifiers,
                None,
                None,
            )
            .await?;
        }
        "down" => {
            dispatch_mouse(
                page,
                DispatchMouseEventType::MousePressed,
                x,
                y,
                button.clone(),
                buttons,
                click_count,
                modifiers,
                None,
                None,
            )
            .await?;
        }
        "up" => {
            dispatch_mouse(
                page,
                DispatchMouseEventType::MouseReleased,
                x,
                y,
                button.clone(),
                0,
                click_count,
                modifiers,
                None,
                None,
            )
            .await?;
        }
        "click" | "double_click" | "context_click" => {
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
                click_count,
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
                click_count,
                modifiers,
                None,
                None,
            )
            .await?;
        }
        other => return Err(format!("unsupported pointer phase: {other}")),
    }
    Ok(json!({ "pointer": { "phase": phase, "x": x, "y": y } }))
}

async fn dispatch_wheel(
    page: &Page,
    input: &UserInputEventV1,
    modifiers: i64,
) -> Result<Value, String> {
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

async fn dispatch_key_input(
    page: &Page,
    input: &UserInputEventV1,
    modifiers: i64,
) -> Result<Value, String> {
    let key = input.key.as_deref().ok_or("key requires key")?;
    match input.phase.as_deref().unwrap_or("press") {
        "down" => dispatch_key(page, key, DispatchKeyEventType::KeyDown, modifiers).await?,
        "up" => dispatch_key(page, key, DispatchKeyEventType::KeyUp, modifiers).await?,
        "press" => {
            dispatch_key(page, key, DispatchKeyEventType::KeyDown, modifiers).await?;
            dispatch_key(page, key, DispatchKeyEventType::KeyUp, modifiers).await?;
        }
        other => return Err(format!("unsupported key phase: {other}")),
    }
    Ok(json!({ "key": { "phase": input.phase.as_deref().unwrap_or("press"), "key": key } }))
}

async fn dispatch_text_input(page: &Page, input: &UserInputEventV1) -> Result<Value, String> {
    let text = input
        .text
        .as_deref()
        .ok_or_else(|| format!("{:?} requires text", input.kind))?;
    timeout(INPUT_TIMEOUT, dispatch_text(page, text))
        .await
        .map_err(|_| "text timed out".to_string())?
        .map_err(|err| format!("text failed: {err}"))?;
    Ok(json!({ "typed": text.len() }))
}

async fn dispatch_navigation(
    page: &Page,
    state: &DaemonState,
    input: &UserInputEventV1,
) -> Result<Value, String> {
    let url = input.url.as_deref().ok_or("navigation requires url")?;
    handlers::navigate::handle_navigate(page, &json!({ "url": url }), state).await
}

pub async fn dispatch_user_input(
    page: &Page,
    state: &DaemonState,
    input: &UserInputEventV1,
) -> Result<Value, String> {
    input.validate()?;
    let modifiers = modifier_mask(input.modifiers.as_deref())?;
    match input.kind {
        UserInputKind::Pointer => dispatch_pointer(page, input, modifiers).await,
        UserInputKind::Wheel => dispatch_wheel(page, input, modifiers).await,
        UserInputKind::Key => dispatch_key_input(page, input, modifiers).await,
        UserInputKind::Text | UserInputKind::Paste | UserInputKind::Composition => {
            dispatch_text_input(page, input).await
        }
        UserInputKind::Navigation => dispatch_navigation(page, state, input).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_mask_accepts_browser_names() {
        assert_eq!(
            modifier_mask(Some(&["shift".into(), "meta".into()])).unwrap(),
            12
        );
        assert_eq!(
            modifier_mask(Some(&["ctrl".into(), "alt".into()])).unwrap(),
            3
        );
    }

    #[test]
    fn modifier_mask_rejects_unknown() {
        let err = modifier_mask(Some(&["hyper".into()])).expect_err("unknown modifier");
        assert!(err.contains("unsupported modifier"));
    }

    #[test]
    fn mouse_button_mask_matches_cdp_bits() {
        assert_eq!(mouse_buttons_mask("left").unwrap(), 1);
        assert_eq!(mouse_buttons_mask("right").unwrap(), 2);
        assert_eq!(mouse_buttons_mask("middle").unwrap(), 4);
    }
}
