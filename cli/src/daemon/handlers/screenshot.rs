use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::Page;
use serde_json::{json, Value};
use tracing::debug;

/// Handle screenshot capture — viewport JPEG (default), element PNG, or full-page JPEG.
///
/// Params:
///   selector  (string, optional) — CSS selector for element crop (forces PNG)
///   full_page (bool, default false) — capture entire scrollable page
///   quality   (u32, default 80) — JPEG compression quality 1-100
///   format    (string, default "jpeg") — "jpeg" or "png"
pub async fn handle_screenshot(page: &Page, params: &Value) -> Result<Value, String> {
    let selector = params.get("selector").and_then(|v| v.as_str());
    let full_page = params
        .get("full_page")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let quality = params.get("quality").and_then(|v| v.as_u64()).unwrap_or(80) as i64;
    let format_str = params
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("jpeg");

    // Element screenshot — always PNG, uses element bounding box clip
    if let Some(sel) = selector {
        debug!("[screenshot] element crop: {sel}");
        let element = page
            .find_element(sel)
            .await
            .map_err(|e| format!("element not found '{sel}': {e}"))?;

        let bytes = element
            .screenshot(CaptureScreenshotFormat::Png)
            .await
            .map_err(|e| format!("element screenshot failed: {e}"))?;

        let data = BASE64.encode(&bytes);

        // Get element bounding box for dimensions
        let dims = get_element_dimensions(page, sel).await;

        return Ok(json!({
            "data": data,
            "mimeType": "image/png",
            "width": dims.0,
            "height": dims.1,
            "scope": "element",
            "selector": sel,
            "byteLength": bytes.len(),
        }));
    }

    // Viewport or full-page screenshot
    let (cdp_format, mime_type) = match format_str {
        "png" => (CaptureScreenshotFormat::Png, "image/png"),
        _ => (CaptureScreenshotFormat::Jpeg, "image/jpeg"),
    };

    let scope = if full_page { "fullPage" } else { "viewport" };
    debug!("[screenshot] {scope} capture, format={format_str}, quality={quality}");

    let mut builder = ScreenshotParams::builder().format(cdp_format);

    // Quality only applies to JPEG
    if format_str != "png" {
        builder = builder.quality(quality);
    }

    if full_page {
        builder = builder.full_page(true).capture_beyond_viewport(true);
    }

    let screenshot_params = builder.build();

    let bytes = page
        .screenshot(screenshot_params)
        .await
        .map_err(|e| format!("screenshot failed: {e}"))?;

    let data = BASE64.encode(&bytes);

    // Get viewport dimensions for metadata
    let (width, height) = if full_page {
        get_full_page_dimensions(page).await
    } else {
        get_viewport_dimensions(page).await
    };

    Ok(json!({
        "data": data,
        "mimeType": mime_type,
        "width": width,
        "height": height,
        "scope": scope,
        "byteLength": bytes.len(),
    }))
}

/// Get viewport dimensions via JS.
async fn get_viewport_dimensions(page: &Page) -> (u64, u64) {
    let js = "JSON.stringify({w: window.innerWidth, h: window.innerHeight})";
    match page.evaluate_expression(js).await {
        Ok(val) => {
            if let Some(s) = val.value().and_then(|v| v.as_str().map(String::from)) {
                if let Ok(parsed) = serde_json::from_str::<Value>(&s) {
                    let w = parsed.get("w").and_then(|v| v.as_u64()).unwrap_or(0);
                    let h = parsed.get("h").and_then(|v| v.as_u64()).unwrap_or(0);
                    return (w, h);
                }
            }
            (0, 0)
        }
        Err(_) => (0, 0),
    }
}

/// Get full scrollable page dimensions via JS.
async fn get_full_page_dimensions(page: &Page) -> (u64, u64) {
    let js = "JSON.stringify({w: Math.max(document.documentElement.scrollWidth, document.documentElement.clientWidth), h: Math.max(document.documentElement.scrollHeight, document.documentElement.clientHeight)})";
    match page.evaluate_expression(js).await {
        Ok(val) => {
            if let Some(s) = val.value().and_then(|v| v.as_str().map(String::from)) {
                if let Ok(parsed) = serde_json::from_str::<Value>(&s) {
                    let w = parsed.get("w").and_then(|v| v.as_u64()).unwrap_or(0);
                    let h = parsed.get("h").and_then(|v| v.as_u64()).unwrap_or(0);
                    return (w, h);
                }
            }
            (0, 0)
        }
        Err(_) => (0, 0),
    }
}

/// Get element bounding box dimensions via JS.
async fn get_element_dimensions(page: &Page, selector: &str) -> (u64, u64) {
    let js = format!(
        r#"(() => {{
            const el = document.querySelector({sel});
            if (!el) return JSON.stringify({{w: 0, h: 0}});
            const r = el.getBoundingClientRect();
            return JSON.stringify({{w: Math.round(r.width), h: Math.round(r.height)}});
        }})()"#,
        sel = serde_json::to_string(selector).unwrap_or_else(|_| format!("\"{}\"", selector))
    );
    match page.evaluate_expression(&js).await {
        Ok(val) => {
            if let Some(s) = val.value().and_then(|v| v.as_str().map(String::from)) {
                if let Ok(parsed) = serde_json::from_str::<Value>(&s) {
                    let w = parsed.get("w").and_then(|v| v.as_u64()).unwrap_or(0);
                    let h = parsed.get("h").and_then(|v| v.as_u64()).unwrap_or(0);
                    return (w, h);
                }
            }
            (0, 0)
        }
        Err(_) => (0, 0),
    }
}
