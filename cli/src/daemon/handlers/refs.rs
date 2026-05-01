//! Handlers for the versioned deterministic ref system:
//! snapshot, get-ref, click-ref, hover-ref, fill-ref.

use crate::daemon::capture::capture_compact_page_state;
use crate::daemon::inspection;
use crate::daemon::narration::events::ActionKind;
use crate::daemon::settle::{ensure_mutation_counter, settle_after_action};
use crate::daemon::state::DaemonState;
use chromiumoxide::Page;
use gsd_browser_common::types::SettleOptions;
use serde_json::{json, Value};
use std::collections::HashMap;

fn ref_action_settle_opts() -> SettleOptions {
    SettleOptions {
        timeout_ms: 1500,
        check_focus_stability: true,
        ..SettleOptions::default()
    }
}

async fn settle_and_capture(page: &Page) -> (Value, Value) {
    ensure_mutation_counter(page).await;
    let settle = settle_after_action(page, &ref_action_settle_opts()).await;
    let state = capture_compact_page_state(page, false).await;
    (
        serde_json::to_value(&state).unwrap_or(json!({})),
        serde_json::to_value(&settle).unwrap_or(json!({})),
    )
}

fn parse_ref(ref_str: &str) -> Result<(u64, String), String> {
    let s = ref_str.trim();
    if !s.starts_with('@') {
        return Err(format!("invalid ref format (must start with @): {s}"));
    }
    let rest = &s[1..];
    let parts: Vec<&str> = rest.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(format!("invalid ref format (expected @vN:eM): {s}"));
    }
    let version_str = parts[0];
    let key = parts[1];
    if !version_str.starts_with('v') {
        return Err(format!(
            "invalid ref format (version must start with v): {s}"
        ));
    }
    let version: u64 = version_str[1..]
        .parse()
        .map_err(|_| format!("invalid version number in ref: {s}"))?;
    if key.is_empty() {
        return Err(format!("invalid ref format (empty element key): {s}"));
    }
    Ok((version, key.to_string()))
}

fn lookup_ref_node(state: &DaemonState, ref_str: &str) -> Result<(u64, String, Value), String> {
    let (version, key) = parse_ref(ref_str)?;
    let store = state.refs.lock().unwrap();

    if store.version == 0 {
        return Err("no snapshot taken yet — run snapshot first".to_string());
    }
    if version != store.version {
        return Err(format!(
            "ref version mismatch: ref is v{version} but current snapshot is v{}",
            store.version
        ));
    }

    let node = store
        .refs
        .get(&key)
        .cloned()
        .ok_or_else(|| format!("ref {ref_str} not found in snapshot v{version}"))?;
    Ok((version, key, node))
}

async fn resolve_ref(
    page: &Page,
    state: &DaemonState,
    ref_str: &str,
) -> Result<(Value, Value), String> {
    let (_version, _key, node) = lookup_ref_node(state, ref_str)?;
    let resolution = inspection::resolve_snapshot_node(page, &node).await?;
    let ok = resolution
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !ok {
        let reason = resolution
            .get("reason")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        return Err(format!("ref {ref_str} could not be resolved: {reason}"));
    }
    Ok((node, resolution))
}

fn ref_resolution_json(ref_str: &str, node: &Value, resolution: &Value) -> Value {
    json!({
        "ref": ref_str,
        "resolved_selector": resolution.get("selector").cloned().unwrap_or(Value::String(String::new())),
        "tier": resolution.get("tier").cloned().unwrap_or(json!(0)),
        "tag": node.get("tag").cloned().unwrap_or(Value::Null),
        "role": node.get("role").cloned().unwrap_or(Value::Null),
        "name": node.get("name").cloned().unwrap_or(Value::Null),
        "frameLabel": resolution.get("frameLabel").cloned().unwrap_or_else(|| node.get("frameLabel").cloned().unwrap_or(Value::Null)),
        "frameUrl": resolution.get("frameUrl").cloned().unwrap_or_else(|| node.get("frameUrl").cloned().unwrap_or(Value::Null)),
        "boundaries": resolution.get("boundaries").cloned().unwrap_or(json!([])),
    })
}

async fn ref_probe(
    page: &Page,
    state: &DaemonState,
    action: ActionKind,
    ref_str: &str,
    resolution: &Value,
    hint: Option<&str>,
) -> crate::daemon::narration::ActionProbe {
    let selector = resolution.get("selector").and_then(|value| value.as_str());
    let mut probe = state
        .narrator
        .probe_action(page, action, selector, hint.or(Some(ref_str)))
        .await;
    if let Some(target) = &mut probe.target {
        target.ref_id = Some(ref_str.to_string());
    }
    probe
}

pub async fn handle_snapshot(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let selector = params.get("selector").and_then(|value| value.as_str());
    let interactive_only = params
        .get("interactive_only")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let limit = params
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(40) as u32;
    let mode = params.get("mode").and_then(|value| value.as_str());

    let snapshot =
        inspection::snapshot_elements(page, state, selector, interactive_only, limit, mode).await?;
    let ok = snapshot
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !ok {
        let reason = snapshot
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("snapshot failed");
        return Err(format!("snapshot: {reason}"));
    }

    let nodes = snapshot
        .get("nodes")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let mut refs: HashMap<String, Value> = HashMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        refs.insert(format!("e{}", idx + 1), node.clone());
    }

    let (version, count) = {
        let mut store = state.refs.lock().unwrap();
        store.version += 1;
        store.refs = refs
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        store.metadata = json!({
            "interactive_only": interactive_only,
            "limit": limit,
            "mode": mode,
            "selector": selector,
            "boundaries": snapshot.get("boundaries").cloned().unwrap_or(json!([])),
        });
        (store.version, store.refs.len())
    };

    let refs_json: Value = refs
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<serde_json::Map<String, Value>>()
        .into();

    Ok(json!({
        "version": version,
        "count": count,
        "refs": refs_json,
        "metadata": {
            "interactive_only": interactive_only,
            "limit": limit,
            "mode": mode,
            "selector": selector,
        },
        "boundaries": snapshot.get("boundaries").cloned().unwrap_or(json!([])),
        "truncated": snapshot.get("truncated").cloned().unwrap_or(json!(false)),
    }))
}

pub fn handle_get_ref(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let ref_str = params
        .get("ref")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "missing required parameter: ref".to_string())?;

    let (version, key, node) = lookup_ref_node(state, ref_str)?;

    Ok(json!({
        "ref": ref_str,
        "version": version,
        "key": key,
        "node": node,
        "tag": node.get("tag").and_then(|value| value.as_str()).unwrap_or(""),
        "role": node.get("role").and_then(|value| value.as_str()).unwrap_or(""),
        "name": node.get("name").and_then(|value| value.as_str()).unwrap_or(""),
        "visible": node.get("visible").and_then(|value| value.as_bool()).unwrap_or(false),
        "enabled": node.get("enabled").and_then(|value| value.as_bool()).unwrap_or(false),
        "frameLabel": node.get("frameLabel").cloned().unwrap_or(Value::Null),
        "frameUrl": node.get("frameUrl").cloned().unwrap_or(Value::Null),
    }))
}

pub async fn handle_click_ref(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let ref_str = params
        .get("ref")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "missing required parameter: ref".to_string())?;

    let (node, resolution) = resolve_ref(page, state, ref_str).await?;
    let probe = ref_probe(page, state, ActionKind::Click, ref_str, &resolution, Some(ref_str)).await;
    state
        .narrator
        .emit_pre(&probe)
        .await
        .map_err(|_| "aborted".to_string())?;
    state.narrator.sleep_lead(&probe).await;

    let result = async {
        let action = inspection::act_on_snapshot_node(page, &node, "click", &json!({})).await?;
        let ok = action
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !ok {
            let reason = action
                .get("reason")
                .and_then(|value| value.as_str())
                .unwrap_or("click failed");
            return Err(format!("ref {ref_str} click failed: {reason}"));
        }

        let (state_json, settle) = settle_and_capture(page).await;
        Ok(json!({
            "state": state_json,
            "settle": settle,
            "clicked": {
                "ref": ref_str,
            },
            "ref_resolution": ref_resolution_json(ref_str, &node, &resolution),
        }))
    }
    .await;
    state.narrator.emit_post(&probe, &result).await;
    result
}

pub async fn handle_hover_ref(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let ref_str = params
        .get("ref")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "missing required parameter: ref".to_string())?;

    let (node, resolution) = resolve_ref(page, state, ref_str).await?;
    let probe = ref_probe(page, state, ActionKind::Hover, ref_str, &resolution, Some(ref_str)).await;
    state
        .narrator
        .emit_pre(&probe)
        .await
        .map_err(|_| "aborted".to_string())?;
    state.narrator.sleep_lead(&probe).await;

    let result = async {
        let action = inspection::act_on_snapshot_node(page, &node, "hover", &json!({})).await?;
        let ok = action
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !ok {
            let reason = action
                .get("reason")
                .and_then(|value| value.as_str())
                .unwrap_or("hover failed");
            return Err(format!("ref {ref_str} hover failed: {reason}"));
        }

        let (state_json, settle) = settle_and_capture(page).await;
        Ok(json!({
            "state": state_json,
            "settle": settle,
            "hovered": {
                "ref": ref_str,
            },
            "ref_resolution": ref_resolution_json(ref_str, &node, &resolution),
        }))
    }
    .await;
    state.narrator.emit_post(&probe, &result).await;
    result
}

pub async fn handle_fill_ref(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let ref_str = params
        .get("ref")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "missing required parameter: ref".to_string())?;
    let text = params
        .get("text")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "missing required parameter: text".to_string())?;
    let slowly = params
        .get("slowly")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let clear_first = params
        .get("clear_first")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let submit = params
        .get("submit")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let (node, resolution) = resolve_ref(page, state, ref_str).await?;
    let probe = ref_probe(page, state, ActionKind::Type, ref_str, &resolution, Some(text)).await;
    state
        .narrator
        .emit_pre(&probe)
        .await
        .map_err(|_| "aborted".to_string())?;
    state.narrator.sleep_lead(&probe).await;

    let result = async {
        let action = inspection::act_on_snapshot_node(
            page,
            &node,
            "fill",
            &json!({
                "text": text,
                "slowly": slowly,
                "clearFirst": clear_first,
                "submit": submit,
            }),
        )
        .await?;
        let ok = action
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !ok {
            let reason = action
                .get("reason")
                .and_then(|value| value.as_str())
                .unwrap_or("fill failed");
            return Err(format!("ref {ref_str} fill failed: {reason}"));
        }

        let (state_json, settle) = settle_and_capture(page).await;
        Ok(json!({
            "state": state_json,
            "settle": settle,
            "typed": {
                "ref": ref_str,
                "text_length": text.len(),
                "slowly": slowly,
                "submitted": submit,
            },
            "ref_resolution": ref_resolution_json(ref_str, &node, &resolution),
        }))
    }
    .await;
    state.narrator.emit_post(&probe, &result).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ref_valid() {
        let (version, key) = parse_ref("@v1:e1").unwrap();
        assert_eq!(version, 1);
        assert_eq!(key, "e1");

        let (version, key) = parse_ref("@v42:e123").unwrap();
        assert_eq!(version, 42);
        assert_eq!(key, "e123");
    }

    #[test]
    fn parse_ref_invalid() {
        assert!(parse_ref("v1:e1").is_err());
        assert!(parse_ref("@v1").is_err());
        assert!(parse_ref("@1:e1").is_err());
        assert!(parse_ref("@vx:e1").is_err());
        assert!(parse_ref("@v1:").is_err());
    }

    #[test]
    fn parse_ref_whitespace_handling() {
        let (version, key) = parse_ref("  @v3:e7  ").unwrap();
        assert_eq!(version, 3);
        assert_eq!(key, "e7");
    }
}
