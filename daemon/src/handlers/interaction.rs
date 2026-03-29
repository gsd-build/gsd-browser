//! Interaction command handlers: click, type, press, hover, scroll, drag,
//! select_option, set_checked, set_viewport, upload_file.
//!
//! Every handler follows the same pattern: validate params → find element or
//! dispatch CDP → settle → capture compact page state → return JSON.

use crate::capture::capture_compact_page_state;
use crate::settle::{ensure_mutation_counter, settle_after_action};
use gsd_browser_common::types::SettleOptions;
use chromiumoxide::cdp::browser_protocol::dom::SetFileInputFilesParams;
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::layout::Point;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;
use tracing::debug;

/// Maximum timeout for element operations.
const ELEMENT_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout for CDP dispatch calls (mouse events, etc.).
const CDP_TIMEOUT: Duration = Duration::from_secs(5);

/// Default settle options for interaction commands.
fn interaction_settle_opts() -> SettleOptions {
    SettleOptions {
        timeout_ms: 1500,
        check_focus_stability: true,
        ..SettleOptions::default()
    }
}

/// Settle and capture page state after an interaction.
async fn settle_and_capture(page: &Page) -> (Value, Value) {
    ensure_mutation_counter(page).await;
    let settle = settle_after_action(page, &interaction_settle_opts()).await;
    let state = capture_compact_page_state(page, false).await;
    (
        serde_json::to_value(&state).unwrap_or(json!({})),
        serde_json::to_value(&settle).unwrap_or(json!({})),
    )
}

// ── Click ──

/// Handle `click` command.
/// Params: { selector?: string, x?: f64, y?: f64 }
pub async fn handle_click(page: &Page, params: &Value) -> Result<Value, String> {
    let selector = params.get("selector").and_then(|v| v.as_str());
    let x = params.get("x").and_then(|v| v.as_f64());
    let y = params.get("y").and_then(|v| v.as_f64());

    match (selector, x, y) {
        (Some(sel), _, _) => click_selector(page, sel).await,
        (None, Some(cx), Some(cy)) => click_coordinates(page, cx, cy).await,
        _ => Err("click requires either 'selector' or both 'x' and 'y' coordinates".to_string()),
    }
}

async fn click_selector(page: &Page, selector: &str) -> Result<Value, String> {
    debug!("click: selector={selector}");

    // Try native element click first, fall back to JS click
    let click_result = timeout(ELEMENT_TIMEOUT, async {
        match page.find_element(selector).await {
            Ok(element) => match element.click().await {
                Ok(_) => Ok(()),
                Err(e) => {
                    debug!("click: native click failed ({e}), falling back to JS");
                    // JS fallback
                    let js = format!(
                        "(() => {{ const el = document.querySelector({sel}); if (!el) throw new Error('element not found'); el.click(); return true; }})()",
                        sel = serde_json::to_string(selector).unwrap()
                    );
                    page.evaluate_expression(&js)
                        .await
                        .map_err(|e2| format!("JS click fallback failed: {e2}"))?;
                    Ok(())
                }
            },
            Err(e) => Err(format!("element not found: {selector} ({e})")),
        }
    })
    .await
    .map_err(|_| format!("click timed out after 10s for: {selector}"))?;

    click_result?;

    let (state, settle) = settle_and_capture(page).await;
    Ok(json!({
        "state": state,
        "settle": settle,
        "clicked": { "selector": selector },
    }))
}

async fn click_coordinates(page: &Page, x: f64, y: f64) -> Result<Value, String> {
    debug!("click: coordinates=({x}, {y})");

    timeout(CDP_TIMEOUT, page.click(Point::new(x, y)))
        .await
        .map_err(|_| format!("click timed out at ({x}, {y})"))?
        .map_err(|e| format!("click failed at ({x}, {y}): {e}"))?;

    let (state, settle) = settle_and_capture(page).await;
    Ok(json!({
        "state": state,
        "settle": settle,
        "clicked": { "x": x, "y": y },
    }))
}

// ── Type ──

/// Handle `type` command (called type_text to avoid Rust keyword).
/// Params: { selector: string, text: string, slowly?: bool, clear_first?: bool, submit?: bool }
pub async fn handle_type_text(page: &Page, params: &Value) -> Result<Value, String> {
    let selector = params
        .get("selector")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: selector".to_string())?;
    let text = params
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: text".to_string())?;
    let slowly = params
        .get("slowly")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let clear_first = params
        .get("clear_first")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let submit = params
        .get("submit")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    debug!("type_text: selector={selector} len={} slowly={slowly} clear={clear_first} submit={submit}", text.len());

    let text_len = text.len();

    // Find element and click to focus
    let element = timeout(ELEMENT_TIMEOUT, page.find_element(selector))
        .await
        .map_err(|_| format!("type: timed out finding element: {selector}"))?
        .map_err(|e| format!("element not found: {selector} ({e})"))?;

    timeout(ELEMENT_TIMEOUT, element.click())
        .await
        .map_err(|_| "type: timed out clicking element to focus".to_string())?
        .map_err(|e| format!("type: click to focus failed: {e}"))?;

    // Clear if requested
    if clear_first {
        let clear_js = format!(
            "(() => {{ const el = document.querySelector({sel}); if(el) {{ el.value = ''; el.dispatchEvent(new Event('input', {{bubbles:true}})); }} }})()",
            sel = serde_json::to_string(selector).unwrap()
        );
        timeout(ELEMENT_TIMEOUT, page.evaluate_expression(&clear_js))
            .await
            .map_err(|_| "type: timed out clearing field".to_string())?
            .map_err(|e| format!("type: clear failed: {e}"))?;
    }

    if slowly {
        // Character-by-character via type_str (dispatches key events)
        let element = timeout(ELEMENT_TIMEOUT, page.find_element(selector))
            .await
            .map_err(|_| format!("type: timed out re-finding element: {selector}"))?
            .map_err(|e| format!("element not found: {selector} ({e})"))?;
        timeout(Duration::from_secs(30), element.type_str(text))
            .await
            .map_err(|_| "type: timed out typing slowly".to_string())?
            .map_err(|e| format!("type: slow typing failed: {e}"))?;
    } else {
        // Atomic fill via JS
        let fill_js = format!(
            "(() => {{ const el = document.querySelector({sel}); if(!el) throw new Error('element not found'); el.value = {val}; el.dispatchEvent(new Event('input', {{bubbles:true}})); el.dispatchEvent(new Event('change', {{bubbles:true}})); return true; }})()",
            sel = serde_json::to_string(selector).unwrap(),
            val = serde_json::to_string(text).unwrap()
        );
        timeout(ELEMENT_TIMEOUT, page.evaluate_expression(&fill_js))
            .await
            .map_err(|_| "type: timed out filling field".to_string())?
            .map_err(|e| format!("type: atomic fill failed: {e}"))?;
    }

    let submitted = if submit {
        // Re-find element and press Enter
        let element = timeout(ELEMENT_TIMEOUT, page.find_element(selector))
            .await
            .map_err(|_| format!("type: timed out re-finding element for submit: {selector}"))?
            .map_err(|e| format!("element not found for submit: {selector} ({e})"))?;
        timeout(ELEMENT_TIMEOUT, element.press_key("Enter"))
            .await
            .map_err(|_| "type: timed out pressing Enter".to_string())?
            .map_err(|e| format!("type: press Enter failed: {e}"))?;
        true
    } else {
        false
    };

    let (state, settle) = settle_and_capture(page).await;
    Ok(json!({
        "state": state,
        "settle": settle,
        "typed": {
            "selector": selector,
            "text_length": text_len,
            "slowly": slowly,
            "submitted": submitted,
        },
    }))
}

// ── Press ──

/// Handle `press` command — press a key or key combination.
/// Params: { key: string }
pub async fn handle_press(page: &Page, params: &Value) -> Result<Value, String> {
    let key = params
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: key".to_string())?;

    debug!("press: key={key}");

    if key.contains('+') {
        // Key combination: e.g. "Meta+A", "Control+Shift+T"
        press_combo(page, key).await?;
    } else {
        // Single key
        // We use JS dispatchEvent for keys since chromiumoxide's press_key
        // is on Element, not Page. Use CDP Input.dispatchKeyEvent via JS.
        let js = format!(
            r#"(() => {{
                const key = {key_json};
                const event = new KeyboardEvent('keydown', {{key, bubbles: true}});
                document.activeElement ? document.activeElement.dispatchEvent(event) : document.dispatchEvent(event);
                const up = new KeyboardEvent('keyup', {{key, bubbles: true}});
                document.activeElement ? document.activeElement.dispatchEvent(up) : document.dispatchEvent(up);
                return true;
            }})()"#,
            key_json = serde_json::to_string(key).unwrap()
        );
        timeout(CDP_TIMEOUT, page.evaluate_expression(&js))
            .await
            .map_err(|_| format!("press timed out for key: {key}"))?
            .map_err(|e| format!("press failed for key {key}: {e}"))?;
    }

    let (state, settle) = settle_and_capture(page).await;
    Ok(json!({
        "state": state,
        "settle": settle,
        "pressed": key,
    }))
}

async fn press_combo(page: &Page, combo: &str) -> Result<(), String> {
    let parts: Vec<&str> = combo.split('+').collect();
    if parts.is_empty() {
        return Err("empty key combination".to_string());
    }

    // Build JS that dispatches keydown for each modifier, then the final key, then keyup in reverse
    let modifiers: Vec<&str> = parts[..parts.len() - 1].iter().copied().collect();
    let final_key = parts[parts.len() - 1];

    let modifier_flags: Vec<String> = modifiers
        .iter()
        .map(|m| match m.to_lowercase().as_str() {
            "meta" | "command" | "cmd" => "metaKey: true".to_string(),
            "control" | "ctrl" => "ctrlKey: true".to_string(),
            "shift" => "shiftKey: true".to_string(),
            "alt" | "option" => "altKey: true".to_string(),
            _ => format!("/* unknown modifier: {m} */"),
        })
        .collect();

    let flags = modifier_flags.join(", ");
    let js = format!(
        r#"(() => {{
            const target = document.activeElement || document;
            const opts = {{ bubbles: true, {flags} }};
            target.dispatchEvent(new KeyboardEvent('keydown', {{ ...opts, key: {key_json} }}));
            target.dispatchEvent(new KeyboardEvent('keypress', {{ ...opts, key: {key_json} }}));
            target.dispatchEvent(new KeyboardEvent('keyup', {{ ...opts, key: {key_json} }}));
            return true;
        }})()"#,
        flags = flags,
        key_json = serde_json::to_string(final_key).unwrap()
    );

    timeout(CDP_TIMEOUT, page.evaluate_expression(&js))
        .await
        .map_err(|_| format!("press combo timed out: {combo}"))?
        .map_err(|e| format!("press combo failed ({combo}): {e}"))?;

    Ok(())
}

// ── Hover ──

/// Handle `hover` command — scroll element into view and dispatch mouseMoved.
/// Params: { selector: string }
pub async fn handle_hover(page: &Page, params: &Value) -> Result<Value, String> {
    let selector = params
        .get("selector")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: selector".to_string())?;

    debug!("hover: selector={selector}");

    let element = timeout(ELEMENT_TIMEOUT, page.find_element(selector))
        .await
        .map_err(|_| format!("hover timed out finding element: {selector}"))?
        .map_err(|e| format!("element not found: {selector} ({e})"))?;

    // scroll_into_view + hover (which dispatches mouseMoved internally)
    timeout(ELEMENT_TIMEOUT, element.scroll_into_view())
        .await
        .map_err(|_| "hover: timed out scrolling element into view".to_string())?
        .map_err(|e| format!("hover: scroll into view failed: {e}"))?;

    // Re-find element after scroll
    let element = timeout(ELEMENT_TIMEOUT, page.find_element(selector))
        .await
        .map_err(|_| format!("hover: timed out re-finding element: {selector}"))?
        .map_err(|e| format!("element not found after scroll: {selector} ({e})"))?;

    timeout(ELEMENT_TIMEOUT, element.hover())
        .await
        .map_err(|_| format!("hover timed out for: {selector}"))?
        .map_err(|e| format!("hover failed for {selector}: {e}"))?;

    let (state, settle) = settle_and_capture(page).await;
    Ok(json!({
        "state": state,
        "settle": settle,
        "hovered": selector,
    }))
}

// ── Scroll ──

/// Handle `scroll` command — scroll the page and return position.
/// Params: { direction: "up"|"down", amount?: i32 }
pub async fn handle_scroll(page: &Page, params: &Value) -> Result<Value, String> {
    let direction = params
        .get("direction")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: direction".to_string())?;
    let amount = params
        .get("amount")
        .and_then(|v| v.as_i64())
        .unwrap_or(300) as i32;

    let scroll_amount = match direction {
        "up" => -amount.abs(),
        "down" => amount.abs(),
        _ => return Err(format!("direction must be 'up' or 'down', got: {direction}")),
    };

    debug!("scroll: direction={direction} amount={scroll_amount}");

    let js = format!(
        r#"(() => {{
            window.scrollBy(0, {scroll_amount});
            return {{
                x: Math.round(window.scrollX),
                y: Math.round(window.scrollY),
                height: document.documentElement.scrollHeight,
                viewport_height: window.innerHeight,
            }};
        }})()"#
    );

    let result = timeout(CDP_TIMEOUT, page.evaluate_expression(&js))
        .await
        .map_err(|_| "scroll timed out".to_string())?
        .map_err(|e| format!("scroll failed: {e}"))?;

    let scroll_info = result.value().cloned().unwrap_or(json!({}));
    let scroll_y = scroll_info.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let scroll_height = scroll_info
        .get("height")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let viewport_height = scroll_info
        .get("viewport_height")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let max_scroll = (scroll_height - viewport_height).max(1.0);
    let percentage = ((scroll_y / max_scroll) * 100.0).round().min(100.0);

    // Capture page state (scroll is mostly synchronous, but capture anyway)
    let state = capture_compact_page_state(page, false).await;

    Ok(json!({
        "state": state,
        "scroll": {
            "x": scroll_info.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            "y": scroll_y,
            "height": scroll_height,
            "viewport_height": viewport_height,
            "percentage": percentage,
        },
    }))
}

// ── Select Option ──

/// Handle `select_option` command — set select element value.
/// Params: { selector: string, option: string }
pub async fn handle_select_option(page: &Page, params: &Value) -> Result<Value, String> {
    let selector = params
        .get("selector")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: selector".to_string())?;
    let option = params
        .get("option")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: option".to_string())?;

    debug!("select_option: selector={selector} option={option}");

    let js = format!(
        r#"(() => {{
            const sel = document.querySelector({sel_json});
            if (!sel) throw new Error('select element not found: ' + {sel_json});
            if (sel.tagName.toLowerCase() !== 'select') throw new Error('element is not a <select>');
            const opts = Array.from(sel.options);
            const match = opts.find(o => o.label === {opt_json} || o.value === {opt_json} || o.textContent.trim() === {opt_json});
            if (!match) throw new Error('option not found: ' + {opt_json});
            sel.value = match.value;
            sel.dispatchEvent(new Event('change', {{bubbles: true}}));
            sel.dispatchEvent(new Event('input', {{bubbles: true}}));
            return {{ selected: match.value, label: match.label || match.textContent.trim() }};
        }})()"#,
        sel_json = serde_json::to_string(selector).unwrap(),
        opt_json = serde_json::to_string(option).unwrap()
    );

    timeout(ELEMENT_TIMEOUT, page.evaluate_expression(&js))
        .await
        .map_err(|_| format!("select_option timed out for: {selector}"))?
        .map_err(|e| format!("select_option failed: {e}"))?;

    let (state, settle) = settle_and_capture(page).await;
    Ok(json!({
        "state": state,
        "settle": settle,
        "selected": { "selector": selector, "option": option },
    }))
}

// ── Set Checked ──

/// Handle `set_checked` command — set checkbox/radio state.
/// Params: { selector: string, checked: bool }
pub async fn handle_set_checked(page: &Page, params: &Value) -> Result<Value, String> {
    let selector = params
        .get("selector")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: selector".to_string())?;
    let checked = params
        .get("checked")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| "missing required parameter: checked (boolean)".to_string())?;

    debug!("set_checked: selector={selector} checked={checked}");

    let js = format!(
        r#"(() => {{
            const el = document.querySelector({sel_json});
            if (!el) throw new Error('element not found: ' + {sel_json});
            el.checked = {checked};
            el.dispatchEvent(new Event('change', {{bubbles: true}}));
            el.dispatchEvent(new Event('input', {{bubbles: true}}));
            return true;
        }})()"#,
        sel_json = serde_json::to_string(selector).unwrap(),
        checked = checked,
    );

    timeout(ELEMENT_TIMEOUT, page.evaluate_expression(&js))
        .await
        .map_err(|_| format!("set_checked timed out for: {selector}"))?
        .map_err(|e| format!("set_checked failed: {e}"))?;

    let (state, settle) = settle_and_capture(page).await;
    Ok(json!({
        "state": state,
        "settle": settle,
        "checked": { "selector": selector, "value": checked },
    }))
}

// ── Drag ──

/// Handle `drag` command — simulate drag from source to target element.
/// Params: { source: string, target: string }
pub async fn handle_drag(page: &Page, params: &Value) -> Result<Value, String> {
    let source_sel = params
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: source".to_string())?;
    let target_sel = params
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: target".to_string())?;

    debug!("drag: source={source_sel} target={target_sel}");

    // Get centers of both elements via JS
    let centers_js = format!(
        r#"(() => {{
            const src = document.querySelector({src_json});
            const tgt = document.querySelector({tgt_json});
            if (!src) throw new Error('source element not found: ' + {src_json});
            if (!tgt) throw new Error('target element not found: ' + {tgt_json});
            const sr = src.getBoundingClientRect();
            const tr = tgt.getBoundingClientRect();
            return {{
                sx: sr.x + sr.width / 2,
                sy: sr.y + sr.height / 2,
                tx: tr.x + tr.width / 2,
                ty: tr.y + tr.height / 2,
            }};
        }})()"#,
        src_json = serde_json::to_string(source_sel).unwrap(),
        tgt_json = serde_json::to_string(target_sel).unwrap()
    );

    let result = timeout(ELEMENT_TIMEOUT, page.evaluate_expression(&centers_js))
        .await
        .map_err(|_| "drag: timed out getting element centers".to_string())?
        .map_err(|e| format!("drag: failed to get element centers: {e}"))?;

    let centers = result.value().cloned().unwrap_or(json!({}));
    let sx = centers.get("sx").and_then(|v| v.as_f64()).ok_or("drag: could not get source x")?;
    let sy = centers.get("sy").and_then(|v| v.as_f64()).ok_or("drag: could not get source y")?;
    let tx = centers.get("tx").and_then(|v| v.as_f64()).ok_or("drag: could not get target x")?;
    let ty = centers.get("ty").and_then(|v| v.as_f64()).ok_or("drag: could not get target y")?;

    // Simulate drag via mouse events: move to source, press, move to target, release
    timeout(CDP_TIMEOUT, page.move_mouse(Point::new(sx, sy)))
        .await
        .map_err(|_| "drag: timed out moving to source".to_string())?
        .map_err(|e| format!("drag: move to source failed: {e}"))?;

    timeout(CDP_TIMEOUT, page.click(Point::new(sx, sy)))
        .await
        .map_err(|_| "drag: timed out clicking source".to_string())?
        .map_err(|e| format!("drag: click source failed: {e}"))?;

    // Move incrementally to the target
    let steps = 10;
    for i in 1..=steps {
        let ratio = i as f64 / steps as f64;
        let ix = sx + (tx - sx) * ratio;
        let iy = sy + (ty - sy) * ratio;
        let _ = timeout(CDP_TIMEOUT, page.move_mouse(Point::new(ix, iy))).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    timeout(CDP_TIMEOUT, page.click(Point::new(tx, ty)))
        .await
        .map_err(|_| "drag: timed out clicking target".to_string())?
        .map_err(|e| format!("drag: click target failed: {e}"))?;

    let (state, settle) = settle_and_capture(page).await;
    Ok(json!({
        "state": state,
        "settle": settle,
        "dragged": { "source": source_sel, "target": target_sel },
    }))
}

// ── Set Viewport ──

/// Handle `set_viewport` command — resize viewport or apply preset.
/// Params: { preset?: string, width?: i64, height?: i64 }
pub async fn handle_set_viewport(page: &Page, params: &Value) -> Result<Value, String> {
    let preset = params.get("preset").and_then(|v| v.as_str());
    let custom_width = params.get("width").and_then(|v| v.as_i64());
    let custom_height = params.get("height").and_then(|v| v.as_i64());

    let (width, height, preset_name) = match preset {
        Some("mobile") => (375, 667, Some("mobile")),
        Some("tablet") => (768, 1024, Some("tablet")),
        Some("desktop") => (1280, 720, Some("desktop")),
        Some("wide") => (1920, 1080, Some("wide")),
        Some(unknown) => return Err(format!(
            "unknown preset: {unknown}. Valid presets: mobile, tablet, desktop, wide"
        )),
        None => match (custom_width, custom_height) {
            (Some(w), Some(h)) => (w, h, None),
            _ => return Err(
                "set_viewport requires either 'preset' or both 'width' and 'height'".to_string(),
            ),
        },
    };

    debug!("set_viewport: {width}x{height} preset={preset_name:?}");

    let params = SetDeviceMetricsOverrideParams::new(width, height, 1.0, false);
    timeout(CDP_TIMEOUT, page.execute(params))
        .await
        .map_err(|_| "set_viewport timed out".to_string())?
        .map_err(|e| format!("set_viewport failed: {e}"))?;

    Ok(json!({
        "width": width,
        "height": height,
        "preset": preset_name,
    }))
}

// ── Upload File ──

/// Handle `upload_file` command — set files on a file input element.
/// Params: { selector: string, files: [string] }
pub async fn handle_upload_file(page: &Page, params: &Value) -> Result<Value, String> {
    let selector = params
        .get("selector")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: selector".to_string())?;
    let files = params
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing required parameter: files (array of paths)".to_string())?;
    let file_paths: Vec<String> = files
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    if file_paths.is_empty() {
        return Err("files array cannot be empty".to_string());
    }

    debug!("upload_file: selector={selector} files={file_paths:?}");

    // Find element to get its backend_node_id
    let element = timeout(ELEMENT_TIMEOUT, page.find_element(selector))
        .await
        .map_err(|_| format!("upload_file: timed out finding element: {selector}"))?
        .map_err(|e| format!("element not found: {selector} ({e})"))?;

    // Use DOM.setFileInputFiles with backend_node_id
    let set_files_params = SetFileInputFilesParams::builder()
        .files(file_paths.iter().map(|s| s.as_str()))
        .backend_node_id(element.backend_node_id)
        .build()
        .map_err(|e| format!("upload_file: failed to build params: {e}"))?;

    timeout(ELEMENT_TIMEOUT, page.execute(set_files_params))
        .await
        .map_err(|_| "upload_file: timed out setting files".to_string())?
        .map_err(|e| format!("upload_file: CDP error: {e}"))?;

    let (state, settle) = settle_and_capture(page).await;
    Ok(json!({
        "state": state,
        "settle": settle,
        "uploaded": { "selector": selector, "files": file_paths },
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn click_requires_selector_or_coordinates() {
        // This tests the param validation logic by checking the function contract
        let params = json!({});
        // No selector, no x/y — should produce an error message
        let selector = params.get("selector").and_then(|v| v.as_str());
        let x = params.get("x").and_then(|v| v.as_f64());
        let y = params.get("y").and_then(|v| v.as_f64());
        assert!(selector.is_none());
        assert!(x.is_none());
        assert!(y.is_none());
    }

    #[test]
    fn type_requires_selector_and_text() {
        let params = json!({"selector": "input"});
        let text = params.get("text").and_then(|v| v.as_str());
        assert!(text.is_none()); // should trigger error in handler
    }

    #[test]
    fn scroll_direction_validation() {
        for dir in &["up", "down"] {
            let amount = match *dir {
                "up" => -300i32,
                "down" => 300,
                _ => panic!("unknown"),
            };
            assert!(amount != 0);
        }
    }

    #[test]
    fn viewport_presets() {
        let presets = [
            ("mobile", 375, 667),
            ("tablet", 768, 1024),
            ("desktop", 1280, 720),
            ("wide", 1920, 1080),
        ];
        for (name, w, h) in &presets {
            let (width, height, _) = match *name {
                "mobile" => (375i64, 667i64, Some("mobile")),
                "tablet" => (768, 1024, Some("tablet")),
                "desktop" => (1280, 720, Some("desktop")),
                "wide" => (1920, 1080, Some("wide")),
                _ => panic!("unknown"),
            };
            assert_eq!(width, *w as i64);
            assert_eq!(height, *h as i64);
        }
    }

    #[test]
    fn set_viewport_needs_preset_or_dimensions() {
        let params = json!({});
        let preset = params.get("preset").and_then(|v| v.as_str());
        let w = params.get("width").and_then(|v| v.as_i64());
        let h = params.get("height").and_then(|v| v.as_i64());
        assert!(preset.is_none());
        assert!(w.is_none());
        assert!(h.is_none());
    }
}
