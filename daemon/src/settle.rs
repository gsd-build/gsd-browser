//! Adaptive DOM settling via MutationObserver injection and poll loop.
//!
//! Ported from the reference `settle.js` implementation. The daemon orchestrates
//! JS evaluate calls into the Chrome page to install a MutationObserver, read
//! the mutation counter + focus state, and poll until the DOM is quiet.

use browser_tools_common::types::{SettleOptions, SettleResult};
use chromiumoxide::Page;
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};
use tracing::{debug, warn};

/// Threshold (ms) after which zero mutations triggers a shortened quiet window.
const ZERO_MUTATION_THRESHOLD_MS: u64 = 60;

/// Shortened quiet window when no mutations have been observed.
const ZERO_MUTATION_QUIET_MS: u64 = 30;

/// Maximum timeout for any single `page.evaluate()` call.
const EVALUATE_TIMEOUT: Duration = Duration::from_secs(25);

// ── JS snippets (evaluated in Chrome's V8) ──

const INSTALL_MUTATION_COUNTER_JS: &str = r#"(() => {
    const key = "__piMutationCounter";
    const installedKey = "__piMutationCounterInstalled";
    const w = window;
    if (typeof w[key] !== "number") w[key] = 0;
    if (w[installedKey]) return;
    const observer = new MutationObserver(() => {
        const current = typeof w[key] === "number" ? w[key] : 0;
        w[key] = current + 1;
    });
    observer.observe(document.documentElement || document.body, {
        subtree: true,
        childList: true,
        attributes: true,
        characterData: true,
    });
    w[installedKey] = true;
})()"#;

const READ_SETTLE_STATE_JS: &str = r#"((wantFocus) => {
    const w = window;
    const mutationCount = typeof w.__piMutationCounter === "number" ? w.__piMutationCounter : 0;
    if (!wantFocus) return { mutationCount, focusDescriptor: "" };
    const el = document.activeElement;
    if (!el || el === document.body || el === document.documentElement) {
        return { mutationCount, focusDescriptor: "" };
    }
    const id = el.id ? `#${el.id}` : "";
    const role = el.getAttribute("role") || "";
    const name = (el.getAttribute("aria-label") || el.getAttribute("name") || "").trim();
    return { mutationCount, focusDescriptor: `${el.tagName.toLowerCase()}${id}|${role}|${name}` };
})"#;

// ── Settle state (deserialized from JS) ──

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SettleState {
    #[serde(default)]
    mutation_count: u64,
    #[serde(default)]
    focus_descriptor: String,
}

// ── Public API ──

/// Installs the MutationObserver on `window.__piMutationCounter` if not already present.
pub async fn ensure_mutation_counter(page: &Page) {
    match timeout(EVALUATE_TIMEOUT, page.evaluate_expression(INSTALL_MUTATION_COUNTER_JS)).await {
        Ok(Ok(_)) => debug!("mutation counter installed"),
        Ok(Err(e)) => warn!("ensure_mutation_counter evaluate error: {e}"),
        Err(_) => warn!("ensure_mutation_counter timed out (25s)"),
    }
}

/// Reads the current mutation counter and optionally the focused element descriptor.
async fn read_settle_state(page: &Page, check_focus: bool) -> SettleState {
    let arg = if check_focus { "true" } else { "false" };
    let js = format!("{READ_SETTLE_STATE_JS}({arg})");

    match timeout(EVALUATE_TIMEOUT, page.evaluate_expression(&js)).await {
        Ok(Ok(result)) => result.into_value::<SettleState>().unwrap_or_default(),
        Ok(Err(e)) => {
            warn!("read_settle_state evaluate error: {e}");
            SettleState::default()
        }
        Err(_) => {
            warn!("read_settle_state timed out (25s)");
            SettleState::default()
        }
    }
}

/// Adaptive DOM settling — polls until DOM mutations quiet down.
///
/// Returns a `SettleResult` with the settle reason, duration, and poll count.
/// Settle reasons:
/// - `zero_mutation_shortcut`: No mutations observed; fast path taken
/// - `dom_quiet`: Mutations stopped within the quiet window
/// - `url_changed_then_quiet`: URL changed, then DOM went quiet
/// - `timeout_fallback`: Timed out waiting for quiet
/// - `evaluate_error`: JS evaluation failed
pub async fn settle_after_action(page: &Page, opts: &SettleOptions) -> SettleResult {
    let timeout_ms = opts.timeout_ms.max(150);
    let poll_ms = opts.poll_ms.clamp(20, 100);
    let base_quiet_window_ms = opts.quiet_window_ms.max(60);
    let check_focus = opts.check_focus_stability;

    let started_at = Instant::now();
    let mut polls: u32 = 0;
    let mut saw_url_change = false;
    let mut last_activity_at = started_at;
    let mut total_mutations_seen: u64 = 0;
    let mut active_quiet_window_ms = base_quiet_window_ms;

    // Install mutation counter
    ensure_mutation_counter(page).await;

    // Read initial state
    let initial = read_settle_state(page, check_focus).await;
    let mut previous_mutation_count = initial.mutation_count;
    let mut previous_focus = initial.focus_descriptor;

    // Get initial URL
    let mut previous_url = page.url().await.ok().flatten().unwrap_or_default();

    while started_at.elapsed().as_millis() < timeout_ms as u128 {
        sleep(Duration::from_millis(poll_ms)).await;
        polls += 1;
        let now = Instant::now();

        // Check URL change
        let current_url = page.url().await.ok().flatten().unwrap_or_default();
        if current_url != previous_url {
            saw_url_change = true;
            previous_url = current_url;
            last_activity_at = now;
        }

        // Read settle state
        let state = read_settle_state(page, check_focus).await;

        if state.mutation_count > previous_mutation_count {
            total_mutations_seen += state.mutation_count - previous_mutation_count;
            previous_mutation_count = state.mutation_count;
            last_activity_at = now;
        }

        if check_focus && state.focus_descriptor != previous_focus {
            previous_focus = state.focus_descriptor;
            last_activity_at = now;
        }

        // Zero-mutation short-circuit: after ZERO_MUTATION_THRESHOLD_MS with
        // no mutations observed at all, reduce the quiet window to settle faster.
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        if total_mutations_seen == 0
            && elapsed_ms >= ZERO_MUTATION_THRESHOLD_MS
            && active_quiet_window_ms != ZERO_MUTATION_QUIET_MS
        {
            active_quiet_window_ms = ZERO_MUTATION_QUIET_MS;
        }

        let quiet_elapsed = now.duration_since(last_activity_at).as_millis() as u64;
        if quiet_elapsed >= active_quiet_window_ms {
            let used_shortcut =
                active_quiet_window_ms == ZERO_MUTATION_QUIET_MS && total_mutations_seen == 0;
            let settle_ms = started_at.elapsed().as_millis() as u64;
            let reason = if used_shortcut {
                "zero_mutation_shortcut"
            } else if saw_url_change {
                "url_changed_then_quiet"
            } else {
                "dom_quiet"
            };

            debug!(
                "settle complete: reason={reason} ms={settle_ms} polls={polls}"
            );

            return SettleResult {
                settle_mode: "adaptive".into(),
                settle_ms,
                settle_reason: reason.into(),
                settle_polls: polls,
            };
        }
    }

    let settle_ms = started_at.elapsed().as_millis() as u64;
    debug!("settle timeout: ms={settle_ms} polls={polls}");

    SettleResult {
        settle_mode: "adaptive".into(),
        settle_ms,
        settle_reason: "timeout_fallback".into(),
        settle_polls: polls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settle_state_deserialize_defaults() {
        let empty: SettleState = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.mutation_count, 0);
        assert_eq!(empty.focus_descriptor, "");
    }

    #[test]
    fn settle_state_deserialize_full() {
        let json = r#"{"mutationCount": 42, "focusDescriptor": "input#q|textbox|search"}"#;
        let state: SettleState = serde_json::from_str(json).unwrap();
        assert_eq!(state.mutation_count, 42);
        assert_eq!(state.focus_descriptor, "input#q|textbox|search");
    }

    #[test]
    fn constants_match_reference() {
        assert_eq!(ZERO_MUTATION_THRESHOLD_MS, 60);
        assert_eq!(ZERO_MUTATION_QUIET_MS, 30);
    }
}
