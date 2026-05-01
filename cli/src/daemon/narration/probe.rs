use crate::daemon::narration::events::{ActionKind, BoundingBox, TargetInfo};
use chromiumoxide::Page;
use serde_json::Value;

/// Build the probe JS expression. Returns a JSON string or "null".
pub fn build_probe_js(selector: &str, auto_scroll: bool) -> String {
    let auto = if auto_scroll { "true" } else { "false" };
    let sel_lit = serde_json::to_string(selector).unwrap_or_else(|_| "null".into());
    format!(
        r#"JSON.stringify((() => {{
            const el = document.querySelector({sel});
            if (!el) return null;
            let r = el.getBoundingClientRect();
            const offscreen = r.bottom < 0 || r.top > window.innerHeight || r.right < 0 || r.left > window.innerWidth;
            let scrolled = false;
            if ({auto} && offscreen) {{
                el.scrollIntoView({{ block: 'center', inline: 'center', behavior: 'instant' }});
                scrolled = true;
                r = el.getBoundingClientRect();
            }}
            const rects = Array.from(el.getClientRects()).filter(q => q.width > 1 && q.height > 1);
            let aim;
            if (rects.length > 0) {{
                const q = rects[0];
                aim = {{ x: q.x, y: q.y, w: q.width, h: q.height }};
            }} else {{
                aim = {{ x: r.x, y: r.y, w: r.width, h: r.height }};
            }}
            return {{ x: r.x, y: r.y, w: r.width, h: r.height, aim, scrolled }};
        }})())"#,
        sel = sel_lit,
        auto = auto,
    )
}

/// Run the probe against the page; returns None if the element is not found.
pub async fn run_probe(page: &Page, selector: &str, auto_scroll: bool) -> Option<TargetInfo> {
    let expr = build_probe_js(selector, auto_scroll);
    let result = page.evaluate_expression(&expr).await.ok()?;
    let value: Value = result.into_value().ok()?;
    let s = value.as_str()?;
    if s == "null" {
        return None;
    }
    let parsed: Value = serde_json::from_str(s).ok()?;
    let obj = parsed.as_object()?;
    let bbox = BoundingBox {
        x: obj.get("x")?.as_f64()?,
        y: obj.get("y")?.as_f64()?,
        w: obj.get("w")?.as_f64()?,
        h: obj.get("h")?.as_f64()?,
    };
    let aim_obj = obj.get("aim")?.as_object()?;
    let aim = BoundingBox {
        x: aim_obj.get("x")?.as_f64()?,
        y: aim_obj.get("y")?.as_f64()?,
        w: aim_obj.get("w")?.as_f64()?,
        h: aim_obj.get("h")?.as_f64()?,
    };
    let scrolled = obj
        .get("scrolled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    Some(TargetInfo {
        selector: Some(selector.to_string()),
        ref_id: None,
        bbox: Some(bbox),
        aim: Some(aim),
        scrolled,
    })
}

/// Build a human-readable label for the action.
pub fn label_for(action: ActionKind, target: Option<&TargetInfo>, hint: Option<&str>) -> String {
    let target_desc = target.and_then(|t| t.selector.as_deref()).unwrap_or("");
    match action {
        ActionKind::Click => format!("clicking {}", hint.unwrap_or(target_desc))
            .trim()
            .to_string(),
        ActionKind::Hover => format!("hovering {}", hint.unwrap_or(target_desc))
            .trim()
            .to_string(),
        ActionKind::Type => {
            let t = hint.unwrap_or("");
            let display = if t.len() > 30 {
                format!("{}...", &t[..27])
            } else {
                t.to_string()
            };
            format!("typing \"{}\"", display)
        }
        ActionKind::Press => format!("pressing {}", hint.unwrap_or(""))
            .trim()
            .to_string(),
        ActionKind::Scroll => format!("scrolling {}", hint.unwrap_or(""))
            .trim()
            .to_string(),
        ActionKind::Navigate => format!("navigating to {}", hint.unwrap_or(""))
            .trim()
            .to_string(),
        ActionKind::SelectOption => format!("selecting option {}", hint.unwrap_or(""))
            .trim()
            .to_string(),
        ActionKind::SetChecked => format!("setting checkbox {}", target_desc)
            .trim()
            .to_string(),
        ActionKind::Drag => format!("dragging {}", target_desc).trim().to_string(),
        ActionKind::UploadFile => format!("uploading to {}", target_desc).trim().to_string(),
        ActionKind::Act => format!("act: {}", hint.unwrap_or("")).trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_js_includes_selector_literal() {
        let js = build_probe_js("button.foo", true);
        assert!(js.contains("\"button.foo\""));
        assert!(js.contains("scrollIntoView"));
    }

    #[test]
    fn build_js_disables_scroll_when_requested() {
        let js = build_probe_js("h1", false);
        assert!(js.contains("if (false && offscreen)"));
    }

    #[test]
    fn label_truncates_long_typed_text() {
        let long = "a".repeat(50);
        let lbl = label_for(ActionKind::Type, None, Some(long.as_str()));
        assert!(lbl.contains("..."));
        assert!(lbl.len() < 60);
    }

    #[test]
    fn label_navigate_uses_url_hint() {
        let lbl = label_for(ActionKind::Navigate, None, Some("https://example.com"));
        assert_eq!(lbl, "navigating to https://example.com");
    }
}
