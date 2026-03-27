//! Compact page state capture via `page.evaluate()`.
//!
//! Ported from the reference `capture.js` `captureCompactPageState` function.
//! Executes a single JS expression in the page context and deserializes the
//! result into `CompactPageState`.

use browser_tools_common::types::CompactPageState;
use chromiumoxide::Page;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::{debug, warn};

/// Maximum timeout for the capture evaluate call.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

/// JS expression that captures compact page state in a single evaluate call.
/// Returns a JSON-serializable object matching `CompactPageState`.
const CAPTURE_PAGE_STATE_JS: &str = r#"((includeBodyText) => {
    const focused = document.activeElement;
    const focusedDesc = focused && focused !== document.body && focused !== document.documentElement
        ? `${focused.tagName.toLowerCase()}${focused.id ? '#' + focused.id : ''}${focused.getAttribute('aria-label') ? ' "' + focused.getAttribute('aria-label') + '"' : ''}`
        : "";

    const headings = Array.from(document.querySelectorAll('h1,h2,h3'))
        .slice(0, 5)
        .map(h => (h.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 80));

    const dialog = document.querySelector('[role="dialog"]:not([hidden]),dialog[open]');
    const dialogTitle = dialog?.querySelector('[role="heading"],[aria-label]')?.textContent?.trim().slice(0, 80) ?? "";

    const bodyText = includeBodyText
        ? (document.body?.innerText || document.body?.textContent || "").trim().replace(/\s+/g, ' ').slice(0, 4000)
        : "";

    return {
        url: window.location.href,
        title: document.title,
        focus: focusedDesc,
        headings,
        bodyText,
        counts: {
            landmarks: document.querySelectorAll('[role="main"],[role="banner"],[role="navigation"],[role="contentinfo"],[role="complementary"],[role="search"],[role="form"],[role="dialog"],[role="alert"],main,header,nav,footer,aside,section,form,dialog').length,
            buttons: document.querySelectorAll('button,[role="button"]').length,
            links: document.querySelectorAll('a[href]').length,
            inputs: document.querySelectorAll('input,textarea,select').length,
        },
        dialog: {
            count: document.querySelectorAll('[role="dialog"]:not([hidden]),dialog[open]').length,
            title: dialogTitle,
        },
    };
})"#;

/// Captures a compact snapshot of the current page state.
///
/// Executes JS in the page context to gather title, URL, focus state, headings,
/// element counts, dialog state, and optional body text. Returns defaults on
/// error or timeout rather than failing.
pub async fn capture_compact_page_state(
    page: &Page,
    include_body_text: bool,
) -> CompactPageState {
    let started_at = Instant::now();
    let arg = if include_body_text { "true" } else { "false" };
    let js = format!("{CAPTURE_PAGE_STATE_JS}({arg})");

    let result = match timeout(CAPTURE_TIMEOUT, page.evaluate_expression(&js)).await {
        Ok(Ok(eval_result)) => match eval_result.into_value::<CompactPageState>() {
            Ok(state) => {
                let elapsed = started_at.elapsed().as_millis();
                debug!("capture_compact_page_state: ok in {elapsed}ms");
                state
            }
            Err(e) => {
                warn!("capture_compact_page_state: deserialize error: {e}");
                CompactPageState::default()
            }
        },
        Ok(Err(e)) => {
            warn!("capture_compact_page_state: evaluate error: {e}");
            CompactPageState::default()
        }
        Err(_) => {
            warn!("capture_compact_page_state: timed out (10s)");
            CompactPageState::default()
        }
    };

    // Override URL and title from page-level API when available (frame-safe)
    let mut state = result;
    if let Ok(Some(url)) = page.url().await {
        state.url = url;
    }
    if let Ok(title) = page.evaluate_expression("document.title").await {
        if let Ok(t) = title.into_value::<String>() {
            state.title = t;
        }
    }

    state
}
