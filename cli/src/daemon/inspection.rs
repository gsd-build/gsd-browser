use crate::daemon::handlers::clean_cdp_error;
use crate::daemon::state::DaemonState;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;

const INSPECTION_TIMEOUT: Duration = Duration::from_secs(30);

const INSPECTION_HELPERS_JS: &str = r##"
const pi = window.__pi || {};

function normalizeText(text) {
  return String(text || "").replace(/\s+/g, " ").trim();
}

function inferRole(el) {
  return pi.inferRole ? pi.inferRole(el) : "";
}

function accessibleName(el) {
  if (pi.accessibleName) return pi.accessibleName(el);
  return normalizeText(el.innerText || el.textContent || "").slice(0, 80);
}

function isVisible(el) {
  if (pi.isVisible) return pi.isVisible(el);
  const style = window.getComputedStyle(el);
  if (style.display === "none" || style.visibility === "hidden") return false;
  const rect = el.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0;
}

function isEnabled(el) {
  if (pi.isEnabled) return pi.isEnabled(el);
  return !el.disabled;
}

function selectorHint(el) {
  if (pi.selectorHints) {
    const hints = pi.selectorHints(el);
    if (hints && hints.length > 0) return hints[0];
  }
  return "";
}

function selectorHints(el) {
  if (pi.selectorHints) {
    const hints = pi.selectorHints(el);
    if (Array.isArray(hints)) return hints;
  }
  const hint = selectorHint(el);
  return hint ? [hint] : [];
}

function simpleHash(text) {
  if (pi.simpleHash) return pi.simpleHash(text);
  let hash = 0;
  const input = String(text || "");
  for (let i = 0; i < input.length; i++) {
    hash = ((hash << 5) - hash) + input.charCodeAt(i);
    hash |= 0;
  }
  return String(hash >>> 0);
}

function frameLabel(name, index) {
  return name || (index === 0 ? "main" : `frame-${index}`);
}

function collectFrameEntries() {
  const entries = [];
  const boundaries = [];

  function walk(win, depth) {
    if (depth > 10) return;
    let name = "";
    let url = "";
    let accessible = true;

    try {
      name = win.name || "";
    } catch (err) {}

    try {
      url = win.location.href || "";
      void win.document;
    } catch (err) {
      accessible = false;
      url = "(cross-origin)";
      boundaries.push({
        name: name || "",
        url,
        reason: "cross-origin frame",
      });
    }

    const index = entries.length;
    entries.push({
      win,
      name,
      url,
      index,
      accessible,
    });

    let childCount = 0;
    try {
      childCount = win.frames.length;
    } catch (err) {
      childCount = 0;
    }

    for (let i = 0; i < childCount; i++) {
      try {
        walk(win.frames[i], depth + 1);
      } catch (err) {}
    }
  }

  walk(window.top || window, 0);
  return { entries, boundaries };
}

function contextFromEntry(entry) {
  if (!entry || !entry.accessible) {
    return {
      ok: false,
      error: "selected frame is cross-origin or unavailable",
      crossOrigin: true,
    };
  }

  return {
    ok: true,
    context: {
      win: entry.win,
      doc: entry.win.document,
      label: frameLabel(entry.name, entry.index),
      url: entry.url || "",
      index: entry.index,
    },
  };
}

function resolveFrameContext(frameSpec) {
  const { entries, boundaries } = collectFrameEntries();

  if (!frameSpec || !frameSpec.kind) {
    const resolved = contextFromEntry(entries[0]);
    if (!resolved.ok) return { ...resolved, boundaries };
    return { ok: true, context: resolved.context, boundaries };
  }

  if (frameSpec.kind === "main") {
    const resolved = contextFromEntry(entries[0]);
    if (!resolved.ok) return { ...resolved, boundaries };
    return { ok: true, context: resolved.context, boundaries };
  }

  let match = null;
  if (frameSpec.kind === "index") {
    match = entries.find((entry) => entry.index === frameSpec.value);
  } else if (frameSpec.kind === "name") {
    match = entries.find((entry) => entry.name === frameSpec.value);
  } else if (frameSpec.kind === "urlPattern") {
    match = entries.find((entry) => (entry.url || "").includes(frameSpec.value));
  }

  if (!match) {
    return {
      ok: false,
      error: "selected frame was not found",
      boundaries,
    };
  }

  const resolved = contextFromEntry(match);
  if (!resolved.ok) return { ...resolved, boundaries };
  return { ok: true, context: resolved.context, boundaries };
}

function collectContexts(frameSpec, includeFrames) {
  if (frameSpec && frameSpec.kind) {
    const resolved = resolveFrameContext(frameSpec);
    if (!resolved.ok) return resolved;
    return { ok: true, contexts: [resolved.context], boundaries: resolved.boundaries };
  }

  const { entries, boundaries } = collectFrameEntries();
  const contexts = [];
  for (const entry of entries) {
    if (!entry.accessible) continue;
    if (!includeFrames && entry.index !== 0) continue;
    contexts.push({
      win: entry.win,
      doc: entry.win.document,
      label: frameLabel(entry.name, entry.index),
      url: entry.url || "",
      index: entry.index,
    });
  }
  return { ok: true, contexts, boundaries };
}

function collectQueryRoots(root) {
  const roots = [root];
  const tree = root.querySelectorAll ? root.querySelectorAll("*") : [];
  for (const el of tree) {
    if (el.shadowRoot) {
      roots.push(el.shadowRoot);
    }
  }
  return roots;
}

function queryAllDeep(root, selector) {
  const roots = collectQueryRoots(root);
  const results = [];
  const seen = new Set();
  for (const scope of roots) {
    let matches;
    try {
      matches = selector ? scope.querySelectorAll(selector) : scope.querySelectorAll("*");
    } catch (err) {
      return { ok: false, error: "invalid selector: " + err.message };
    }
    for (const el of matches) {
      if (seen.has(el)) continue;
      seen.add(el);
      results.push(el);
    }
  }
  return { ok: true, elements: results };
}

function deepDomPath(el) {
  const path = [];
  let current = el;
  const rootDoc = el && el.ownerDocument ? el.ownerDocument : document;

  while (current && current !== rootDoc.documentElement) {
    const parent = current.parentNode;
    if (!parent) break;

    if (parent instanceof ShadowRoot) {
      const idx = Array.from(parent.children || []).indexOf(current);
      if (idx < 0) return [];
      path.push({ kind: "child", index: idx });
      path.push({ kind: "shadow" });
      current = parent.host;
      continue;
    }

    const idx = Array.from(parent.children || []).indexOf(current);
    if (idx < 0) return [];
    path.push({ kind: "child", index: idx });
    current = parent;
  }

  return path.reverse();
}

function resolveDeepPath(doc, path) {
  let current = doc.documentElement;
  for (const segment of path || []) {
    if (segment.kind === "shadow") {
      if (!current || !current.shadowRoot) return null;
      current = current.shadowRoot;
      continue;
    }

    const children = current && current.children ? current.children : null;
    if (!children || segment.index < 0 || segment.index >= children.length) {
      return null;
    }
    current = children[segment.index];
  }

  return current;
}

function absoluteCenter(context, el) {
  const rect = el.getBoundingClientRect();
  let x = rect.left + rect.width / 2;
  let y = rect.top + rect.height / 2;
  let win = context.win;

  while (win && win !== win.top) {
    const frameEl = win.frameElement;
    if (!frameEl) break;
    const frameRect = frameEl.getBoundingClientRect();
    x += frameRect.left;
    y += frameRect.top;
    win = win.parent;
  }

  return { x, y };
}

function absoluteBounds(context, el) {
  const rect = el.getBoundingClientRect();
  let x = rect.left;
  let y = rect.top;
  let win = context.win;

  while (win && win !== win.top) {
    const frameEl = win.frameElement;
    if (!frameEl) break;
    const frameRect = frameEl.getBoundingClientRect();
    x += frameRect.left;
    y += frameRect.top;
    win = win.parent;
  }

  return { x, y, w: rect.width, h: rect.height };
}

function elementSummary(el, context) {
  const text = normalizeText(el.innerText || el.textContent || "");
  return {
    tag: el.tagName.toLowerCase(),
    role: inferRole(el),
    name: accessibleName(el),
    selector_hint: selectorHint(el),
    visible: isVisible(el),
    enabled: isEnabled(el),
    text: text.slice(0, 80),
    frameLabel: context.label,
    frameUrl: context.url,
    value: el.value !== undefined ? String(el.value) : null,
    checked: !!el.checked,
  };
}
"##;

fn selected_frame_value(state: &DaemonState) -> Value {
    let selected = state.selected_frame.lock().unwrap().clone();
    match selected.as_deref() {
        Some(value) if value.starts_with("name:") => {
            json!({"kind": "name", "value": value.trim_start_matches("name:")})
        }
        Some(value) if value.starts_with("index:") => {
            let parsed = value
                .trim_start_matches("index:")
                .parse::<u64>()
                .unwrap_or(0);
            json!({"kind": "index", "value": parsed})
        }
        Some(value) if value.starts_with("url:") => {
            json!({"kind": "urlPattern", "value": value.trim_start_matches("url:")})
        }
        Some(_) => json!({"kind": "main"}),
        None => Value::Null,
    }
}

fn build_js(state: &DaemonState, include_frames: bool, body: &str) -> String {
    let frame_spec = selected_frame_value(state);
    build_js_with_frame_spec(&frame_spec, include_frames, body)
}

fn build_js_with_frame_spec(frame_spec: &Value, include_frames: bool, body: &str) -> String {
    format!(
        r#"(() => {{
  const frameSpec = {frame_spec};
  const includeFrames = {include_frames};
  {helpers}
  {body}
}})()"#,
        frame_spec = serde_json::to_string(&frame_spec).unwrap(),
        include_frames = if include_frames { "true" } else { "false" },
        helpers = INSPECTION_HELPERS_JS,
        body = body,
    )
}

async fn eval_json_value(page: &Page, js: &str, label: &str) -> Result<Value, String> {
    let result = timeout(INSPECTION_TIMEOUT, page.evaluate_expression(js))
        .await
        .map_err(|_| format!("{label} timed out after 30s"))?
        .map_err(|err| format!("{label} error: {}", clean_cdp_error(&err)))?;
    let value = result.value().cloned().unwrap_or(Value::Null);
    let json_str = value
        .as_str()
        .ok_or_else(|| format!("{label} returned non-string result"))?;
    serde_json::from_str(json_str).map_err(|err| format!("{label} parse error: {err}"))
}

pub async fn eval_expression(
    page: &Page,
    state: &DaemonState,
    expression: &str,
) -> Result<Value, String> {
    let body = format!(
        r#"
  const resolved = resolveFrameContext(frameSpec);
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, error: resolved.error, boundaries: resolved.boundaries || [] }});
  }}
  try {{
    const value = resolved.context.win.eval({expression});
    return JSON.stringify({{ ok: true, value }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, error: String(err), boundaries: resolved.boundaries || [] }});
  }}
"#,
        expression = serde_json::to_string(expression).unwrap()
    );
    eval_json_value(page, &build_js(state, false, &body), "eval").await
}

pub async fn target_url(page: &Page, state: &DaemonState) -> Result<Value, String> {
    let body = r#"
  const resolved = resolveFrameContext(frameSpec);
  if (!resolved.ok) {
    return JSON.stringify({ ok: false, error: resolved.error, boundaries: resolved.boundaries || [] });
  }
  return JSON.stringify({
    ok: true,
    url: resolved.context.url,
    frameLabel: resolved.context.label,
    boundaries: resolved.boundaries || [],
  });
"#;
    eval_json_value(page, &build_js(state, false, body), "target_url").await
}

pub async fn selector_query(
    page: &Page,
    state: &DaemonState,
    selector: &str,
    include_frames: bool,
) -> Result<Value, String> {
    let body = format!(
        r#"
  const resolved = collectContexts(frameSpec, includeFrames || !!(frameSpec && frameSpec.kind));
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, error: resolved.error, boundaries: resolved.boundaries || [], crossOrigin: !!resolved.crossOrigin }});
  }}
  const matches = [];
  for (const context of resolved.contexts) {{
    const queried = queryAllDeep(context.doc, {selector});
    if (!queried.ok) {{
      return JSON.stringify({{ ok: false, error: queried.error, boundaries: resolved.boundaries || [] }});
    }}
    for (const el of queried.elements) {{
      matches.push({{ summary: elementSummary(el, context) }});
    }}
  }}
  return JSON.stringify({{
    ok: true,
    count: matches.length,
    first: matches.length > 0 ? matches[0].summary : null,
    matches: matches.map((m) => m.summary),
    boundaries: resolved.boundaries || [],
  }});
"#,
        selector = serde_json::to_string(selector).unwrap()
    );
    eval_json_value(
        page,
        &build_js(state, include_frames, &body),
        "selector_query",
    )
    .await
}

pub async fn resolve_selector_target(
    page: &Page,
    state: &DaemonState,
    selector: &str,
    prefer_visible: bool,
) -> Result<Value, String> {
    let body = format!(
        r#"
  const resolved = collectContexts(frameSpec, true);
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, error: resolved.error, boundaries: resolved.boundaries || [], crossOrigin: !!resolved.crossOrigin }});
  }}

  const matches = [];
  for (const context of resolved.contexts) {{
    const queried = queryAllDeep(context.doc, {selector});
    if (!queried.ok) {{
      return JSON.stringify({{ ok: false, error: queried.error, boundaries: resolved.boundaries || [] }});
    }}
    for (const el of queried.elements) {{
      matches.push({{ element: el, context, summary: elementSummary(el, context) }});
    }}
  }}

  if (matches.length === 0) {{
    return JSON.stringify({{ ok: false, error: "element not found: " + {selector}, boundaries: resolved.boundaries || [] }});
  }}

  let target = null;
  if ({prefer_visible}) {{
    target = matches.find((match) => match.summary.visible) || null;
  }}
  if (!target) target = matches[0];

  if (target.element.scrollIntoView) {{
    target.element.scrollIntoView({{ block: "center", inline: "center", behavior: "instant" }});
  }}

  const center = absoluteCenter(target.context, target.element);
  return JSON.stringify({{
    ok: true,
    count: matches.length,
    target: target.summary,
    center,
    boundaries: resolved.boundaries || [],
  }});
"#,
        selector = serde_json::to_string(selector).unwrap(),
        prefer_visible = if prefer_visible { "true" } else { "false" },
    );
    eval_json_value(
        page,
        &build_js(state, true, &body),
        "resolve_selector_target",
    )
    .await
}

pub async fn perform_selector_action(
    page: &Page,
    state: &DaemonState,
    selector: &str,
    action: &str,
    options: &Value,
    prefer_visible: bool,
) -> Result<Value, String> {
    let body = format!(
        r#"
  const resolved = collectContexts(frameSpec, true);
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, error: resolved.error, boundaries: resolved.boundaries || [], crossOrigin: !!resolved.crossOrigin }});
  }}

  const matches = [];
  for (const context of resolved.contexts) {{
    const queried = queryAllDeep(context.doc, {selector});
    if (!queried.ok) {{
      return JSON.stringify({{ ok: false, error: queried.error, boundaries: resolved.boundaries || [] }});
    }}
    for (const el of queried.elements) {{
      matches.push({{ element: el, context, summary: elementSummary(el, context) }});
    }}
  }}

  if (matches.length === 0) {{
    return JSON.stringify({{ ok: false, error: "element not found: " + {selector}, boundaries: resolved.boundaries || [] }});
  }}

  let target = null;
  if ({prefer_visible}) {{
    target = matches.find((match) => match.summary.visible) || null;
  }}
  if (!target) target = matches[0];

  const el = target.element;
  const context = target.context;
  const options = {options};

  if (el.scrollIntoView) {{
    el.scrollIntoView({{ block: "center", inline: "center", behavior: "instant" }});
  }}

  try {{
    switch ({action}) {{
      case "focus":
        if (typeof el.focus === "function") el.focus();
        break;
      case "click":
        if (typeof el.focus === "function") el.focus();
        if (typeof el.click === "function") {{
          el.click();
        }} else {{
          el.dispatchEvent(new MouseEvent("click", {{ bubbles: true, cancelable: true, view: context.win }}));
        }}
        break;
      case "hover": {{
        const hoverEvents = ["pointerover", "mouseover", "mouseenter", "pointermove", "mousemove"];
        for (const eventName of hoverEvents) {{
          el.dispatchEvent(new MouseEvent(eventName, {{ bubbles: true, cancelable: true, view: context.win }}));
        }}
        break;
      }}
      case "type": {{
        const text = String(options.text || "");
        const clearFirst = !!options.clearFirst;
        const slowly = !!options.slowly;
        const submit = !!options.submit;

        if (typeof el.focus === "function") el.focus();

        if ("value" in el) {{
          if (clearFirst) {{
            el.value = "";
            el.dispatchEvent(new Event("input", {{ bubbles: true }}));
          }}

          if (slowly) {{
            let nextValue = clearFirst ? "" : String(el.value || "");
            for (const ch of text) {{
              nextValue += ch;
              el.value = nextValue;
              el.dispatchEvent(new Event("input", {{ bubbles: true }}));
            }}
          }} else {{
            el.value = text;
            el.dispatchEvent(new Event("input", {{ bubbles: true }}));
          }}
          el.dispatchEvent(new Event("change", {{ bubbles: true }}));
        }} else if (el.isContentEditable) {{
          if (clearFirst) el.textContent = "";
          el.textContent = text;
          el.dispatchEvent(new Event("input", {{ bubbles: true }}));
        }} else {{
          throw new Error("element does not support text input");
        }}

        if (submit) {{
          el.dispatchEvent(new KeyboardEvent("keydown", {{ key: "Enter", bubbles: true }}));
          el.dispatchEvent(new KeyboardEvent("keyup", {{ key: "Enter", bubbles: true }}));
          const form = el.form || el.closest("form");
          if (form) {{
            if (typeof form.requestSubmit === "function") {{
              form.requestSubmit();
            }} else if (typeof form.submit === "function") {{
              form.submit();
            }}
          }}
        }}
        break;
      }}
      case "select_option": {{
        if (!el.tagName || el.tagName.toLowerCase() !== "select") {{
          throw new Error("element is not a <select>");
        }}
        const optionValue = String(options.option || "");
        const match = Array.from(el.options || []).find((option) =>
          option.label === optionValue ||
          option.value === optionValue ||
          normalizeText(option.textContent || "") === optionValue
        );
        if (!match) {{
          throw new Error("option not found: " + optionValue);
        }}
        el.value = match.value;
        el.dispatchEvent(new Event("change", {{ bubbles: true }}));
        el.dispatchEvent(new Event("input", {{ bubbles: true }}));
        break;
      }}
      case "set_checked": {{
        if (!("checked" in el)) {{
          throw new Error("element does not support checked state");
        }}
        el.checked = !!options.checked;
        el.dispatchEvent(new Event("change", {{ bubbles: true }}));
        el.dispatchEvent(new Event("input", {{ bubbles: true }}));
        break;
      }}
      default:
        throw new Error("unsupported selector action: " + {action});
    }}
  }} catch (err) {{
    return JSON.stringify({{
      ok: false,
      error: String(err),
      count: matches.length,
      target: elementSummary(el, context),
      boundaries: resolved.boundaries || [],
    }});
  }}

  return JSON.stringify({{
    ok: true,
    count: matches.length,
    target: elementSummary(el, context),
    center: absoluteCenter(context, el),
    boundaries: resolved.boundaries || [],
  }});
"#,
        selector = serde_json::to_string(selector).unwrap(),
        action = serde_json::to_string(action).unwrap(),
        options = serde_json::to_string(options).unwrap(),
        prefer_visible = if prefer_visible { "true" } else { "false" },
    );
    eval_json_value(
        page,
        &build_js(state, true, &body),
        "perform_selector_action",
    )
    .await
}

pub async fn text_query(
    page: &Page,
    state: &DaemonState,
    text: &str,
    include_frames: bool,
) -> Result<Value, String> {
    let body = format!(
        r#"
  const resolved = collectContexts(frameSpec, includeFrames || !!(frameSpec && frameSpec.kind));
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, error: resolved.error, boundaries: resolved.boundaries || [], crossOrigin: !!resolved.crossOrigin }});
  }}
  const search = {text}.toLowerCase();
  const matches = [];
  for (const context of resolved.contexts) {{
    const docText = normalizeText((context.doc.body && (context.doc.body.innerText || context.doc.body.textContent)) || (context.doc.documentElement && (context.doc.documentElement.innerText || context.doc.documentElement.textContent)) || "");
    if (!search || docText.toLowerCase().includes(search)) {{
      matches.push({{
        frameLabel: context.label,
        frameUrl: context.url,
        snippet: docText.slice(0, 160),
      }});
    }}
  }}
  return JSON.stringify({{
    ok: true,
    found: matches.length > 0,
    matches,
    boundaries: resolved.boundaries || [],
  }});
"#,
        text = serde_json::to_string(text).unwrap()
    );
    eval_json_value(page, &build_js(state, include_frames, &body), "text_query").await
}

pub async fn page_source(
    page: &Page,
    state: &DaemonState,
    selector: Option<&str>,
) -> Result<Value, String> {
    let body = if let Some(selector) = selector {
        format!(
            r#"
  const resolved = resolveFrameContext(frameSpec);
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, error: resolved.error, boundaries: resolved.boundaries || [] }});
  }}
  const queried = queryAllDeep(resolved.context.doc, {selector});
  if (!queried.ok) {{
    return JSON.stringify({{ ok: false, error: queried.error, boundaries: resolved.boundaries || [] }});
  }}
  const html = queried.elements.length > 0 ? queried.elements[0].outerHTML : "";
  return JSON.stringify({{
    ok: true,
    html,
    length: html.length,
    frameLabel: resolved.context.label,
    frameUrl: resolved.context.url,
    boundaries: resolved.boundaries || [],
  }});
"#,
            selector = serde_json::to_string(selector).unwrap()
        )
    } else {
        r#"
  const resolved = resolveFrameContext(frameSpec);
  if (!resolved.ok) {
    return JSON.stringify({ ok: false, error: resolved.error, boundaries: resolved.boundaries || [] });
  }
  const html = resolved.context.doc.documentElement
    ? resolved.context.doc.documentElement.outerHTML
    : "";
  return JSON.stringify({
    ok: true,
    html,
    length: html.length,
    frameLabel: resolved.context.label,
    frameUrl: resolved.context.url,
    boundaries: resolved.boundaries || [],
  });
"#
        .to_string()
    };
    eval_json_value(page, &build_js(state, false, &body), "page_source").await
}

pub async fn find_elements(
    page: &Page,
    state: &DaemonState,
    role: &str,
    text: &str,
    selector: &str,
    limit: u32,
) -> Result<Value, String> {
    let body = format!(
        r#"
  const resolved = collectContexts(frameSpec, true);
  if (!resolved.ok) {{
    return JSON.stringify({{ elements: [], count: 0, truncated: false, error: resolved.error, boundaries: resolved.boundaries || [] }});
  }}
  const role = {role}.trim();
  const searchText = {text}.trim().toLowerCase();
  const selector = {selector}.trim();
  const limit = {limit};
  const results = [];
  for (const context of resolved.contexts) {{
    const queried = queryAllDeep(context.doc, selector || null);
    if (!queried.ok) {{
      return JSON.stringify({{ elements: [], count: 0, truncated: false, error: queried.error, boundaries: resolved.boundaries || [] }});
    }}
    for (const el of queried.elements) {{
      if (results.length >= limit) break;
      const summary = elementSummary(el, context);
      if (role && summary.role !== role) continue;
      const haystack = normalizeText((el.innerText || el.textContent || "")).toLowerCase();
      if (searchText && !haystack.includes(searchText) && !summary.name.toLowerCase().includes(searchText)) continue;
      if (!selector && !summary.visible) continue;
      results.push(summary);
    }}
    if (results.length >= limit) break;
  }}
  return JSON.stringify({{
    elements: results,
    count: results.length,
    truncated: results.length >= limit,
    boundaries: resolved.boundaries || [],
  }});
"#,
        role = serde_json::to_string(role).unwrap(),
        text = serde_json::to_string(text).unwrap(),
        selector = serde_json::to_string(selector).unwrap(),
        limit = limit,
    );
    eval_json_value(page, &build_js(state, true, &body), "find").await
}

pub async fn snapshot_elements(
    page: &Page,
    state: &DaemonState,
    selector: Option<&str>,
    interactive_only: bool,
    limit: u32,
    mode: Option<&str>,
) -> Result<Value, String> {
    let body = format!(
        r#"
  const resolved = collectContexts(frameSpec, true);
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, error: resolved.error, boundaries: resolved.boundaries || [], crossOrigin: !!resolved.crossOrigin }});
  }}

  const scopeSelector = {selector};
  const interactiveOnly = {interactive_only};
  const limit = {limit};
  const mode = {mode};
  const results = [];
  const seen = new Set();
  let scopeFound = !scopeSelector;

  function nearestHeading(el) {{
    let heading = "";
    let walker = el.previousElementSibling;
    while (walker && !heading) {{
      if (/^H[1-6]$/.test(walker.tagName)) {{
        heading = normalizeText(walker.textContent || "").slice(0, 60);
      }}
      walker = walker.previousElementSibling;
    }}
    if (heading) return heading;

    let parent = el.parentElement;
    while (parent && parent !== parent.ownerDocument.body && !heading) {{
      const found = parent.querySelector("h1,h2,h3,h4,h5,h6");
      if (found) heading = normalizeText(found.textContent || "").slice(0, 60);
      parent = parent.parentElement;
    }}
    return heading;
  }}

  function formOwnership(el) {{
    const form = el.form || el.closest("form");
    if (!form) return "";
    return form.id || form.getAttribute("name") || form.getAttribute("action") || "anonymous-form";
  }}

  function includeElement(el, visible) {{
    const tag = el.tagName.toLowerCase();
    const role = inferRole(el);
    const interactive = pi.isInteractiveEl ? pi.isInteractiveEl(el) : ["a","button","input","select","textarea","summary"].includes(tag);

    if (mode === "visible_only") return visible;
    if (mode === "interactive" || (!mode && interactiveOnly)) return interactive && visible;
    if (mode === "form") return ["input","select","textarea","button","label","fieldset","legend","output","datalist"].includes(tag);
    if (mode === "dialog") return tag === "dialog" || role === "dialog" || role === "alertdialog" || el.getAttribute("aria-modal") === "true";
    if (mode === "navigation") return role === "link" || role === "navigation" || tag === "nav" || tag === "a";
    if (mode === "errors") return role === "alert" || role === "status" || el.classList.contains("error") || el.classList.contains("alert") || el.getAttribute("aria-live") !== null;
    if (mode === "headings") return /^h[1-6]$/.test(tag) || role === "heading";
    return visible;
  }}

  for (const context of resolved.contexts) {{
    let scopes = [context.doc.body || context.doc.documentElement];
    if (scopeSelector) {{
      const scoped = queryAllDeep(context.doc, scopeSelector);
      if (!scoped.ok) {{
        return JSON.stringify({{ ok: false, error: scoped.error, boundaries: resolved.boundaries || [] }});
      }}
      if (scoped.elements.length === 0) {{
        continue;
      }}
      scopeFound = true;
      scopes = scoped.elements;
    }}

    for (const scope of scopes) {{
      const queried = queryAllDeep(scope, null);
      if (!queried.ok) {{
        return JSON.stringify({{ ok: false, error: queried.error, boundaries: resolved.boundaries || [] }});
      }}

      for (const el of queried.elements) {{
        if (results.length >= limit) break;
        if (seen.has(el)) continue;
        seen.add(el);

        const visible = isVisible(el);
        if (!includeElement(el, visible)) continue;

        const tag = el.tagName.toLowerCase();
        const text = normalizeText(el.innerText || el.textContent || "").slice(0, 200);
        const bounds = absoluteBounds(context, el);
        results.push({{
          tag,
          role: inferRole(el),
          name: accessibleName(el),
          x: bounds.x,
          y: bounds.y,
          w: bounds.w,
          h: bounds.h,
          selectorHints: selectorHints(el),
          visible,
          enabled: isEnabled(el),
          deepPath: deepDomPath(el),
          contentHash: simpleHash(`${{tag}}:${{text}}`),
          structuralSignature: `${{tag}}:${{el.childElementCount}}:${{el.attributes.length}}`,
          nearestHeading: nearestHeading(el),
          formOwnership: formOwnership(el),
          frameIndex: context.index,
          frameName: context.win.name || "",
          frameLabel: context.label,
          frameUrl: context.url,
        }});
      }}

      if (results.length >= limit) break;
    }}

    if (results.length >= limit) break;
  }}

  if (!scopeFound) {{
    return JSON.stringify({{ ok: false, error: "scope selector not found: " + scopeSelector, boundaries: resolved.boundaries || [] }});
  }}

  return JSON.stringify({{
    ok: true,
    nodes: results,
    count: results.length,
    truncated: results.length >= limit,
    boundaries: resolved.boundaries || [],
  }});
"#,
        selector = serde_json::to_string(&selector).unwrap(),
        interactive_only = if interactive_only { "true" } else { "false" },
        limit = limit,
        mode = serde_json::to_string(&mode).unwrap(),
    );
    eval_json_value(page, &build_js(state, true, &body), "snapshot").await
}

fn snapshot_node_frame_spec(node: &Value) -> Value {
    if let Some(name) = node.get("frameName").and_then(|value| value.as_str()) {
        if !name.is_empty() {
            return json!({ "kind": "name", "value": name });
        }
    }
    if let Some(url) = node.get("frameUrl").and_then(|value| value.as_str()) {
        if !url.is_empty() {
            return json!({ "kind": "urlPattern", "value": url });
        }
    }
    if let Some(index) = node.get("frameIndex").and_then(|value| value.as_u64()) {
        return json!({ "kind": "index", "value": index });
    }
    json!({ "kind": "main" })
}

pub async fn resolve_snapshot_node(page: &Page, node: &Value) -> Result<Value, String> {
    let frame_spec = snapshot_node_frame_spec(node);
    let body = format!(
        r#"
  const node = {node};
  const resolved = resolveFrameContext(frameSpec);
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, reason: resolved.error, boundaries: resolved.boundaries || [], crossOrigin: !!resolved.crossOrigin }});
  }}
  const context = resolved.context;

  function matchesFingerprint(el) {{
    const tag = el.tagName.toLowerCase();
    if (node.tag && tag !== node.tag) return false;
    const text = normalizeText(el.innerText || el.textContent || "").slice(0, 200);
    const hash = simpleHash(`${{tag}}:${{text}}`);
    if (hash === node.contentHash) return true;
    const signature = `${{tag}}:${{el.childElementCount}}:${{el.attributes.length}}`;
    return signature === node.structuralSignature && signature !== `${{tag}}:0:0`;
  }}

  function locate() {{
    if (Array.isArray(node.deepPath) && node.deepPath.length > 0) {{
      const byPath = resolveDeepPath(context.doc, node.deepPath);
      if (byPath && byPath.tagName && byPath.tagName.toLowerCase() === node.tag) {{
        return {{ element: byPath, tier: 1 }};
      }}
    }}

    if (Array.isArray(node.selectorHints)) {{
      for (const hint of node.selectorHints) {{
        const queried = queryAllDeep(context.doc, hint);
        if (queried.ok && queried.elements.length === 1) {{
          return {{ element: queried.elements[0], tier: 2 }};
        }}
      }}
    }}

    if (node.role && node.name) {{
      const queried = queryAllDeep(context.doc, null);
      if (queried.ok) {{
        for (const el of queried.elements) {{
          if (inferRole(el) === node.role && accessibleName(el) === node.name) {{
            return {{ element: el, tier: 3 }};
          }}
        }}
      }}
    }}

    if (node.tag) {{
      const queried = queryAllDeep(context.doc, node.tag);
      if (queried.ok) {{
        for (const el of queried.elements) {{
          if (matchesFingerprint(el)) {{
            return {{ element: el, tier: 4 }};
          }}
        }}
      }}
    }}

    return null;
  }}

  const match = locate();
  if (!match) {{
    return JSON.stringify({{ ok: false, reason: "stale", boundaries: resolved.boundaries || [], frameLabel: context.label, frameUrl: context.url }});
  }}

  return JSON.stringify({{
    ok: true,
    tier: match.tier,
    selector: selectorHint(match.element),
    summary: elementSummary(match.element, context),
    frameLabel: context.label,
    frameUrl: context.url,
    boundaries: resolved.boundaries || [],
  }});
"#,
        node = serde_json::to_string(node).unwrap(),
    );
    eval_json_value(
        page,
        &build_js_with_frame_spec(&frame_spec, false, &body),
        "resolve_snapshot_node",
    )
    .await
}

pub async fn act_on_snapshot_node(
    page: &Page,
    node: &Value,
    action: &str,
    options: &Value,
) -> Result<Value, String> {
    let frame_spec = snapshot_node_frame_spec(node);
    let body = format!(
        r#"
  const node = {node};
  const action = {action};
  const options = {options};
  const resolved = resolveFrameContext(frameSpec);
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, reason: resolved.error, boundaries: resolved.boundaries || [], crossOrigin: !!resolved.crossOrigin }});
  }}
  const context = resolved.context;

  function matchesFingerprint(el) {{
    const tag = el.tagName.toLowerCase();
    if (node.tag && tag !== node.tag) return false;
    const text = normalizeText(el.innerText || el.textContent || "").slice(0, 200);
    const hash = simpleHash(`${{tag}}:${{text}}`);
    if (hash === node.contentHash) return true;
    const signature = `${{tag}}:${{el.childElementCount}}:${{el.attributes.length}}`;
    return signature === node.structuralSignature && signature !== `${{tag}}:0:0`;
  }}

  function locate() {{
    if (Array.isArray(node.deepPath) && node.deepPath.length > 0) {{
      const byPath = resolveDeepPath(context.doc, node.deepPath);
      if (byPath && byPath.tagName && byPath.tagName.toLowerCase() === node.tag) {{
        return {{ element: byPath, tier: 1 }};
      }}
    }}

    if (Array.isArray(node.selectorHints)) {{
      for (const hint of node.selectorHints) {{
        const queried = queryAllDeep(context.doc, hint);
        if (queried.ok && queried.elements.length === 1) {{
          return {{ element: queried.elements[0], tier: 2 }};
        }}
      }}
    }}

    if (node.role && node.name) {{
      const queried = queryAllDeep(context.doc, null);
      if (queried.ok) {{
        for (const el of queried.elements) {{
          if (inferRole(el) === node.role && accessibleName(el) === node.name) {{
            return {{ element: el, tier: 3 }};
          }}
        }}
      }}
    }}

    if (node.tag) {{
      const queried = queryAllDeep(context.doc, node.tag);
      if (queried.ok) {{
        for (const el of queried.elements) {{
          if (matchesFingerprint(el)) {{
            return {{ element: el, tier: 4 }};
          }}
        }}
      }}
    }}

    return null;
  }}

  const match = locate();
  if (!match) {{
    return JSON.stringify({{ ok: false, reason: "stale", boundaries: resolved.boundaries || [], frameLabel: context.label, frameUrl: context.url }});
  }}

  const el = match.element;
  if (el.scrollIntoView) {{
    el.scrollIntoView({{ block: "center", inline: "center", behavior: "instant" }});
  }}

  try {{
    if (action === "click") {{
      if (el.focus) el.focus();
      if (typeof el.click === "function") {{
        el.click();
      }} else {{
        el.dispatchEvent(new MouseEvent("click", {{ bubbles: true, cancelable: true, view: context.win }}));
      }}
    }} else if (action === "hover") {{
      const hoverEvents = ["pointerover", "mouseover", "mouseenter", "pointermove", "mousemove"];
      for (const eventName of hoverEvents) {{
        el.dispatchEvent(new MouseEvent(eventName, {{ bubbles: true, cancelable: true, view: context.win }}));
      }}
    }} else if (action === "fill") {{
      const text = String(options.text || "");
      const clearFirst = !!options.clearFirst;
      const slowly = !!options.slowly;
      const submit = !!options.submit;
      if (el.focus) el.focus();

      if (clearFirst && "value" in el) {{
        el.value = "";
        el.dispatchEvent(new Event("input", {{ bubbles: true }}));
      }}

      if ("value" in el) {{
        if (slowly) {{
          let value = clearFirst ? "" : String(el.value || "");
          for (const ch of text) {{
            value += ch;
            el.value = value;
            el.dispatchEvent(new Event("input", {{ bubbles: true }}));
          }}
        }} else {{
          el.value = text;
          el.dispatchEvent(new Event("input", {{ bubbles: true }}));
        }}
        el.dispatchEvent(new Event("change", {{ bubbles: true }}));
      }} else if (el.isContentEditable) {{
        if (clearFirst) el.textContent = "";
        el.textContent = text;
        el.dispatchEvent(new Event("input", {{ bubbles: true }}));
      }} else {{
        throw new Error("element does not support text input");
      }}

      if (submit) {{
        el.dispatchEvent(new KeyboardEvent("keydown", {{ key: "Enter", bubbles: true }}));
        el.dispatchEvent(new KeyboardEvent("keyup", {{ key: "Enter", bubbles: true }}));
        const form = el.form || el.closest("form");
        if (form) {{
          if (typeof form.requestSubmit === "function") {{
            form.requestSubmit();
          }} else if (typeof form.submit === "function") {{
            form.submit();
          }}
        }}
      }}
    }} else {{
      throw new Error("unsupported ref action: " + action);
    }}
  }} catch (err) {{
    return JSON.stringify({{
      ok: false,
      reason: String(err),
      tier: match.tier,
      selector: selectorHint(el),
      summary: elementSummary(el, context),
      frameLabel: context.label,
      frameUrl: context.url,
      boundaries: resolved.boundaries || [],
    }});
  }}

  return JSON.stringify({{
    ok: true,
    tier: match.tier,
    selector: selectorHint(el),
    summary: elementSummary(el, context),
    frameLabel: context.label,
    frameUrl: context.url,
    boundaries: resolved.boundaries || [],
  }});
"#,
        node = serde_json::to_string(node).unwrap(),
        action = serde_json::to_string(action).unwrap(),
        options = serde_json::to_string(options).unwrap(),
    );
    eval_json_value(
        page,
        &build_js_with_frame_spec(&frame_spec, false, &body),
        "act_on_snapshot_node",
    )
    .await
}

pub async fn region_signature(
    page: &Page,
    state: &DaemonState,
    selector: &str,
) -> Result<Value, String> {
    let body = format!(
        r#"
  const resolved = resolveFrameContext(frameSpec);
  if (!resolved.ok) {{
    return JSON.stringify({{ ok: false, error: resolved.error, boundaries: resolved.boundaries || [] }});
  }}
  const queried = queryAllDeep(resolved.context.doc, {selector});
  if (!queried.ok) {{
    return JSON.stringify({{ ok: false, error: queried.error, boundaries: resolved.boundaries || [] }});
  }}
  const html = queried.elements.length > 0 ? (queried.elements[0].innerHTML || "") : "";
  return JSON.stringify({{
    ok: true,
    html,
    boundaries: resolved.boundaries || [],
  }});
"#,
        selector = serde_json::to_string(selector).unwrap()
    );
    eval_json_value(page, &build_js(state, false, &body), "region_signature").await
}
