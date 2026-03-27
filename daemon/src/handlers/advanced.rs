//! Action cache (selector resolution caching) and prompt injection detection.

use crate::state::{CachedAction, DaemonState};
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// Handle action cache operations: stats, get, put, clear.
pub fn handle_action_cache(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("stats");

    let mut cache = state.action_cache.lock().unwrap();

    match action {
        "stats" => Ok(json!({
            "entries": cache.entries.len(),
            "hits": cache.hits,
            "misses": cache.misses,
            "hitRate": if cache.hits + cache.misses > 0 {
                cache.hits as f64 / (cache.hits + cache.misses) as f64
            } else {
                0.0
            },
        })),
        "get" => {
            let intent = params
                .get("intent")
                .and_then(|v| v.as_str())
                .ok_or("'intent' is required for action_cache get")?;
            let result = cache.entries.get(intent).map(|entry| {
                json!({
                    "found": true,
                    "selector": entry.selector,
                    "score": entry.score,
                    "cached_at": entry.cached_at,
                })
            });
            match result {
                Some(val) => {
                    cache.hits += 1;
                    Ok(val)
                }
                None => {
                    cache.misses += 1;
                    Ok(json!({
                        "found": false,
                    }))
                }
            }
        }
        "put" => {
            let intent = params
                .get("intent")
                .and_then(|v| v.as_str())
                .ok_or("'intent' is required for action_cache put")?;
            let selector = params
                .get("selector")
                .and_then(|v| v.as_str())
                .ok_or("'selector' is required for action_cache put")?;
            let score = params
                .get("score")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);

            cache.entries.insert(
                intent.to_string(),
                CachedAction {
                    selector: selector.to_string(),
                    score,
                    cached_at: now_secs(),
                },
            );
            Ok(json!({
                "stored": true,
                "intent": intent,
                "selector": selector,
            }))
        }
        "clear" => {
            let count = cache.entries.len();
            cache.entries.clear();
            cache.hits = 0;
            cache.misses = 0;
            Ok(json!({
                "cleared": count,
            }))
        }
        _ => Err(format!("unknown action_cache action: {action}")),
    }
}

/// JavaScript IIFE that scans page text and hidden elements for prompt injection patterns.
const INJECTION_SCAN_JS: &str = r#"
(() => {
    const findings = [];
    const patterns = [
        { name: 'script_tag', regex: /<script[\s>]/gi, severity: 'high', description: 'Inline script tag found' },
        { name: 'event_handler', regex: /\bon\w+\s*=/gi, severity: 'high', description: 'Event handler attribute found' },
        { name: 'data_uri', regex: /data:\s*(?:text\/html|application\/javascript)/gi, severity: 'high', description: 'Data URI with executable content' },
        { name: 'base64_js', regex: /(?:atob|btoa|eval)\s*\(/gi, severity: 'medium', description: 'Base64/eval function call' },
        { name: 'template_syntax', regex: /\{\{.*?\}\}|\$\{.*?\}/gs, severity: 'low', description: 'Template syntax found' },
        { name: 'sql_pattern', regex: /(?:SELECT|INSERT|UPDATE|DELETE|DROP|UNION)\s+/gi, severity: 'medium', description: 'SQL-like pattern found' },
        { name: 'prompt_override', regex: /(?:ignore|disregard|forget)\s+(?:previous|above|all)\s+(?:instructions|prompts|rules)/gi, severity: 'high', description: 'Prompt override attempt' },
        { name: 'system_prompt', regex: /(?:you\s+are|act\s+as|pretend\s+to\s+be|new\s+instructions?:)/gi, severity: 'high', description: 'System prompt injection attempt' },
    ];

    const includeHidden = arguments[0] !== false;

    // Scan visible text
    const visibleText = document.body ? document.body.innerText : '';
    for (const p of patterns) {
        const matches = visibleText.match(p.regex);
        if (matches) {
            findings.push({
                pattern: p.name,
                severity: p.severity,
                description: p.description,
                count: matches.length,
                source: 'visible_text',
                samples: matches.slice(0, 3),
            });
        }
    }

    // Scan hidden elements
    if (includeHidden && document.body) {
        const hiddenEls = document.querySelectorAll(
            '[style*="display:none"], [style*="display: none"], [style*="visibility:hidden"], ' +
            '[style*="visibility: hidden"], [style*="opacity:0"], [style*="opacity: 0"], ' +
            '[hidden], [aria-hidden="true"], .sr-only, .visually-hidden'
        );
        for (const el of hiddenEls) {
            const text = el.textContent || '';
            if (text.length < 5) continue;
            for (const p of patterns) {
                const matches = text.match(p.regex);
                if (matches) {
                    findings.push({
                        pattern: p.name,
                        severity: p.severity,
                        description: p.description,
                        count: matches.length,
                        source: 'hidden_element',
                        element: el.tagName.toLowerCase() + (el.className ? '.' + el.className.split(' ')[0] : ''),
                        samples: matches.slice(0, 3),
                    });
                }
            }
        }
    }

    return { findings, scannedAt: Date.now() };
})()
"#;

/// Scan page content for prompt injection patterns.
pub async fn handle_check_injection(page: &Page, params: &Value) -> Result<Value, String> {
    let include_hidden = params
        .get("includeHidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let call_js = format!(
        r#"
        (() => {{
            const includeHidden = {};
            {}
        }})()
        "#,
        if include_hidden { "true" } else { "false" },
        // Inline the scan logic, replacing arguments[0] with includeHidden
        INJECTION_SCAN_JS
            .replace("arguments[0]", "includeHidden")
            .trim_start_matches('\n')
    );

    let result = page
        .evaluate(call_js)
        .await
        .map_err(|e| format!("injection scan failed: {e}"))?;

    let val = result.into_value::<Value>().unwrap_or(json!(null));

    let findings = val
        .get("findings")
        .cloned()
        .unwrap_or(json!([]));

    let finding_arr = findings.as_array().map(|a| a.len()).unwrap_or(0);

    Ok(json!({
        "findings": findings,
        "count": finding_arr,
        "includeHidden": include_hidden,
        "clean": finding_arr == 0,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DaemonState;

    #[test]
    fn action_cache_stats_empty() {
        let state = DaemonState::new();
        let result = handle_action_cache(&state, &json!({"action": "stats"})).unwrap();
        assert_eq!(result["entries"], 0);
        assert_eq!(result["hits"], 0);
        assert_eq!(result["misses"], 0);
    }

    #[test]
    fn action_cache_put_get_clear() {
        let state = DaemonState::new();

        // Put
        let r = handle_action_cache(
            &state,
            &json!({"action": "put", "intent": "submit_form", "selector": "button[type=submit]", "score": 0.95}),
        ).unwrap();
        assert_eq!(r["stored"], true);

        // Get — hit
        let r = handle_action_cache(&state, &json!({"action": "get", "intent": "submit_form"})).unwrap();
        assert_eq!(r["found"], true);
        assert_eq!(r["selector"], "button[type=submit]");

        // Get — miss
        let r = handle_action_cache(&state, &json!({"action": "get", "intent": "nonexistent"})).unwrap();
        assert_eq!(r["found"], false);

        // Stats after 1 hit + 1 miss
        let r = handle_action_cache(&state, &json!({"action": "stats"})).unwrap();
        assert_eq!(r["entries"], 1);
        assert_eq!(r["hits"], 1);
        assert_eq!(r["misses"], 1);

        // Clear
        let r = handle_action_cache(&state, &json!({"action": "clear"})).unwrap();
        assert_eq!(r["cleared"], 1);

        let r = handle_action_cache(&state, &json!({"action": "stats"})).unwrap();
        assert_eq!(r["entries"], 0);
    }
}
