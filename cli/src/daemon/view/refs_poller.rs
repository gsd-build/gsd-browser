use chromiumoxide::Page;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

const REFS_INTERVAL: Duration = Duration::from_millis(1500);
const REFS_JS: &str = r#"JSON.stringify((() => {
  const sel = 'a[href], button, input, select, textarea, [role="button"], [role="link"], [role="checkbox"], [role="textbox"], [role="searchbox"], [contenteditable="true"]';
  const out = [];
  let i = 0;
  for (const el of document.querySelectorAll(sel)) {
    if (i >= 80) break;
    const r = el.getBoundingClientRect();
    if (r.width < 4 || r.height < 4) continue;
    if (r.bottom < 0 || r.top > window.innerHeight) continue;
    if (r.right < 0 || r.left > window.innerWidth) continue;
    const style = window.getComputedStyle(el);
    if (style.visibility === 'hidden' || style.display === 'none') continue;
    const role = el.getAttribute('role') || el.tagName.toLowerCase();
    const name = (el.getAttribute('aria-label') || el.innerText || el.value || el.getAttribute('placeholder') || '').trim().slice(0, 80);
    out.push({ key: 'e' + (++i), role, name, x: r.x, y: r.y, w: r.width, h: r.height });
  }
  return out;
})())"#;

#[derive(Clone, Serialize)]
pub struct RefsMessage {
    #[serde(rename = "type")]
    ty: &'static str,
    pub refs: Vec<RefInfo>,
    pub timestamp: u64,
}

#[derive(Clone, Serialize)]
pub struct RefInfo {
    pub key: String,
    pub role: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub async fn run_refs_loop(page: Arc<Page>, tx: broadcast::Sender<RefsMessage>) {
    let mut ticker = tokio::time::interval(REFS_INTERVAL);
    loop {
        ticker.tick().await;
        if tx.receiver_count() == 0 {
            continue;
        }

        let result = match page.evaluate_expression(REFS_JS).await {
            Ok(result) => result,
            Err(_) => continue,
        };
        let value: Value = match result.into_value() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(s) = value.as_str() else {
            continue;
        };
        let parsed: Value = match serde_json::from_str(s) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(arr) = parsed.as_array() else {
            continue;
        };
        let refs: Vec<RefInfo> = arr
            .iter()
            .filter_map(|item| {
                let o = item.as_object()?;
                Some(RefInfo {
                    key: o.get("key")?.as_str()?.to_string(),
                    role: o.get("role")?.as_str()?.to_string(),
                    name: o
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string(),
                    x: o.get("x")?.as_f64()?,
                    y: o.get("y")?.as_f64()?,
                    w: o.get("w")?.as_f64()?,
                    h: o.get("h")?.as_f64()?,
                })
            })
            .collect();

        let _ = tx.send(RefsMessage {
            ty: "refs",
            refs,
            timestamp: crate::daemon::narration::events::now_ms(),
        });
    }
}
