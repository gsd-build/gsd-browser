//! Playwright test code generation from the action timeline.

use crate::daemon::state::DaemonState;
use gsd_browser_common::state_dir;
use serde_json::{json, Value};
use std::fs;

/// Generate a Playwright test script from the action timeline.
pub fn handle_generate_test(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("recorded-session");

    let output_path = params
        .get("outputPath")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let include_assertions = params
        .get("includeAssertions")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Read timeline snapshot
    let timeline = state.timeline.lock().unwrap();
    let entries = timeline.snapshot();
    drop(timeline);

    if entries.is_empty() {
        return Err("no actions recorded in timeline — nothing to generate".to_string());
    }

    // Build the Playwright test script
    let mut lines: Vec<String> = Vec::new();
    lines.push("import { test, expect } from '@playwright/test';".to_string());
    lines.push(String::new());
    lines.push(format!("test.describe('{name}', () => {{"));
    lines.push(format!("  test('{name}', async ({{ page }}) => {{"));

    for entry in &entries {
        let tool = entry.tool.as_str();
        let params_str = &entry.params_summary;

        match tool {
            "navigate" => {
                // Extract URL from params or after_url
                let url = extract_param_str(params_str, "url")
                    .unwrap_or_else(|| entry.after_url.clone());
                if !url.is_empty() {
                    lines.push(format!("    await page.goto('{}');", escape_js(&url)));
                }
            }
            "back" => {
                lines.push("    await page.goBack();".to_string());
            }
            "forward" => {
                lines.push("    await page.goForward();".to_string());
            }
            "reload" => {
                lines.push("    await page.reload();".to_string());
            }
            "click" | "click_ref" => {
                if let Some(sel) = extract_param_str(params_str, "selector") {
                    lines.push(format!("    await page.click('{}');", escape_js(&sel)));
                } else if let Some(r) = extract_param_str(params_str, "ref") {
                    lines.push(format!(
                        "    // ref-based click: {r} — resolve selector manually"
                    ));
                }
            }
            "type" | "fill_ref" => {
                let sel = extract_param_str(params_str, "selector").unwrap_or_default();
                let text = extract_param_str(params_str, "text").unwrap_or_default();
                if !sel.is_empty() {
                    lines.push(format!(
                        "    await page.fill('{}', '{}');",
                        escape_js(&sel),
                        escape_js(&text)
                    ));
                }
            }
            "press" => {
                let key = extract_param_str(params_str, "key").unwrap_or_default();
                if !key.is_empty() {
                    lines.push(format!(
                        "    await page.keyboard.press('{}');",
                        escape_js(&key)
                    ));
                }
            }
            "hover" | "hover_ref" => {
                if let Some(sel) = extract_param_str(params_str, "selector") {
                    lines.push(format!("    await page.hover('{}');", escape_js(&sel)));
                }
            }
            "scroll" => {
                let direction = extract_param_str(params_str, "direction").unwrap_or_default();
                let amount = extract_param_str(params_str, "amount").unwrap_or("300".to_string());
                let delta = if direction == "up" {
                    format!("-{amount}")
                } else {
                    amount
                };
                lines.push(format!(
                    "    await page.mouse.wheel(0, {delta});"
                ));
            }
            "select_option" => {
                let sel = extract_param_str(params_str, "selector").unwrap_or_default();
                let opt = extract_param_str(params_str, "option").unwrap_or_default();
                if !sel.is_empty() {
                    lines.push(format!(
                        "    await page.selectOption('{}', '{}');",
                        escape_js(&sel),
                        escape_js(&opt)
                    ));
                }
            }
            "set_checked" => {
                let sel = extract_param_str(params_str, "selector").unwrap_or_default();
                let checked = params_str.contains("true");
                if !sel.is_empty() {
                    lines.push(format!(
                        "    await page.setChecked('{}', {checked});",
                        escape_js(&sel)
                    ));
                }
            }
            "wait_for" => {
                if let Some(condition) = extract_param_str(params_str, "condition") {
                    let value = extract_param_str(params_str, "value").unwrap_or_default();
                    match condition.as_str() {
                        "selector_visible" => {
                            lines.push(format!(
                                "    await page.waitForSelector('{}');",
                                escape_js(&value)
                            ));
                        }
                        "selector_hidden" => {
                            lines.push(format!(
                                "    await page.waitForSelector('{}', {{ state: 'hidden' }});",
                                escape_js(&value)
                            ));
                        }
                        "url_contains" => {
                            lines.push(format!(
                                "    await page.waitForURL('*{}*');",
                                escape_js(&value)
                            ));
                        }
                        "network_idle" => {
                            lines.push(
                                "    await page.waitForLoadState('networkidle');".to_string(),
                            );
                        }
                        "delay" => {
                            lines.push(format!(
                                "    await page.waitForTimeout({value});"
                            ));
                        }
                        "text_visible" => {
                            lines.push(format!(
                                "    await page.waitForSelector('text=\"{}\"');",
                                escape_js(&value)
                            ));
                        }
                        _ => {
                            lines.push(format!(
                                "    // wait_for {condition}: {value}"
                            ));
                        }
                    }
                }
            }
            "assert" => {
                if include_assertions {
                    lines.push(format!(
                        "    // assertion: {}",
                        truncate(params_str, 80)
                    ));
                }
            }
            _ => {
                // Other tools: emit as comment
                lines.push(format!("    // {tool}: {}", truncate(params_str, 60)));
            }
        }
    }

    lines.push("  });".to_string());
    lines.push("});".to_string());
    lines.push(String::new());

    let script = lines.join("\n");

    // Determine output path
    let file_path = if let Some(p) = output_path {
        p
    } else {
        let dir = state_dir().join("generated-tests");
        let _ = fs::create_dir_all(&dir);
        dir.join(format!("{name}.spec.ts"))
            .to_string_lossy()
            .to_string()
    };

    fs::write(&file_path, &script)
        .map_err(|e| format!("failed to write test file: {e}"))?;

    Ok(json!({
        "path": file_path,
        "actions": entries.len(),
        "lines": script.lines().count(),
    }))
}

/// Try to extract a named parameter from a truncated JSON params summary string.
fn extract_param_str(params_summary: &str, key: &str) -> Option<String> {
    // Try parsing as JSON first
    if let Ok(val) = serde_json::from_str::<Value>(params_summary) {
        return val.get(key).and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else {
                Some(v.to_string())
            }
        });
    }
    // Fallback: search for "key":"value" pattern
    let pattern = format!("\"{}\":\"", key);
    if let Some(start) = params_summary.find(&pattern) {
        let after = &params_summary[start + pattern.len()..];
        if let Some(end) = after.find('"') {
            return Some(after[..end].to_string());
        }
    }
    None
}

fn escape_js(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() > max {
        &s[..max]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::state::DaemonState;

    #[test]
    fn generate_test_empty_timeline() {
        let state = DaemonState::new();
        let err = handle_generate_test(&state, &json!({})).unwrap_err();
        assert!(err.contains("no actions"));
    }

    #[test]
    fn generate_test_with_entries() {
        let state = DaemonState::new();
        {
            let mut tl = state.timeline.lock().unwrap();
            tl.begin_action("navigate", r#"{"url":"https://example.com"}"#, "about:blank");
            tl.finish_action(1, "https://example.com", "ok", "");
            tl.begin_action("click", r#"{"selector":"button.submit"}"#, "https://example.com");
            tl.finish_action(2, "https://example.com", "ok", "");
        }

        let tmp = std::env::temp_dir().join("bt-test-gen.spec.ts");
        let result = handle_generate_test(
            &state,
            &json!({"name": "test-gen", "outputPath": tmp.to_str().unwrap()}),
        )
        .unwrap();

        assert_eq!(result["actions"], 2);
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("page.goto('https://example.com')"));
        assert!(content.contains("page.click('button.submit')"));
        let _ = std::fs::remove_file(tmp);
    }
}
