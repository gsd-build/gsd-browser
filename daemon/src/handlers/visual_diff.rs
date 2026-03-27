use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams, Viewport,
};
use chromiumoxide::Page;
use image::{ImageBuffer, ImageReader, Rgba};
use serde_json::{json, Value};
use std::io::Cursor;
use std::path::PathBuf;
use tracing::debug;

/// Get the baselines directory at ~/.browser-tools/baselines/
fn baselines_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".browser-tools")
        .join("baselines")
}

/// Handle visual diff — take screenshot, compare against stored baseline, return similarity.
///
/// Params:
///   name             (string, optional) — baseline name (default: auto from URL + viewport)
///   selector         (string, optional) — CSS selector to scope comparison
///   threshold        (f64, default 0.1) — pixel matching tolerance 0–1
///   update_baseline  (bool, default false) — overwrite existing baseline
pub async fn handle_visual_diff(page: &Page, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let threshold = params
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.1);
    let update_baseline = params
        .get("update_baseline")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let selector = params.get("selector").and_then(|v| v.as_str());

    debug!("[visual_diff] name={name}, threshold={threshold}, update_baseline={update_baseline}");

    // Take current screenshot as PNG
    let screenshot_bytes = if let Some(sel) = selector {
        let element = page
            .find_element(sel)
            .await
            .map_err(|e| format!("element not found '{sel}': {e}"))?;
        element
            .screenshot(CaptureScreenshotFormat::Png)
            .await
            .map_err(|e| format!("element screenshot failed: {e}"))?
    } else {
        let params_cdp = CaptureScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png)
            .build();
        let result = page
            .execute(params_cdp)
            .await
            .map_err(|e| format!("screenshot failed: {e}"))?;
        // Binary wraps a base64-encoded string — decode it to raw PNG bytes
        let b64_str: &str = result.result.data.as_ref();
        BASE64
            .decode(b64_str)
            .map_err(|e| format!("failed to decode screenshot base64: {e}"))?
    };

    // Decode current screenshot
    let current_img = ImageReader::new(Cursor::new(&screenshot_bytes))
        .with_guessed_format()
        .map_err(|e| format!("failed to guess image format: {e}"))?
        .decode()
        .map_err(|e| format!("failed to decode screenshot: {e}"))?
        .to_rgba8();

    let dir = baselines_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create baselines dir: {e}"))?;

    let sanitized_name = name.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");
    let baseline_path = dir.join(format!("{sanitized_name}.png"));

    // If no baseline exists or update requested, save current as baseline
    if !baseline_path.exists() || update_baseline {
        std::fs::write(&baseline_path, &screenshot_bytes)
            .map_err(|e| format!("failed to write baseline: {e}"))?;

        let status = if update_baseline {
            "baseline_updated"
        } else {
            "baseline_created"
        };

        return Ok(json!({
            "status": status,
            "baselinePath": baseline_path.to_string_lossy(),
            "similarity": 1.0,
            "diffPixelCount": 0,
            "width": current_img.width(),
            "height": current_img.height(),
        }));
    }

    // Load existing baseline
    let baseline_bytes = std::fs::read(&baseline_path)
        .map_err(|e| format!("failed to read baseline: {e}"))?;

    let baseline_img = ImageReader::new(Cursor::new(&baseline_bytes))
        .with_guessed_format()
        .map_err(|e| format!("failed to guess baseline format: {e}"))?
        .decode()
        .map_err(|e| format!("failed to decode baseline: {e}"))?
        .to_rgba8();

    // Compare pixel-by-pixel
    let (bw, bh) = (baseline_img.width(), baseline_img.height());
    let (cw, ch) = (current_img.width(), current_img.height());

    if bw != cw || bh != ch {
        // Different dimensions — report mismatch
        return Ok(json!({
            "status": "dimension_mismatch",
            "baselinePath": baseline_path.to_string_lossy(),
            "similarity": 0.0,
            "diffPixelCount": (bw as u64 * bh as u64).max(cw as u64 * ch as u64),
            "baselineSize": format!("{bw}x{bh}"),
            "currentSize": format!("{cw}x{ch}"),
        }));
    }

    let total_pixels = (cw as u64) * (ch as u64);
    let threshold_u8 = (threshold * 255.0).clamp(0.0, 255.0) as i32;
    let mut diff_count: u64 = 0;

    // Build diff image — matching pixels are transparent, differing pixels are red
    let mut diff_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(cw, ch);

    for y in 0..ch {
        for x in 0..cw {
            let bp = baseline_img.get_pixel(x, y);
            let cp = current_img.get_pixel(x, y);

            let dr = (bp[0] as i32 - cp[0] as i32).abs();
            let dg = (bp[1] as i32 - cp[1] as i32).abs();
            let db = (bp[2] as i32 - cp[2] as i32).abs();
            let da = (bp[3] as i32 - cp[3] as i32).abs();

            if dr > threshold_u8 || dg > threshold_u8 || db > threshold_u8 || da > threshold_u8 {
                diff_count += 1;
                diff_img.put_pixel(x, y, Rgba([255, 0, 0, 200]));
            } else {
                // Semi-transparent original for context
                diff_img.put_pixel(x, y, Rgba([cp[0], cp[1], cp[2], 80]));
            }
        }
    }

    let similarity = if total_pixels > 0 {
        1.0 - (diff_count as f64 / total_pixels as f64)
    } else {
        1.0
    };

    // Write diff image if there are differences
    let diff_path = if diff_count > 0 {
        let dp = dir.join(format!("{sanitized_name}-diff.png"));
        diff_img
            .save(&dp)
            .map_err(|e| format!("failed to write diff image: {e}"))?;
        Some(dp.to_string_lossy().to_string())
    } else {
        None
    };

    Ok(json!({
        "status": if diff_count == 0 { "match" } else { "changed" },
        "baselinePath": baseline_path.to_string_lossy(),
        "diffPath": diff_path,
        "similarity": (similarity * 10000.0).round() / 10000.0,
        "diffPixelCount": diff_count,
        "totalPixels": total_pixels,
        "threshold": threshold,
        "width": cw,
        "height": ch,
    }))
}

/// Handle zoom region — capture a rectangular region and optionally upscale it.
///
/// Params:
///   x      (f64, required) — left coordinate in CSS pixels
///   y      (f64, required) — top coordinate in CSS pixels
///   width  (f64, required) — width of region in CSS pixels
///   height (f64, required) — height of region in CSS pixels
///   scale  (f64, default 2) — upscale factor (1 = native, 2-4 = zoomed)
pub async fn handle_zoom_region(page: &Page, params: &Value) -> Result<Value, String> {
    let x = params
        .get("x")
        .and_then(|v| v.as_f64())
        .ok_or("missing required param 'x'")?;
    let y = params
        .get("y")
        .and_then(|v| v.as_f64())
        .ok_or("missing required param 'y'")?;
    let width = params
        .get("width")
        .and_then(|v| v.as_f64())
        .ok_or("missing required param 'width'")?;
    let height = params
        .get("height")
        .and_then(|v| v.as_f64())
        .ok_or("missing required param 'height'")?;
    let scale = params
        .get("scale")
        .and_then(|v| v.as_f64())
        .unwrap_or(2.0);

    debug!("[zoom_region] x={x}, y={y}, w={width}, h={height}, scale={scale}");

    // Use CDP screenshot with clip viewport
    let clip = Viewport {
        x,
        y,
        width,
        height,
        scale: scale,
    };

    let cdp_params = CaptureScreenshotParams::builder()
        .format(CaptureScreenshotFormat::Png)
        .clip(clip)
        .build();

    let result = page
        .execute(cdp_params)
        .await
        .map_err(|e| format!("screenshot with clip failed: {e}"))?;

    // Binary wraps the base64-encoded data string from CDP
    let data: String = result.result.data.into();
    let png_bytes = BASE64
        .decode(&data)
        .map_err(|e| format!("failed to decode screenshot base64: {e}"))?;

    let output_width = (width * scale) as u64;
    let output_height = (height * scale) as u64;

    Ok(json!({
        "data": data,
        "mimeType": "image/png",
        "width": output_width,
        "height": output_height,
        "region": {
            "x": x,
            "y": y,
            "width": width,
            "height": height,
        },
        "scale": scale,
        "byteLength": png_bytes.len(),
    }))
}
