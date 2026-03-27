//! Handlers for intent-scored element discovery and semantic actions.
//!
//! `find_best` — scores visible interactive elements against 8 semantic intents,
//! returns the top 5 candidates with scores and reasons.
//!
//! `act` — composite handler: runs find_best internally, takes the top candidate,
//! dispatches click (or focus for search_field), settles, and captures state.

use super::interaction::handle_click;
use crate::capture::capture_compact_page_state;
use crate::settle::{ensure_mutation_counter, settle_after_action};
use browser_tools_common::types::SettleOptions;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;
use tracing::debug;

const JS_TIMEOUT: Duration = Duration::from_secs(10);

/// Settle and capture page state after intent actions.
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

/// The JS IIFE that scores candidates for a given intent.
/// Shared between find_best and act.
fn intent_scoring_js(intent: &str, scope_selector: Option<&str>) -> String {
    let intent_json = serde_json::to_string(intent).unwrap();
    let scope_json = match scope_selector {
        Some(s) => serde_json::to_string(s).unwrap(),
        None => "null".to_string(),
    };

    format!(
        r#"(() => {{
    const intent = {intent_json};
    const scopeSel = {scope_json};

    const root = scopeSel ? document.querySelector(scopeSel) : document;
    if (!root) throw new Error('scope element not found: ' + scopeSel);

    // Collect all interactive elements
    const candidates = Array.from(root.querySelectorAll(
        'a, button, input, select, textarea, [role=button], [role=link], [role=menuitem], ' +
        '[role=tab], [role=search], [role=searchbox], [tabindex], [onclick]'
    ));

    function isVisible(el) {{
        if (el.hidden || el.disabled) return false;
        const rect = el.getBoundingClientRect();
        if (rect.width === 0 && rect.height === 0) return false;
        const style = getComputedStyle(el);
        if (style.display === 'none' || style.visibility === 'hidden' || parseFloat(style.opacity) === 0) return false;
        return true;
    }}

    function getText(el) {{
        return (el.textContent || '').trim().substring(0, 100).toLowerCase();
    }}

    function getAriaLabel(el) {{
        return (el.getAttribute('aria-label') || '').toLowerCase();
    }}

    function getRole(el) {{
        return (el.getAttribute('role') || '').toLowerCase();
    }}

    function buildSelector(el) {{
        if (el.id) return '#' + CSS.escape(el.id);
        const tag = el.tagName.toLowerCase();
        // Try data-testid
        const testId = el.getAttribute('data-testid');
        if (testId) return tag + '[data-testid=' + JSON.stringify(testId) + ']';
        // Try unique name
        if (el.name) {{
            const sel = tag + '[name=' + JSON.stringify(el.name) + ']';
            if (document.querySelectorAll(sel).length === 1) return sel;
        }}
        // Try unique text content for buttons/links
        if ((tag === 'button' || tag === 'a') && el.textContent.trim()) {{
            const text = el.textContent.trim();
            const candidates = Array.from(document.querySelectorAll(tag)).filter(
                e => e.textContent.trim() === text
            );
            if (candidates.length === 1 && candidates[0] === el) {{
                // Can't easily use :contains in CSS, use a class or type
            }}
        }}
        // Use type attribute for inputs
        if (el.type) {{
            const sel = tag + '[type=' + JSON.stringify(el.type) + ']';
            if (document.querySelectorAll(sel).length === 1) return sel;
        }}
        // Fallback: nth-of-type
        const all = Array.from(document.querySelectorAll(tag));
        const idx = all.indexOf(el);
        return tag + ':nth-of-type(' + (idx + 1) + ')';
    }}

    // Intent-specific scoring functions
    const intentScorers = {{
        submit_form(el, tag, type, text, role, aria) {{
            let score = 0;
            let reasons = [];
            if (type === 'submit') {{ score += 0.5; reasons.push('type=submit'); }}
            if (tag === 'button' && !el.type) {{ score += 0.2; reasons.push('button no-type'); }}
            if (/submit|send|save|confirm|create|register|sign.?up|log.?in|continue|next|apply|ok/i.test(text || el.value || aria)) {{
                score += 0.3; reasons.push('submit-like text');
            }}
            if (el.closest('form')) {{ score += 0.15; reasons.push('inside form'); }}
            if (role === 'button') {{ score += 0.05; reasons.push('role=button'); }}
            return {{ score, reasons }};
        }},
        close_dialog(el, tag, type, text, role, aria) {{
            let score = 0;
            let reasons = [];
            if (/close|dismiss|cancel|×|✕|x/i.test(text || aria)) {{
                score += 0.4; reasons.push('close/dismiss text');
            }}
            const inDialog = el.closest('dialog, [role=dialog], [role=alertdialog], .modal');
            if (inDialog) {{ score += 0.3; reasons.push('inside dialog/modal'); }}
            if (aria && /close|dismiss/i.test(aria)) {{ score += 0.2; reasons.push('aria close/dismiss'); }}
            if (tag === 'button') {{ score += 0.05; reasons.push('is button'); }}
            return {{ score, reasons }};
        }},
        primary_cta(el, tag, type, text, role, aria) {{
            let score = 0;
            let reasons = [];
            if (tag === 'button' || tag === 'a' || role === 'button') {{
                score += 0.15; reasons.push('interactive element');
            }}
            // Large / prominent
            const rect = el.getBoundingClientRect();
            const area = rect.width * rect.height;
            if (area > 3000) {{ score += 0.15; reasons.push('large area'); }}
            // Primary styling indicators
            const style = getComputedStyle(el);
            const bg = style.backgroundColor;
            if (bg && bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent') {{
                score += 0.2; reasons.push('has background color');
            }}
            if (/get.?started|sign.?up|try|buy|subscribe|download|start|learn.?more/i.test(text || aria)) {{
                score += 0.3; reasons.push('CTA-like text');
            }}
            // Position: earlier in DOM = more likely primary
            return {{ score, reasons }};
        }},
        search_field(el, tag, type, text, role, aria) {{
            let score = 0;
            let reasons = [];
            if (role === 'search' || role === 'searchbox') {{ score += 0.5; reasons.push('search role'); }}
            if (type === 'search') {{ score += 0.5; reasons.push('type=search'); }}
            if (tag === 'input' && /search/i.test(aria || el.placeholder || el.name || '')) {{
                score += 0.4; reasons.push('search in attributes');
            }}
            if (tag === 'input' || tag === 'textarea') {{ score += 0.05; reasons.push('is input'); }}
            return {{ score, reasons }};
        }},
        next_step(el, tag, type, text, role, aria) {{
            let score = 0;
            let reasons = [];
            if (/next|continue|proceed|forward|→|›|>>|step/i.test(text || aria)) {{
                score += 0.4; reasons.push('next-like text');
            }}
            if (tag === 'button' || role === 'button') {{ score += 0.15; reasons.push('is button'); }}
            if (type === 'submit') {{ score += 0.1; reasons.push('type=submit'); }}
            return {{ score, reasons }};
        }},
        dismiss(el, tag, type, text, role, aria) {{
            let score = 0;
            let reasons = [];
            if (/dismiss|close|cancel|no.?thanks|skip|later|not.?now|got.?it|ok|accept/i.test(text || aria)) {{
                score += 0.4; reasons.push('dismiss-like text');
            }}
            // Overlay / popup context
            const overlay = el.closest('[class*=overlay], [class*=popup], [class*=banner], [class*=toast], [class*=notification]');
            if (overlay) {{ score += 0.2; reasons.push('inside overlay/popup'); }}
            if (tag === 'button') {{ score += 0.1; reasons.push('is button'); }}
            return {{ score, reasons }};
        }},
        auth_action(el, tag, type, text, role, aria) {{
            let score = 0;
            let reasons = [];
            if (/log.?in|sign.?in|sign.?up|register|auth|sso|forgot.?password/i.test(text || aria)) {{
                score += 0.4; reasons.push('auth-like text');
            }}
            if (type === 'submit' && el.closest('form')) {{
                const form = el.closest('form');
                const hasPassword = form.querySelector('input[type=password]');
                if (hasPassword) {{ score += 0.3; reasons.push('form has password field'); }}
            }}
            if (tag === 'button' || tag === 'a') {{ score += 0.1; reasons.push('interactive element'); }}
            return {{ score, reasons }};
        }},
        back_navigation(el, tag, type, text, role, aria) {{
            let score = 0;
            let reasons = [];
            if (/back|previous|←|‹|<<|return|go.?back/i.test(text || aria)) {{
                score += 0.4; reasons.push('back-like text');
            }}
            if (tag === 'a' && el.href) {{
                // Link that goes "up" in path
                try {{
                    const url = new URL(el.href);
                    if (url.pathname.length < location.pathname.length) {{
                        score += 0.2; reasons.push('shorter path');
                    }}
                }} catch(e) {{}}
            }}
            if (role === 'navigation' || el.closest('nav')) {{
                score += 0.1; reasons.push('in navigation');
            }}
            return {{ score, reasons }};
        }},
    }};

    const scorer = intentScorers[intent];
    if (!scorer) throw new Error('unknown intent: ' + intent + '. Valid: ' + Object.keys(intentScorers).join(', '));

    const scored = [];
    for (const el of candidates) {{
        if (!isVisible(el)) continue;
        const tag = el.tagName.toLowerCase();
        const type = (el.getAttribute('type') || '').toLowerCase();
        const text = getText(el);
        const role = getRole(el);
        const aria = getAriaLabel(el);
        const {{ score, reasons }} = scorer(el, tag, type, text, role, aria);
        if (score <= 0) continue;
        const rect = el.getBoundingClientRect();
        scored.push({{
            score: Math.round(score * 1000) / 1000,
            selector: buildSelector(el),
            tag,
            type: type || null,
            role: role || null,
            name: aria || el.name || null,
            text: (el.textContent || '').trim().substring(0, 80) || null,
            reason: reasons.join(', '),
            bounds: {{ x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) }},
        }});
    }}

    scored.sort((a, b) => b.score - a.score);
    return {{
        intent,
        candidateCount: scored.length,
        candidates: scored.slice(0, 5),
        scope: scopeSel || 'document',
    }};
}})()"#
    )
}

/// Handle `find_best` — find the best-matching element for a semantic intent.
///
/// Params: { intent: string, scope?: string }
pub async fn handle_find_best(page: &Page, params: &Value) -> Result<Value, String> {
    let intent = params
        .get("intent")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: intent".to_string())?;
    let scope = params.get("scope").and_then(|v| v.as_str());

    debug!("find_best: intent={intent} scope={scope:?}");

    let js = intent_scoring_js(intent, scope);

    let result = timeout(JS_TIMEOUT, page.evaluate_expression(&js))
        .await
        .map_err(|_| format!("find_best timed out for intent: {intent}"))?
        .map_err(|e| format!("find_best failed: {e}"))?;

    let data = result.value().cloned().unwrap_or(json!({}));
    Ok(data)
}

/// Handle `act` — execute a semantic action: find the best candidate for the intent,
/// click it (or focus for search_field), settle, and capture state.
///
/// Params: { intent: string, scope?: string }
pub async fn handle_act(page: &Page, params: &Value) -> Result<Value, String> {
    let intent = params
        .get("intent")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: intent".to_string())?;
    let scope = params.get("scope").and_then(|v| v.as_str());

    debug!("act: intent={intent} scope={scope:?}");

    // Phase 1: Find the best candidate
    let js = intent_scoring_js(intent, scope);
    let result = timeout(JS_TIMEOUT, page.evaluate_expression(&js))
        .await
        .map_err(|_| format!("act: find_best timed out for intent: {intent}"))?
        .map_err(|e| format!("act: find_best failed: {e}"))?;

    let find_data = result.value().cloned().unwrap_or(json!({}));
    let candidates = find_data
        .get("candidates")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if candidates.is_empty() {
        return Err(format!(
            "act: no candidates found for intent '{intent}'"
        ));
    }

    let top = &candidates[0];
    let selector = top
        .get("selector")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "act: top candidate has no selector".to_string())?;
    let score = top.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);

    // Phase 2: Execute the action
    let action_performed;
    if intent == "search_field" {
        // Focus the search field instead of clicking
        let focus_js = format!(
            "(() => {{ const el = document.querySelector({sel}); if (!el) throw new Error('element not found'); el.focus(); return true; }})()",
            sel = serde_json::to_string(selector).unwrap()
        );
        timeout(JS_TIMEOUT, page.evaluate_expression(&focus_js))
            .await
            .map_err(|_| "act: focus timed out".to_string())?
            .map_err(|e| format!("act: focus failed: {e}"))?;
        action_performed = "focus";
    } else {
        // Click the element
        let click_params = json!({ "selector": selector });
        handle_click(page, &click_params).await?;
        action_performed = "click";
    }

    // Phase 3: Settle and capture
    let (state, settle) = settle_and_capture(page).await;

    Ok(json!({
        "intent": intent,
        "action": action_performed,
        "candidate": top,
        "score": score,
        "state": state,
        "settle": settle,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_scoring_js_includes_all_intents() {
        let js = intent_scoring_js("submit_form", None);
        assert!(js.contains("submit_form"));
        assert!(js.contains("close_dialog"));
        assert!(js.contains("primary_cta"));
        assert!(js.contains("search_field"));
        assert!(js.contains("next_step"));
        assert!(js.contains("dismiss"));
        assert!(js.contains("auth_action"));
        assert!(js.contains("back_navigation"));
    }

    #[test]
    fn intent_scoring_js_with_scope() {
        let js = intent_scoring_js("submit_form", Some("form.login"));
        assert!(js.contains("form.login"));
    }

    #[test]
    fn valid_intents() {
        let valid = [
            "submit_form",
            "close_dialog",
            "primary_cta",
            "search_field",
            "next_step",
            "dismiss",
            "auth_action",
            "back_navigation",
        ];
        for intent in &valid {
            let js = intent_scoring_js(intent, None);
            assert!(js.contains(intent));
        }
    }
}
