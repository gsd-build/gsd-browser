use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chromiumoxide::Page;
use gsd_browser_common::cloud::{
    CloudFrame, CloudRef, CloudRefs, CloudSessionStatus, CloudUserInput,
};
use gsd_browser_common::identity::{
    identity_metadata_path, identity_profile_dir, BrowserIdentity, IdentityScope,
};
use image::{GenericImageView, ImageReader};
use serde_json::{json, Value};
use std::fs;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::daemon::{handlers, state::DaemonState};

static FRAME_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
struct ViewportMetrics {
    width: u32,
    height: u32,
    device_pixel_ratio: f64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn page_url(page: &Page) -> String {
    page.url().await.unwrap_or_default().unwrap_or_default()
}

async fn page_title(page: &Page) -> String {
    page.get_title()
        .await
        .unwrap_or_default()
        .unwrap_or_default()
}

fn image_dimensions_from_base64(data: &str) -> Option<(u32, u32)> {
    let bytes = BASE64.decode(data).ok()?;
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(image.dimensions())
}

async fn viewport_metrics(
    page: &Page,
    fallback_width: u32,
    fallback_height: u32,
) -> ViewportMetrics {
    let value = page
        .evaluate_expression(
            r#"(() => ({
                width: Math.max(0, Math.round(window.innerWidth || 0)),
                height: Math.max(0, Math.round(window.innerHeight || 0)),
                devicePixelRatio: Number(window.devicePixelRatio || 1)
            }))()"#,
        )
        .await
        .ok()
        .and_then(|result| result.into_value().ok())
        .unwrap_or_else(|| json!({}));

    let width = value
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    let height = value
        .get("height")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;

    ViewportMetrics {
        width: if width == 0 { fallback_width } else { width },
        height: if height == 0 { fallback_height } else { height },
        device_pixel_ratio: value
            .get("devicePixelRatio")
            .and_then(Value::as_f64)
            .unwrap_or(1.0),
    }
}

fn runtime_identity(state: &DaemonState) -> Option<BrowserIdentity> {
    let scope = state.session.identity_scope?;
    let key = state.session.identity_key.clone()?;
    Some(BrowserIdentity {
        scope,
        project_id: state.session.identity_project_id.clone(),
        display_name: key.clone(),
        key,
    })
}

pub async fn handle_cloud_session_status(
    page: &Page,
    state: &DaemonState,
) -> Result<Value, String> {
    let status = CloudSessionStatus {
        session_name: state.session.session_name.clone(),
        active_url: page_url(page).await,
        active_title: page_title(page).await,
        identity: runtime_identity(state),
        control_owner: "agent".to_string(),
    };
    serde_json::to_value(status).map_err(|err| err.to_string())
}

pub async fn handle_cloud_frame(page: &Page, params: &Value) -> Result<Value, String> {
    let quality = params.get("quality").and_then(Value::as_u64).unwrap_or(70) as u32;
    let shot = handlers::screenshot::handle_screenshot(
        page,
        &json!({"format": "jpeg", "quality": quality, "full_page": false}),
    )
    .await?;
    let data = shot
        .get("data")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut width = shot
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    let mut height = shot
        .get("height")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    if (width == 0 || height == 0) && !data.is_empty() {
        if let Some((decoded_width, decoded_height)) = image_dimensions_from_base64(&data) {
            width = decoded_width;
            height = decoded_height;
        }
    }
    let viewport = viewport_metrics(page, width, height).await;
    let encoded_bytes = BASE64.decode(&data).map(|bytes| bytes.len()).unwrap_or(0);
    let capture_scale_x = if viewport.width == 0 {
        1.0
    } else {
        width as f64 / viewport.width as f64
    };
    let capture_scale_y = if viewport.height == 0 {
        1.0
    } else {
        height as f64 / viewport.height as f64
    };
    let frame = CloudFrame {
        sequence: FRAME_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        content_type: "image/jpeg".to_string(),
        data_base64: data,
        width,
        height,
        viewport_width: viewport.width,
        viewport_height: viewport.height,
        viewport_css_width: viewport.width,
        viewport_css_height: viewport.height,
        capture_pixel_width: width,
        capture_pixel_height: height,
        device_pixel_ratio: viewport.device_pixel_ratio,
        capture_scale_x,
        capture_scale_y,
        captured_at_ms: now_ms(),
        encoded_bytes,
        quality,
        capture_pixel_ratio: viewport.device_pixel_ratio,
        url: page_url(page).await,
        title: page_title(page).await,
    };
    serde_json::to_value(frame).map_err(|err| err.to_string())
}

fn cloud_ref_from_snapshot_item(version: u64, key: &str, item: &Value) -> CloudRef {
    let role = item
        .get("role")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| item.get("tag").and_then(Value::as_str))
        .unwrap_or("element")
        .to_string();
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    CloudRef {
        ref_id: format!("@v{version}:{key}"),
        key: key.to_string(),
        role,
        name,
        x: item.get("x").and_then(Value::as_f64).unwrap_or_default(),
        y: item.get("y").and_then(Value::as_f64).unwrap_or_default(),
        w: item.get("w").and_then(Value::as_f64).unwrap_or_default(),
        h: item.get("h").and_then(Value::as_f64).unwrap_or_default(),
    }
}

pub async fn handle_cloud_refs(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(200);
    let snapshot = handlers::refs::handle_snapshot(
        page,
        state,
        &json!({"mode": "interactive", "limit": limit}),
    )
    .await?;
    let version = snapshot
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let truncated = snapshot
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let refs = snapshot
        .get("refs")
        .and_then(Value::as_object)
        .map(|items| {
            let mut refs = items
                .iter()
                .map(|(key, item)| cloud_ref_from_snapshot_item(version, key, item))
                .collect::<Vec<_>>();
            refs.sort_by(|left, right| left.key.cmp(&right.key));
            refs
        })
        .unwrap_or_default();

    serde_json::to_value(CloudRefs {
        version,
        refs,
        truncated,
        limit: Some(limit),
        captured_at_ms: now_ms(),
    })
    .map_err(|err| err.to_string())
}

pub async fn handle_cloud_user_input(
    page: &Page,
    state: &DaemonState,
    params: &Value,
) -> Result<Value, String> {
    let cloud: CloudUserInput =
        serde_json::from_value(params.clone()).map_err(|err| err.to_string())?;
    let input = cloud.into_user_input_event("cloud")?;
    crate::daemon::view::control::authorize_page_effect(
        state,
        crate::daemon::view::control::PageEffectSource::Cloud,
        &input,
    )
    .await?;
    crate::daemon::input_dispatch::dispatch_user_input(page, state, &input).await
}

fn read_identity(path: std::path::PathBuf) -> Option<BrowserIdentity> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn handle_cloud_identity_list(params: &Value) -> Result<Value, String> {
    let scope = params
        .get("scope")
        .and_then(Value::as_str)
        .map(IdentityScope::parse)
        .transpose()?;
    let project_id = params.get("projectId").and_then(Value::as_str);
    if matches!(scope, Some(IdentityScope::Project)) && project_id.is_none() {
        return Err("project identity requires projectId".to_string());
    }
    let mut identities = Vec::new();
    let scopes: Vec<IdentityScope> = match scope {
        Some(scope) => vec![scope],
        None => vec![
            IdentityScope::Session,
            IdentityScope::Project,
            IdentityScope::Global,
        ],
    };
    for scope in scopes {
        let roots = match scope {
            IdentityScope::Project => {
                let project_root = gsd_browser_common::state_dir()
                    .join("identities")
                    .join(scope.as_dir());
                if let Some(project_id) = project_id {
                    vec![project_root.join(gsd_browser_common::sanitize_filename(project_id)?)]
                } else {
                    match fs::read_dir(project_root) {
                        Ok(entries) => entries.flatten().map(|entry| entry.path()).collect(),
                        Err(_) => Vec::new(),
                    }
                }
            }
            _ => vec![gsd_browser_common::state_dir()
                .join("identities")
                .join(scope.as_dir())],
        };
        for root in roots {
            let Ok(entries) = fs::read_dir(root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path().join("identity.json");
                if let Some(identity) = read_identity(path) {
                    identities.push(identity);
                }
            }
        }
    }
    Ok(json!({ "identities": identities }))
}

pub fn handle_cloud_identity_save(params: &Value) -> Result<Value, String> {
    let identity: BrowserIdentity =
        serde_json::from_value(params.clone()).map_err(|err| err.to_string())?;
    let profile_dir = identity_profile_dir(
        identity.scope,
        identity.project_id.as_deref(),
        &identity.key,
    )?;
    fs::create_dir_all(&profile_dir)
        .map_err(|err| format!("failed to create identity profile dir: {err}"))?;
    let metadata_path = identity_metadata_path(
        identity.scope,
        identity.project_id.as_deref(),
        &identity.key,
    )?;
    let data = serde_json::to_string_pretty(&identity).map_err(|err| err.to_string())?;
    fs::write(&metadata_path, data)
        .map_err(|err| format!("failed to write identity metadata: {err}"))?;
    serde_json::to_value(identity).map_err(|err| err.to_string())
}

pub fn handle_cloud_identity_revoke(params: &Value) -> Result<Value, String> {
    let scope = params
        .get("scope")
        .and_then(Value::as_str)
        .ok_or_else(|| "revoke requires scope".to_string())
        .and_then(IdentityScope::parse)?;
    let project_id = params.get("projectId").and_then(Value::as_str);
    let key = params
        .get("key")
        .and_then(Value::as_str)
        .ok_or("revoke requires key")?;
    let profile_dir = identity_profile_dir(scope, project_id, key)?;
    let identity_dir = profile_dir
        .parent()
        .ok_or("identity profile path has no parent")?;
    if identity_dir.exists() {
        fs::remove_dir_all(identity_dir)
            .map_err(|err| format!("failed to remove identity: {err}"))?;
    }
    Ok(json!({ "revoked": true }))
}
