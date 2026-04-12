use chromiumoxide::Page;
use serde_json::{json, Value};
use tracing::debug;

/// Handle extract — run CSS selectors from a JSON schema against the page and return structured data.
///
/// Params:
///   schema   (object, required) — JSON schema with _selector and _attribute hints per property
///   selector (string, optional) — CSS selector to scope extraction to a container
///   multiple (bool, default false) — extract array of items from matching containers
pub async fn handle_extract(page: &Page, params: &Value) -> Result<Value, String> {
    let schema = params
        .get("schema")
        .ok_or("missing required param 'schema'")?;
    let scope_selector = params.get("selector").and_then(|v| v.as_str());
    let multiple = params
        .get("multiple")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    debug!("[extract] multiple={multiple}, scope={scope_selector:?}");

    // Build the extraction JS IIFE from the schema
    let js = build_extraction_js(schema, scope_selector, multiple)?;

    debug!("[extract] executing JS extraction");

    let eval_result = page
        .evaluate_expression(&js)
        .await
        .map_err(|e| format!("extraction JS failed: {}", super::clean_cdp_error(&e)))?;

    let raw_value = eval_result.value().cloned().unwrap_or(Value::Null);

    // The JS returns a JSON string, so parse it
    let data = if let Some(s) = raw_value.as_str() {
        serde_json::from_str::<Value>(s).unwrap_or(Value::String(s.to_string()))
    } else {
        raw_value
    };

    if multiple {
        let count = data.as_array().map(|a| a.len()).unwrap_or(0);
        Ok(json!({
            "data": data,
            "count": count,
            "multiple": true,
            "scope": scope_selector,
        }))
    } else {
        let field_count = data.as_object().map(|o| o.len()).unwrap_or(0);
        Ok(json!({
            "data": data,
            "fieldCount": field_count,
            "multiple": false,
            "scope": scope_selector,
        }))
    }
}

/// Build a JS IIFE that extracts data based on the schema definition.
fn build_extraction_js(
    schema: &Value,
    scope_selector: Option<&str>,
    multiple: bool,
) -> Result<String, String> {
    let properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .ok_or("schema must have a 'properties' object")?;

    // Build per-property extraction code
    let mut field_extractions = Vec::new();
    for (key, prop_def) in properties {
        let sel = prop_def
            .get("_selector")
            .and_then(|v| v.as_str())
            .unwrap_or("*");
        let attr = prop_def
            .get("_attribute")
            .and_then(|v| v.as_str())
            .unwrap_or("textContent");

        let sel_escaped = sel.replace('\\', "\\\\").replace('\'', "\\'");
        let attr_escaped = attr.replace('\\', "\\\\").replace('\'', "\\'");
        let key_escaped = key.replace('\\', "\\\\").replace('\'', "\\'");

        let extract_code = match attr {
            "textContent" | "innerText" => {
                format!(
                    "result['{key_escaped}'] = (ctx.querySelector('{sel_escaped}') || {{}}).{attr_escaped} || null;"
                )
            }
            "innerHTML" => {
                format!(
                    "result['{key_escaped}'] = (ctx.querySelector('{sel_escaped}') || {{}}).innerHTML || null;"
                )
            }
            _ => {
                format!(
                    "result['{key_escaped}'] = (ctx.querySelector('{sel_escaped}') || {{}}).getAttribute('{attr_escaped}') || null;"
                )
            }
        };

        field_extractions.push(extract_code);
    }

    let field_code = field_extractions.join("\n        ");

    let js = if multiple {
        let container_sel = scope_selector.unwrap_or("body > *");
        let container_escaped = container_sel.replace('\\', "\\\\").replace('\'', "\\'");
        format!(
            r#"(() => {{
    try {{
        const containers = document.querySelectorAll('{container_escaped}');
        const results = [];
        containers.forEach(ctx => {{
            const result = {{}};
            {field_code}
            results.push(result);
        }});
        return JSON.stringify(results);
    }} catch(e) {{
        return JSON.stringify({{error: e.message}});
    }}
}})()"#
        )
    } else {
        let scope_code = if let Some(sel) = scope_selector {
            let sel_escaped = sel.replace('\\', "\\\\").replace('\'', "\\'");
            format!("document.querySelector('{sel_escaped}') || document")
        } else {
            "document".to_string()
        };
        format!(
            r#"(() => {{
    try {{
        const ctx = {scope_code};
        const result = {{}};
        {field_code}
        return JSON.stringify(result);
    }} catch(e) {{
        return JSON.stringify({{error: e.message}});
    }}
}})()"#
        )
    };

    Ok(js)
}
