//! Device emulation handler — hardcoded presets with fuzzy name matching.

use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams, SetUserAgentOverrideParams,
};
use chromiumoxide::Page;
use serde_json::{json, Value};
use tracing::debug;

/// A device preset for emulation.
struct DevicePreset {
    name: &'static str,
    width: i64,
    height: i64,
    device_scale_factor: f64,
    mobile: bool,
    user_agent: &'static str,
}

const DEVICE_PRESETS: &[DevicePreset] = &[
    DevicePreset {
        name: "iPhone 15",
        width: 393,
        height: 852,
        device_scale_factor: 3.0,
        mobile: true,
        user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
    },
    DevicePreset {
        name: "iPhone 15 Pro",
        width: 393,
        height: 852,
        device_scale_factor: 3.0,
        mobile: true,
        user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
    },
    DevicePreset {
        name: "iPhone 15 Pro Max",
        width: 430,
        height: 932,
        device_scale_factor: 3.0,
        mobile: true,
        user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
    },
    DevicePreset {
        name: "Pixel 7",
        width: 412,
        height: 915,
        device_scale_factor: 2.625,
        mobile: true,
        user_agent: "Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Mobile Safari/537.36",
    },
    DevicePreset {
        name: "Pixel 8",
        width: 412,
        height: 915,
        device_scale_factor: 2.625,
        mobile: true,
        user_agent: "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36",
    },
    DevicePreset {
        name: "Galaxy S23",
        width: 360,
        height: 780,
        device_scale_factor: 3.0,
        mobile: true,
        user_agent: "Mozilla/5.0 (Linux; Android 13; SM-S911B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Mobile Safari/537.36",
    },
    DevicePreset {
        name: "Galaxy S24",
        width: 360,
        height: 780,
        device_scale_factor: 3.0,
        mobile: true,
        user_agent: "Mozilla/5.0 (Linux; Android 14; SM-S921B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36",
    },
    DevicePreset {
        name: "iPad Pro 11",
        width: 834,
        height: 1194,
        device_scale_factor: 2.0,
        mobile: true,
        user_agent: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
    },
    DevicePreset {
        name: "iPad Air",
        width: 820,
        height: 1180,
        device_scale_factor: 2.0,
        mobile: true,
        user_agent: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
    },
    DevicePreset {
        name: "iPad Mini",
        width: 768,
        height: 1024,
        device_scale_factor: 2.0,
        mobile: true,
        user_agent: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
    },
];

/// Fuzzy-match a device name (case-insensitive contains).
fn find_device(query: &str) -> Option<&'static DevicePreset> {
    let lower = query.to_lowercase();

    // Try exact match first
    if let Some(preset) = DEVICE_PRESETS.iter().find(|d| d.name.to_lowercase() == lower) {
        return Some(preset);
    }

    // Fall back to contains match
    DEVICE_PRESETS
        .iter()
        .find(|d| d.name.to_lowercase().contains(&lower) || lower.contains(&d.name.to_lowercase()))
}

/// Handle `emulate_device` command.
/// Params: { device: string }
/// Special: device="list" returns all available presets.
pub async fn handle_emulate_device(page: &Page, params: &Value) -> Result<Value, String> {
    let device_name = params
        .get("device")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required parameter: device".to_string())?;

    // Special case: list all devices
    if device_name.eq_ignore_ascii_case("list") {
        let devices: Vec<Value> = DEVICE_PRESETS
            .iter()
            .map(|d| {
                json!({
                    "name": d.name,
                    "width": d.width,
                    "height": d.height,
                    "deviceScaleFactor": d.device_scale_factor,
                    "mobile": d.mobile,
                })
            })
            .collect();

        return Ok(json!({ "devices": devices }));
    }

    let preset = find_device(device_name).ok_or_else(|| {
        let available: Vec<&str> = DEVICE_PRESETS.iter().map(|d| d.name).collect();
        format!(
            "device not found: '{device_name}'. Available: {}",
            available.join(", ")
        )
    })?;

    debug!(
        "emulate_device: {} ({}x{} @{}x, mobile={})",
        preset.name, preset.width, preset.height, preset.device_scale_factor, preset.mobile
    );

    // Set device metrics
    let metrics = SetDeviceMetricsOverrideParams::new(
        preset.width,
        preset.height,
        preset.device_scale_factor,
        preset.mobile,
    );
    page.execute(metrics)
        .await
        .map_err(|e| format!("SetDeviceMetricsOverride failed: {e}"))?;

    // Set user agent
    let ua = SetUserAgentOverrideParams::new(preset.user_agent);
    page.execute(ua)
        .await
        .map_err(|e| format!("SetUserAgentOverride failed: {e}"))?;

    // Enable touch emulation via JS
    let touch_js = format!(
        "(() => {{ Object.defineProperty(navigator, 'maxTouchPoints', {{ get: () => {} }}); }})()",
        if preset.mobile { 5 } else { 0 }
    );
    let _ = page.evaluate_expression(&touch_js).await;

    Ok(json!({
        "device": preset.name,
        "width": preset.width,
        "height": preset.height,
        "deviceScaleFactor": preset.device_scale_factor,
        "mobile": preset.mobile,
        "userAgent": preset.user_agent,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_device_exact() {
        let d = find_device("iPhone 15").unwrap();
        assert_eq!(d.name, "iPhone 15");
    }

    #[test]
    fn find_device_case_insensitive() {
        let d = find_device("iphone 15 pro").unwrap();
        assert_eq!(d.name, "iPhone 15 Pro");
    }

    #[test]
    fn find_device_contains() {
        let d = find_device("pixel 7").unwrap();
        assert_eq!(d.name, "Pixel 7");
    }

    #[test]
    fn find_device_ipad() {
        let d = find_device("iPad Pro 11").unwrap();
        assert_eq!(d.name, "iPad Pro 11");
    }

    #[test]
    fn find_device_galaxy() {
        let d = find_device("galaxy s23").unwrap();
        assert_eq!(d.name, "Galaxy S23");
    }

    #[test]
    fn find_device_not_found() {
        assert!(find_device("Nokia 3310").is_none());
    }
}
