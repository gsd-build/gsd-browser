use chromiumoxide::cdp::browser_protocol::network::{
    CookieParam, GetCookiesParams, SetCookiesParams,
};
use chromiumoxide::Page;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct BrowserState {
    cookies: Vec<Value>,
    local_storage: Value,
    session_storage: Value,
}

fn state_dir() -> PathBuf {
    gsd_browser_common::state_dir().join("state")
}

pub async fn handle_save_state(page: &Page, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    // 1. Get cookies via CDP
    let cookies_resp = page
        .execute(GetCookiesParams::default())
        .await
        .map_err(|e| format!("GetCookies failed: {e}"))?;

    let cookies_json = serde_json::to_value(&cookies_resp.result.cookies)
        .map_err(|e| format!("serialize cookies: {e}"))?;

    // 2. Get localStorage via JS
    let local_storage: Value = page
        .evaluate(
            r#"(() => {
                const items = {};
                for (let i = 0; i < localStorage.length; i++) {
                    const key = localStorage.key(i);
                    items[key] = localStorage.getItem(key);
                }
                return items;
            })()"#,
        )
        .await
        .map_err(|e| format!("localStorage read failed: {e}"))?
        .into_value()
        .map_err(|e| format!("localStorage deserialize: {e}"))?;

    // 3. Get sessionStorage via JS
    let session_storage: Value = page
        .evaluate(
            r#"(() => {
                const items = {};
                for (let i = 0; i < sessionStorage.length; i++) {
                    const key = sessionStorage.key(i);
                    items[key] = sessionStorage.getItem(key);
                }
                return items;
            })()"#,
        )
        .await
        .map_err(|e| format!("sessionStorage read failed: {e}"))?
        .into_value()
        .map_err(|e| format!("sessionStorage deserialize: {e}"))?;

    // 4. Serialize and write to disk
    let state = BrowserState {
        cookies: match cookies_json {
            Value::Array(arr) => arr,
            _ => vec![],
        },
        local_storage,
        session_storage,
    };

    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create state dir: {e}"))?;

    let file_path = dir.join(format!("{name}.json"));
    let json_bytes =
        serde_json::to_string_pretty(&state).map_err(|e| format!("serialize state: {e}"))?;
    fs::write(&file_path, json_bytes).map_err(|e| format!("write state file: {e}"))?;

    let cookie_count = state.cookies.len();
    let ls_count = if let Value::Object(m) = &state.local_storage {
        m.len()
    } else {
        0
    };
    let ss_count = if let Value::Object(m) = &state.session_storage {
        m.len()
    } else {
        0
    };

    info!(
        "[state_persist] saved state '{}': {} cookies, {} localStorage, {} sessionStorage → {:?}",
        name, cookie_count, ls_count, ss_count, file_path
    );

    Ok(json!({
        "name": name,
        "path": file_path.to_string_lossy(),
        "cookies": cookie_count,
        "localStorage": ls_count,
        "sessionStorage": ss_count,
    }))
}

pub async fn handle_restore_state(page: &Page, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let file_path = state_dir().join(format!("{name}.json"));
    if !file_path.exists() {
        return Err(format!(
            "state file not found: {name}.json (looked in {:?})",
            state_dir()
        ));
    }

    let json_str =
        fs::read_to_string(&file_path).map_err(|e| format!("read state file: {e}"))?;
    let state: BrowserState =
        serde_json::from_str(&json_str).map_err(|e| format!("parse state file: {e}"))?;

    // 1. Restore cookies via CDP SetCookiesParams
    if !state.cookies.is_empty() {
        let cookie_params: Vec<CookieParam> = state
            .cookies
            .iter()
            .filter_map(|c| {
                let name = c.get("name")?.as_str()?.to_string();
                let value = c.get("value")?.as_str()?.to_string();
                let mut cp = CookieParam::new(name, value);
                if let Some(domain) = c.get("domain").and_then(|v| v.as_str()) {
                    cp.domain = Some(domain.to_string());
                }
                if let Some(path) = c.get("path").and_then(|v| v.as_str()) {
                    cp.path = Some(path.to_string());
                }
                if let Some(secure) = c.get("secure").and_then(|v| v.as_bool()) {
                    cp.secure = Some(secure);
                }
                if let Some(http_only) = c.get("httpOnly").and_then(|v| v.as_bool()) {
                    cp.http_only = Some(http_only);
                }
                Some(cp)
            })
            .collect();

        if !cookie_params.is_empty() {
            page.execute(SetCookiesParams::new(cookie_params))
            .await
            .map_err(|e| format!("SetCookies failed: {e}"))?;
        }
    }

    // 2. Restore localStorage via JS
    if let Value::Object(map) = &state.local_storage {
        if !map.is_empty() {
            let ls_json = serde_json::to_string(map)
                .map_err(|e| format!("serialize localStorage: {e}"))?;
            let js = format!(
                r#"(() => {{
                    const items = JSON.parse({ls_json_str});
                    for (const [key, value] of Object.entries(items)) {{
                        localStorage.setItem(key, value);
                    }}
                    return Object.keys(items).length;
                }})()"#,
                ls_json_str = serde_json::to_string(&ls_json).unwrap()
            );
            page.evaluate(js)
                .await
                .map_err(|e| format!("localStorage restore failed: {e}"))?;
        }
    }

    // 3. Restore sessionStorage via JS
    if let Value::Object(map) = &state.session_storage {
        if !map.is_empty() {
            let ss_json = serde_json::to_string(map)
                .map_err(|e| format!("serialize sessionStorage: {e}"))?;
            let js = format!(
                r#"(() => {{
                    const items = JSON.parse({ss_json_str});
                    for (const [key, value] of Object.entries(items)) {{
                        sessionStorage.setItem(key, value);
                    }}
                    return Object.keys(items).length;
                }})()"#,
                ss_json_str = serde_json::to_string(&ss_json).unwrap()
            );
            page.evaluate(js)
                .await
                .map_err(|e| format!("sessionStorage restore failed: {e}"))?;
        }
    }

    let cookie_count = state.cookies.len();
    let ls_count = if let Value::Object(m) = &state.local_storage {
        m.len()
    } else {
        0
    };
    let ss_count = if let Value::Object(m) = &state.session_storage {
        m.len()
    } else {
        0
    };

    info!(
        "[state_persist] restored state '{}': {} cookies, {} localStorage, {} sessionStorage",
        name, cookie_count, ls_count, ss_count
    );

    Ok(json!({
        "name": name,
        "restored": true,
        "cookies": cookie_count,
        "localStorage": ls_count,
        "sessionStorage": ss_count,
    }))
}
