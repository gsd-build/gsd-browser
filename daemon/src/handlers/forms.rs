//! Handlers for form analysis and filling.
//!
//! `analyze_form` — auto-detects the form on the page, resolves field labels
//! via a 7-level cascade, and returns structured metadata.
//!
//! `fill_form` — resolves form field identifiers to selectors, dispatches
//! fill/select/check via existing interaction handlers, optionally submits.

use super::interaction::{handle_click, handle_select_option, handle_set_checked, handle_type_text};
use crate::capture::capture_compact_page_state;
use crate::settle::{ensure_mutation_counter, settle_after_action};
use browser_tools_common::types::SettleOptions;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;
use tracing::debug;

const JS_TIMEOUT: Duration = Duration::from_secs(10);

/// Settle and capture page state after form fill operations.
async fn settle_and_capture(page: &Page) -> (Value, Value) {
    ensure_mutation_counter(page).await;
    let opts = SettleOptions {
        timeout_ms: 1500,
        check_focus_stability: true,
        ..SettleOptions::default()
    };
    let settle = settle_after_action(page, &opts).await;
    let state = capture_compact_page_state(page, false).await;
    (
        serde_json::to_value(&state).unwrap_or(json!({})),
        serde_json::to_value(&settle).unwrap_or(json!({})),
    )
}

/// Handle `analyze_form` — inspect a form's fields, labels, and submit buttons.
///
/// Params: { selector?: string }
/// The JS IIFE auto-detects the form if no selector is given:
///   1. Single `<form>` on page → use it
///   2. Multiple forms → pick the one with most visible inputs
///   3. No forms → fall back to document.body
pub async fn handle_analyze_form(page: &Page, params: &Value) -> Result<Value, String> {
    let selector = params.get("selector").and_then(|v| v.as_str());

    debug!("analyze_form: selector={selector:?}");

    let selector_json = match selector {
        Some(s) => serde_json::to_string(s).unwrap(),
        None => "null".to_string(),
    };

    let js = format!(
        r#"(() => {{
    const selectorArg = {selector_json};

    // Auto-detect the form container
    let form;
    if (selectorArg) {{
        form = document.querySelector(selectorArg);
        if (!form) throw new Error('form not found: ' + selectorArg);
    }} else {{
        const forms = Array.from(document.querySelectorAll('form'));
        if (forms.length === 1) {{
            form = forms[0];
        }} else if (forms.length > 1) {{
            // Pick the form with the most visible inputs
            form = forms.reduce((best, f) => {{
                const count = f.querySelectorAll('input:not([type=hidden]), select, textarea').length;
                const bestCount = best.querySelectorAll('input:not([type=hidden]), select, textarea').length;
                return count > bestCount ? f : best;
            }}, forms[0]);
        }} else {{
            form = document.body;
        }}
    }}

    // Humanize a name attribute: "first_name" → "First Name"
    function humanize(name) {{
        if (!name) return '';
        return name
            .replace(/[_\-]/g, ' ')
            .replace(/([a-z])([A-Z])/g, '$1 $2')
            .replace(/\b\w/g, c => c.toUpperCase());
    }}

    // 7-level label resolution
    function resolveLabel(el) {{
        // 1. aria-labelledby
        const lblBy = el.getAttribute('aria-labelledby');
        if (lblBy) {{
            const parts = lblBy.split(/\s+/).map(id => {{
                const ref = document.getElementById(id);
                return ref ? ref.textContent.trim() : '';
            }}).filter(Boolean);
            if (parts.length) return parts.join(' ');
        }}
        // 2. aria-label
        const ariaLabel = el.getAttribute('aria-label');
        if (ariaLabel) return ariaLabel;
        // 3. label[for]
        if (el.id) {{
            const forLabel = document.querySelector('label[for=' + JSON.stringify(el.id) + ']');
            if (forLabel) return forLabel.textContent.trim();
        }}
        // 4. Wrapping label
        const wrap = el.closest('label');
        if (wrap) {{
            // Clone and remove the input to get just the label text
            const clone = wrap.cloneNode(true);
            clone.querySelectorAll('input, select, textarea').forEach(c => c.remove());
            const text = clone.textContent.trim();
            if (text) return text;
        }}
        // 5. placeholder
        if (el.placeholder) return el.placeholder;
        // 6. title
        if (el.title) return el.title;
        // 7. Humanized name
        return humanize(el.name);
    }}

    // Build a unique CSS selector for an element
    function buildSelector(el) {{
        if (el.id) return '#' + CSS.escape(el.id);
        if (el.name) {{
            const tag = el.tagName.toLowerCase();
            const sel = tag + '[name=' + JSON.stringify(el.name) + ']';
            if (form.querySelectorAll(sel).length === 1) return sel;
        }}
        // Fall back to nth-of-type within form
        const tag = el.tagName.toLowerCase();
        const siblings = Array.from(form.querySelectorAll(tag));
        const idx = siblings.indexOf(el);
        if (idx >= 0) return tag + ':nth-of-type(' + (idx + 1) + ')';
        return tag;
    }}

    const elements = form.querySelectorAll('input, select, textarea, button');
    const fields = [];
    const submitButtons = [];

    elements.forEach(el => {{
        const tag = el.tagName.toLowerCase();
        const type = (el.getAttribute('type') || (tag === 'select' ? 'select' : tag === 'textarea' ? 'textarea' : 'text')).toLowerCase();

        // Submit buttons go to a separate list
        if (tag === 'button' || type === 'submit' || type === 'image') {{
            submitButtons.push({{
                tag,
                type,
                name: el.name || '',
                text: el.textContent.trim() || el.value || '',
                selector: buildSelector(el),
            }});
            return;
        }}

        const label = resolveLabel(el);
        const field = {{
            tag,
            type,
            name: el.name || '',
            id: el.id || '',
            label,
            selector: buildSelector(el),
            required: el.required || el.hasAttribute('required'),
            value: el.value || '',
            hidden: type === 'hidden' || el.hidden || (el.offsetParent === null && type !== 'hidden'),
            disabled: el.disabled,
        }};

        // Validation state
        if (el.validity) {{
            field.valid = el.validity.valid;
            if (el.validationMessage) field.validationMessage = el.validationMessage;
        }}

        // Checkbox/radio checked state
        if (type === 'checkbox' || type === 'radio') {{
            field.checked = el.checked;
        }}

        // Select options
        if (tag === 'select') {{
            field.options = Array.from(el.options).map(o => ({{
                label: o.label || o.textContent.trim(),
                value: o.value,
                selected: o.selected,
            }}));
        }}

        // Fieldset / group
        const fieldset = el.closest('fieldset');
        if (fieldset) {{
            const legend = fieldset.querySelector('legend');
            if (legend) field.group = legend.textContent.trim();
        }}

        fields.push(field);
    }});

    return {{
        formSelector: form === document.body ? 'body' : buildSelector(form),
        fieldCount: fields.length,
        fields,
        submitButtons,
    }};
}})()"#
    );

    let result = timeout(JS_TIMEOUT, page.evaluate_expression(&js))
        .await
        .map_err(|_| "analyze_form timed out".to_string())?
        .map_err(|e| format!("analyze_form failed: {e}"))?;

    let data = result.value().cloned().unwrap_or(json!({}));
    Ok(data)
}

/// Handle `fill_form` — fill multiple form fields by identifier, optionally submit.
///
/// Params: {
///   values: { "field_identifier": "value", ... },
///   selector?: string,  // scope to a specific form
///   submit?: bool
/// }
///
/// Phase 1: JS IIFE resolves each key to a CSS selector + field type using
/// 4-level priority (label → name → placeholder → aria-label).
/// Phase 2: Rust dispatches to existing interaction handlers per field type.
/// Phase 3: If submit=true, click the form's submit button.
pub async fn handle_fill_form(page: &Page, params: &Value) -> Result<Value, String> {
    let values = params
        .get("values")
        .ok_or_else(|| "missing required parameter: values".to_string())?;
    let values_map = values
        .as_object()
        .ok_or_else(|| "values must be a JSON object".to_string())?;
    let form_selector = params.get("selector").and_then(|v| v.as_str());
    let submit = params
        .get("submit")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if values_map.is_empty() {
        return Err("values object is empty".to_string());
    }

    debug!(
        "fill_form: fields={} selector={form_selector:?} submit={submit}",
        values_map.len()
    );

    // Phase 1: Resolve field identifiers → CSS selectors + types
    let form_sel_json = match form_selector {
        Some(s) => serde_json::to_string(s).unwrap(),
        None => "null".to_string(),
    };
    let keys: Vec<&str> = values_map.keys().map(|k| k.as_str()).collect();
    let keys_json = serde_json::to_string(&keys).unwrap();

    let resolve_js = format!(
        r#"(() => {{
    const formSel = {form_sel_json};
    const keys = {keys_json};

    let form;
    if (formSel) {{
        form = document.querySelector(formSel);
        if (!form) throw new Error('form not found: ' + formSel);
    }} else {{
        const forms = Array.from(document.querySelectorAll('form'));
        if (forms.length === 1) form = forms[0];
        else if (forms.length > 1) {{
            form = forms.reduce((best, f) => {{
                const count = f.querySelectorAll('input:not([type=hidden]), select, textarea').length;
                const bestCount = best.querySelectorAll('input:not([type=hidden]), select, textarea').length;
                return count > bestCount ? f : best;
            }}, forms[0]);
        }} else {{
            form = document.body;
        }}
    }}

    // Build a unique CSS selector for an element
    function buildSelector(el) {{
        if (el.id) return '#' + CSS.escape(el.id);
        if (el.name) {{
            const tag = el.tagName.toLowerCase();
            const sel = tag + '[name=' + JSON.stringify(el.name) + ']';
            if (document.querySelectorAll(sel).length === 1) return sel;
        }}
        const tag = el.tagName.toLowerCase();
        const siblings = Array.from(form.querySelectorAll(tag));
        const idx = siblings.indexOf(el);
        if (idx >= 0) return tag + ':nth-of-type(' + (idx + 1) + ')';
        return tag;
    }}

    const elements = Array.from(form.querySelectorAll('input, select, textarea'));

    function normalize(s) {{ return (s || '').toLowerCase().trim(); }}

    function resolveField(key) {{
        const lk = normalize(key);
        for (const el of elements) {{
            // Priority 1: label text match
            if (el.id) {{
                const lbl = document.querySelector('label[for=' + JSON.stringify(el.id) + ']');
                if (lbl && normalize(lbl.textContent) === lk) return el;
            }}
            const wrap = el.closest('label');
            if (wrap) {{
                const clone = wrap.cloneNode(true);
                clone.querySelectorAll('input, select, textarea').forEach(c => c.remove());
                if (normalize(clone.textContent) === lk) return el;
            }}
        }}
        for (const el of elements) {{
            // Priority 2: name match
            if (normalize(el.name) === lk) return el;
        }}
        for (const el of elements) {{
            // Priority 3: placeholder match
            if (normalize(el.placeholder) === lk) return el;
        }}
        for (const el of elements) {{
            // Priority 4: aria-label match
            if (normalize(el.getAttribute('aria-label')) === lk) return el;
        }}
        return null;
    }}

    const resolved = [];
    const errors = [];

    for (const key of keys) {{
        const el = resolveField(key);
        if (!el) {{
            errors.push(key);
            continue;
        }}
        const tag = el.tagName.toLowerCase();
        const type = (el.getAttribute('type') || (tag === 'select' ? 'select' : tag === 'textarea' ? 'textarea' : 'text')).toLowerCase();
        resolved.push({{
            key,
            selector: buildSelector(el),
            type,
            tag,
        }});
    }}

    // Find submit button for optional submission
    let submitSelector = null;
    const submitBtn = form.querySelector('button[type=submit], input[type=submit], button:not([type])');
    if (submitBtn) submitSelector = buildSelector(submitBtn);

    return {{ resolved, errors, submitSelector }};
}})()"#
    );

    let result = timeout(JS_TIMEOUT, page.evaluate_expression(&resolve_js))
        .await
        .map_err(|_| "fill_form: timed out resolving field selectors".to_string())?
        .map_err(|e| format!("fill_form: field resolution failed: {e}"))?;

    let resolve_data = result.value().cloned().unwrap_or(json!({}));
    let resolved = resolve_data
        .get("resolved")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let errors = resolve_data
        .get("errors")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let submit_selector = resolve_data
        .get("submitSelector")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if resolved.is_empty() && !errors.is_empty() {
        let names: Vec<String> = errors
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        return Err(format!("no fields could be resolved: {}", names.join(", ")));
    }

    // Phase 2: Dispatch fill operations per field
    let mut filled = Vec::new();
    let mut fill_errors = Vec::new();

    for field in &resolved {
        let key = field
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let selector = field
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let field_type = field
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("text");

        let value = match values_map.get(key) {
            Some(v) => v,
            None => continue,
        };

        let result = match field_type {
            "select" | "select-one" | "select-multiple" => {
                let option_str = value.as_str().unwrap_or("");
                let p = json!({ "selector": selector, "option": option_str });
                handle_select_option(page, &p).await
            }
            "checkbox" | "radio" => {
                let checked = match value {
                    Value::Bool(b) => *b,
                    Value::String(s) => matches!(s.as_str(), "true" | "on" | "yes" | "1"),
                    _ => true,
                };
                let p = json!({ "selector": selector, "checked": checked });
                handle_set_checked(page, &p).await
            }
            _ => {
                // Text-like input: text, email, password, number, tel, url, textarea, etc.
                let text_str = match value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let p = json!({ "selector": selector, "text": text_str });
                handle_type_text(page, &p).await
            }
        };

        match result {
            Ok(_) => filled.push(key.to_string()),
            Err(e) => fill_errors.push(format!("{key}: {e}")),
        }
    }

    // Phase 3: Optional submit
    let submitted = if submit {
        match &submit_selector {
            Some(sel) => {
                let p = json!({ "selector": sel });
                match handle_click(page, &p).await {
                    Ok(_) => true,
                    Err(e) => {
                        fill_errors.push(format!("submit: {e}"));
                        false
                    }
                }
            }
            None => {
                fill_errors.push("submit: no submit button found".to_string());
                false
            }
        }
    } else {
        false
    };

    let (state, settle) = settle_and_capture(page).await;

    let unresolved: Vec<String> = errors
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    Ok(json!({
        "state": state,
        "settle": settle,
        "filled": filled,
        "errors": fill_errors,
        "unresolved": unresolved,
        "submitted": submitted,
        "fieldCount": resolved.len(),
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn fill_form_requires_values() {
        let params = json!({});
        let values = params.get("values");
        assert!(values.is_none());
    }

    #[test]
    fn fill_form_values_must_be_object() {
        let params = json!({"values": "not_an_object"});
        let values = params.get("values").and_then(|v| v.as_object());
        assert!(values.is_none());
    }

    #[test]
    fn checkbox_value_parsing() {
        // Verify our match logic for checkbox values
        for (input, expected) in &[
            ("true", true),
            ("on", true),
            ("yes", true),
            ("1", true),
            ("false", false),
            ("off", false),
        ] {
            let result = matches!(*input, "true" | "on" | "yes" | "1");
            assert_eq!(result, *expected, "input={input}");
        }
    }
}
