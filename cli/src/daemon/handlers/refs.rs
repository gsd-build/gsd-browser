//! Handlers for the versioned deterministic ref system:
//! snapshot, get-ref, click-ref, hover-ref, fill-ref, with 4-tier resolution.

use crate::daemon::state::DaemonState;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
use tracing::debug;

/// Timeout for JS evaluation calls.
const EVAL_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// JS IIFE: Snapshot — walks DOM and builds deterministic RefNode objects
// ---------------------------------------------------------------------------
const SNAPSHOT_JS: &str = r##"(function(opts) {
  var pi = window.__pi;
  if (!pi) return JSON.stringify({error: "window.__pi not injected"});

  var scope = opts.selector ? document.querySelector(opts.selector) : document.body;
  if (!scope) return JSON.stringify({error: "scope selector not found: " + opts.selector});

  var interactiveOnly = opts.interactiveOnly !== false;
  var limit = opts.limit || 40;
  var mode = opts.mode || null;

  var allEls = scope.querySelectorAll("*");
  var results = [];

  for (var i = 0; i < allEls.length && results.length < limit; i++) {
    var el = allEls[i];

    // Skip invisible elements unless mode is not visible_only (i.e. we only enforce visible_only when mode says so)
    var visible = pi.isVisible(el);

    // Mode-based filtering
    if (mode === "visible_only" && !visible) continue;

    var tag = el.tagName.toLowerCase();
    var role = pi.inferRole(el);
    var isInteractive = pi.isInteractiveEl(el);

    if (mode === "interactive" || (!mode && interactiveOnly)) {
      if (!isInteractive) continue;
    } else if (mode === "form") {
      if (["input","select","textarea","button","label","fieldset","legend","output","datalist"].indexOf(tag) === -1) continue;
    } else if (mode === "dialog") {
      if (tag !== "dialog" && role !== "dialog" && role !== "alertdialog" &&
          el.getAttribute("aria-modal") !== "true") continue;
    } else if (mode === "navigation") {
      if (role !== "link" && role !== "navigation" && tag !== "nav" && tag !== "a") continue;
    } else if (mode === "errors") {
      if (role !== "alert" && role !== "status" && !el.classList.contains("error") &&
          !el.classList.contains("alert") && el.getAttribute("aria-live") === null) continue;
    } else if (mode === "headings") {
      if (!/^h[1-6]$/.test(tag) && role !== "heading") continue;
    }

    // Skip non-visible interactive elements unless explicitly in a non-visible mode
    if (!mode && !visible) continue;

    var name = pi.accessibleName(el);
    var domPathArr = pi.domPath(el);
    var hints = pi.selectorHints(el);
    var enabled = pi.isEnabled(el);

    // Content hash: hash of trimmed text content + tag
    var textContent = (el.textContent || "").trim().slice(0, 200);
    var contentHash = pi.simpleHash(tag + ":" + textContent);

    // Structural signature: tag + childElementCount + attribute count
    var structSig = tag + ":" + el.childElementCount + ":" + el.attributes.length;

    // Nearest heading (walk up and siblings)
    var nearestHeading = "";
    var walker = el.previousElementSibling;
    while (walker && !nearestHeading) {
      if (/^H[1-6]$/.test(walker.tagName)) nearestHeading = (walker.textContent || "").trim().slice(0, 60);
      walker = walker.previousElementSibling;
    }
    if (!nearestHeading) {
      var parent = el.parentElement;
      while (parent && parent !== document.body && !nearestHeading) {
        var headingChild = parent.querySelector("h1,h2,h3,h4,h5,h6");
        if (headingChild) nearestHeading = (headingChild.textContent || "").trim().slice(0, 60);
        parent = parent.parentElement;
      }
    }

    // Form ownership
    var formOwnership = "";
    var form = el.closest("form");
    if (form) {
      var formId = form.id || "";
      var formName = form.getAttribute("name") || "";
      var formAction = form.getAttribute("action") || "";
      formOwnership = formId || formName || formAction || "anonymous-form";
    }

    results.push({
      tag: tag,
      role: role,
      name: name,
      selectorHints: hints,
      visible: visible,
      enabled: enabled,
      domPath: domPathArr,
      contentHash: contentHash,
      structuralSignature: structSig,
      nearestHeading: nearestHeading,
      formOwnership: formOwnership
    });
  }

  return JSON.stringify(results);
})"##;

// ---------------------------------------------------------------------------
// JS IIFE: 4-tier resolution — resolves a RefNode back to a live DOM element
// ---------------------------------------------------------------------------
const RESOLVE_JS: &str = r##"(function(node) {
  var pi = window.__pi;
  if (!pi) return JSON.stringify({ok: false, reason: "window.__pi not injected"});

  // Tier 1: domPath — walk by child indices, verify tag match
  if (node.domPath && node.domPath.length > 0) {
    try {
      var el = document.documentElement;
      for (var i = 0; i < node.domPath.length; i++) {
        var idx = node.domPath[i];
        if (!el.children || idx >= el.children.length) { el = null; break; }
        el = el.children[idx];
      }
      if (el && el.tagName && el.tagName.toLowerCase() === node.tag) {
        return JSON.stringify({ok: true, selector: pi.cssPath(el), tier: 1});
      }
    } catch(e) {}
  }

  // Tier 2: selectorHints — try each, check for single match
  if (node.selectorHints && node.selectorHints.length > 0) {
    for (var h = 0; h < node.selectorHints.length; h++) {
      try {
        var matches = document.querySelectorAll(node.selectorHints[h]);
        if (matches.length === 1) {
          return JSON.stringify({ok: true, selector: node.selectorHints[h], tier: 2});
        }
      } catch(e) {}
    }
  }

  // Tier 3: role + name — find by role attribute or tag-based role, filter by accessible name
  if (node.role && node.name) {
    try {
      var candidates = [];
      // Try explicit role attribute
      var byRole = document.querySelectorAll("[role='" + node.role + "']");
      for (var r = 0; r < byRole.length; r++) candidates.push(byRole[r]);

      // Try tag-based matches
      var tagMap = {
        link: "a[href]", button: "button", textbox: "input,textarea",
        searchbox: "input[type=search]", combobox: "select",
        checkbox: "input[type=checkbox]", radio: "input[type=radio]"
      };
      if (tagMap[node.role]) {
        var byTag = document.querySelectorAll(tagMap[node.role]);
        for (var t = 0; t < byTag.length; t++) {
          if (candidates.indexOf(byTag[t]) === -1) candidates.push(byTag[t]);
        }
      }

      for (var c = 0; c < candidates.length; c++) {
        var candidateName = pi.accessibleName(candidates[c]);
        if (candidateName === node.name) {
          return JSON.stringify({ok: true, selector: pi.cssPath(candidates[c]), tier: 3});
        }
      }
    } catch(e) {}
  }

  // Tier 4: fingerprint — match by tag + contentHash or structuralSignature
  if (node.tag) {
    try {
      var byTag = document.querySelectorAll(node.tag);
      for (var f = 0; f < byTag.length; f++) {
        var el = byTag[f];
        var textContent = (el.textContent || "").trim().slice(0, 200);
        var hash = pi.simpleHash(node.tag + ":" + textContent);
        if (hash === node.contentHash) {
          return JSON.stringify({ok: true, selector: pi.cssPath(el), tier: 4});
        }
        var sig = node.tag + ":" + el.childElementCount + ":" + el.attributes.length;
        if (sig === node.structuralSignature && node.structuralSignature !== node.tag + ":0:0") {
          return JSON.stringify({ok: true, selector: pi.cssPath(el), tier: 4});
        }
      }
    } catch(e) {}
  }

  return JSON.stringify({ok: false, reason: "stale"});
})"##;

// ---------------------------------------------------------------------------
// Ref string parsing: @vN:eM → (version, key)
// ---------------------------------------------------------------------------
fn parse_ref(ref_str: &str) -> Result<(u64, String), String> {
    let s = ref_str.trim();
    if !s.starts_with('@') {
        return Err(format!("invalid ref format (must start with @): {s}"));
    }
    let rest = &s[1..]; // vN:eM
    let parts: Vec<&str> = rest.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(format!("invalid ref format (expected @vN:eM): {s}"));
    }
    let version_str = parts[0];
    let key = parts[1];
    if !version_str.starts_with('v') {
        return Err(format!("invalid ref format (version must start with v): {s}"));
    }
    let version: u64 = version_str[1..]
        .parse()
        .map_err(|_| format!("invalid version number in ref: {s}"))?;
    if key.is_empty() {
        return Err(format!("invalid ref format (empty element key): {s}"));
    }
    Ok((version, key.to_string()))
}

// ---------------------------------------------------------------------------
// handle_snapshot: evaluate snapshot JS, store in RefStore, return results
// ---------------------------------------------------------------------------
pub async fn handle_snapshot(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let selector = params.get("selector").and_then(|v| v.as_str());
    let interactive_only = params
        .get("interactive_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(40);
    let mode = params.get("mode").and_then(|v| v.as_str());

    // Build the options JSON to pass into the IIFE
    let mut opts = json!({
        "interactiveOnly": interactive_only,
        "limit": limit,
    });
    if let Some(sel) = selector {
        opts["selector"] = json!(sel);
    }
    if let Some(m) = mode {
        opts["mode"] = json!(m);
    }

    let js = format!(
        "({SNAPSHOT_JS})({opts})",
        SNAPSHOT_JS = SNAPSHOT_JS,
        opts = serde_json::to_string(&opts).unwrap()
    );

    debug!("snapshot: evaluating JS (interactive_only={interactive_only}, limit={limit}, mode={mode:?})");

    let result = timeout(EVAL_TIMEOUT, page.evaluate_expression(&js))
        .await
        .map_err(|_| "snapshot: JS evaluation timed out (10s)".to_string())?
        .map_err(|e| format!("snapshot: JS evaluation failed: {}", super::clean_cdp_error(&e)))?;

    let raw_str = result
        .value()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| "snapshot: JS returned non-string result".to_string())?;

    // Check for error response
    if let Ok(err_val) = serde_json::from_str::<Value>(&raw_str) {
        if let Some(err_msg) = err_val.get("error").and_then(|v| v.as_str()) {
            return Err(format!("snapshot: {err_msg}"));
        }
    }

    let nodes: Vec<Value> = serde_json::from_str(&raw_str)
        .map_err(|e| format!("snapshot: failed to parse JS result: {e}"))?;

    // Build refs map with deterministic keys e1, e2, ...
    let mut refs: HashMap<String, Value> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        let key = format!("e{}", i + 1);
        refs.insert(key, node.clone());
    }

    // Store in RefStore, incrementing version
    let (version, count) = {
        let mut store = state.refs.lock().unwrap();
        store.version += 1;
        store.refs = refs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        store.metadata = json!({
            "interactive_only": interactive_only,
            "limit": limit,
            "mode": mode,
            "selector": selector,
        });
        (store.version, store.refs.len())
    };

    debug!("snapshot: stored {count} refs at version {version}");

    // Build response with refs as a map
    let refs_json: Value = refs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
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
    }))
}

// ---------------------------------------------------------------------------
// handle_get_ref: parse ref string, validate version, return metadata
// ---------------------------------------------------------------------------
pub fn handle_get_ref(state: &DaemonState, params: &Value) -> Result<Value, String> {
    let ref_str = params
        .get("ref")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: ref".to_string())?;

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
        .ok_or_else(|| format!("ref {ref_str} not found in snapshot v{version}"))?;

    Ok(json!({
        "ref": ref_str,
        "version": version,
        "key": key,
        "node": node,
        "tag": node.get("tag").and_then(|v| v.as_str()).unwrap_or(""),
        "role": node.get("role").and_then(|v| v.as_str()).unwrap_or(""),
        "name": node.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        "visible": node.get("visible").and_then(|v| v.as_bool()).unwrap_or(false),
        "enabled": node.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
    }))
}

// ---------------------------------------------------------------------------
// resolve_ref: parse ref → look up → 4-tier JS resolution → CSS selector
// ---------------------------------------------------------------------------
async fn resolve_ref(
    page: &Page,
    state: &DaemonState,
    ref_str: &str,
) -> Result<(String, u64, Value), String> {
    let (version, key) = parse_ref(ref_str)?;

    let node = {
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

        store
            .refs
            .get(&key)
            .cloned()
            .ok_or_else(|| format!("ref {ref_str} not found in snapshot v{version}"))?
    };

    // Build JS call for 4-tier resolution
    let node_json = serde_json::to_string(&node).unwrap();
    let js = format!(
        "({RESOLVE_JS})({node_json})",
        RESOLVE_JS = RESOLVE_JS,
        node_json = node_json
    );

    let result = timeout(EVAL_TIMEOUT, page.evaluate_expression(&js))
        .await
        .map_err(|_| "resolve_ref: JS evaluation timed out".to_string())?
        .map_err(|e| format!("resolve_ref: JS evaluation failed: {}", super::clean_cdp_error(&e)))?;

    let raw_str = result
        .value()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| "resolve_ref: JS returned non-string result".to_string())?;

    let resolution: Value = serde_json::from_str(&raw_str)
        .map_err(|e| format!("resolve_ref: failed to parse resolution: {e}"))?;

    let ok = resolution
        .get("ok")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !ok {
        let reason = resolution
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(format!(
            "ref {ref_str} could not be resolved: {reason}"
        ));
    }

    let selector = resolution
        .get("selector")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "resolve_ref: resolution missing selector".to_string())?
        .to_string();

    let tier = resolution
        .get("tier")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    debug!("resolve_ref: {ref_str} → {selector} (tier {tier})");

    Ok((selector, tier, node))
}

// ---------------------------------------------------------------------------
// handle_click_ref: resolve → delegate to interaction::handle_click
// ---------------------------------------------------------------------------
pub async fn handle_click_ref(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let ref_str = params
        .get("ref")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: ref".to_string())?;

    let (selector, tier, node) = resolve_ref(page, state, ref_str).await?;

    let click_params = json!({"selector": selector});
    let mut result = super::interaction::handle_click(page, &click_params).await?;

    // Annotate result with ref resolution info
    if let Some(obj) = result.as_object_mut() {
        obj.insert(
            "ref_resolution".to_string(),
            json!({
                "ref": ref_str,
                "resolved_selector": selector,
                "tier": tier,
                "tag": node.get("tag"),
                "role": node.get("role"),
                "name": node.get("name"),
            }),
        );
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// handle_hover_ref: resolve → delegate to interaction::handle_hover
// ---------------------------------------------------------------------------
pub async fn handle_hover_ref(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let ref_str = params
        .get("ref")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: ref".to_string())?;

    let (selector, tier, node) = resolve_ref(page, state, ref_str).await?;

    let hover_params = json!({"selector": selector});
    let mut result = super::interaction::handle_hover(page, &hover_params).await?;

    if let Some(obj) = result.as_object_mut() {
        obj.insert(
            "ref_resolution".to_string(),
            json!({
                "ref": ref_str,
                "resolved_selector": selector,
                "tier": tier,
                "tag": node.get("tag"),
                "role": node.get("role"),
                "name": node.get("name"),
            }),
        );
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// handle_fill_ref: resolve → delegate to interaction::handle_type_text
// ---------------------------------------------------------------------------
pub async fn handle_fill_ref(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let ref_str = params
        .get("ref")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: ref".to_string())?;
    let text = params
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: text".to_string())?;

    let slowly = params.get("slowly").and_then(|v| v.as_bool()).unwrap_or(false);
    let clear_first = params.get("clear_first").and_then(|v| v.as_bool()).unwrap_or(false);
    let submit = params.get("submit").and_then(|v| v.as_bool()).unwrap_or(false);

    let (selector, tier, node) = resolve_ref(page, state, ref_str).await?;

    let type_params = json!({
        "selector": selector,
        "text": text,
        "slowly": slowly,
        "clear_first": clear_first,
        "submit": submit,
    });
    let mut result = super::interaction::handle_type_text(page, &type_params).await?;

    if let Some(obj) = result.as_object_mut() {
        obj.insert(
            "ref_resolution".to_string(),
            json!({
                "ref": ref_str,
                "resolved_selector": selector,
                "tier": tier,
                "tag": node.get("tag"),
                "role": node.get("role"),
                "name": node.get("name"),
            }),
        );
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ref_valid() {
        let (v, k) = parse_ref("@v1:e1").unwrap();
        assert_eq!(v, 1);
        assert_eq!(k, "e1");

        let (v, k) = parse_ref("@v42:e123").unwrap();
        assert_eq!(v, 42);
        assert_eq!(k, "e123");
    }

    #[test]
    fn parse_ref_invalid() {
        assert!(parse_ref("v1:e1").is_err()); // missing @
        assert!(parse_ref("@v1").is_err()); // missing :eM
        assert!(parse_ref("@1:e1").is_err()); // missing v prefix
        assert!(parse_ref("@vx:e1").is_err()); // non-numeric version
        assert!(parse_ref("@v1:").is_err()); // empty key
    }

    #[test]
    fn parse_ref_whitespace_handling() {
        let (v, k) = parse_ref("  @v3:e7  ").unwrap();
        assert_eq!(v, 3);
        assert_eq!(k, "e7");
    }
}
