//! HAR 1.2 export from the network log buffer.

use crate::daemon::logs::DaemonLogs;
use gsd_browser_common::state_dir;
use serde_json::{json, Value};
use std::fs;

/// Export network logs as a HAR 1.2 JSON file.
pub fn handle_har_export(logs: &DaemonLogs, params: &Value) -> Result<Value, String> {
    let filename = params
        .get("filename")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Read network entries non-destructively
    let entries = logs.network.snapshot();

    if entries.is_empty() {
        return Err("no network entries captured — nothing to export".to_string());
    }

    // Build HAR 1.2 structure
    let har_entries: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "startedDateTime": format_iso_timestamp(e.timestamp),
                "time": 0,
                "request": {
                    "method": e.method,
                    "url": e.url,
                    "httpVersion": "HTTP/1.1",
                    "cookies": [],
                    "headers": [],
                    "queryString": [],
                    "headersSize": -1,
                    "bodySize": -1,
                },
                "response": {
                    "status": e.status,
                    "statusText": status_text(e.status),
                    "httpVersion": "HTTP/1.1",
                    "cookies": [],
                    "headers": [],
                    "content": {
                        "size": -1,
                        "mimeType": "",
                        "text": if e.failed { &e.failure_text } else { "" },
                    },
                    "redirectURL": "",
                    "headersSize": -1,
                    "bodySize": -1,
                },
                "cache": {},
                "timings": {
                    "send": 0,
                    "wait": 0,
                    "receive": 0,
                },
                "resourceType": e.resource_type,
            })
        })
        .collect();

    let har = json!({
        "log": {
            "version": "1.2",
            "creator": {
                "name": "gsd-browser",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "entries": har_entries,
        }
    });

    // Determine output path
    let file_path = if let Some(f) = filename {
        f
    } else {
        let dir = state_dir().join("har");
        let _ = fs::create_dir_all(&dir);
        let ts = chrono_lite_timestamp();
        dir.join(format!("export-{ts}.har"))
            .to_string_lossy()
            .to_string()
    };

    let json_str = serde_json::to_string_pretty(&har)
        .map_err(|e| format!("failed to serialize HAR: {e}"))?;

    fs::write(&file_path, &json_str)
        .map_err(|e| format!("failed to write HAR file: {e}"))?;

    Ok(json!({
        "path": file_path,
        "entries": entries.len(),
        "size": json_str.len(),
    }))
}

/// Convert a CDP timestamp (seconds since epoch) to ISO 8601 format.
fn format_iso_timestamp(ts: f64) -> String {
    if ts <= 0.0 {
        return "1970-01-01T00:00:00.000Z".to_string();
    }
    let secs = ts as u64;
    let millis = ((ts - secs as f64) * 1000.0) as u64;
    // Simple UTC ISO 8601 without pulling in chrono crate
    let s = secs;
    let days = s / 86400;
    let time_of_day = s % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Compute year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z"
    )
}

/// Convert days since 1970-01-01 to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Civil calendar algorithm
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn status_text(status: u32) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "",
    }
}

fn chrono_lite_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_iso_timestamp() {
        // 2024-01-01 00:00:00 UTC = 1704067200
        let ts = 1704067200.0;
        let iso = format_iso_timestamp(ts);
        assert!(iso.starts_with("2024-01-01T00:00:00"));
    }

    #[test]
    fn test_status_text() {
        assert_eq!(status_text(200), "OK");
        assert_eq!(status_text(404), "Not Found");
        assert_eq!(status_text(999), "");
    }

    #[test]
    fn har_export_empty_logs() {
        let logs = DaemonLogs::new();
        let result = handle_har_export(&logs, &json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no network entries"));
    }

    #[test]
    fn har_export_with_entries() {
        use gsd_browser_common::types::NetworkLogEntry;
        let logs = DaemonLogs::new();
        logs.network.push(NetworkLogEntry {
            method: "GET".to_string(),
            url: "https://example.com/api".to_string(),
            status: 200,
            resource_type: "Fetch".to_string(),
            timestamp: 1704067200.0,
            failed: false,
            failure_text: String::new(),
            response_body: String::new(),
        });

        let tmp = std::env::temp_dir().join("bt-har-test.har");
        let result = handle_har_export(&logs, &json!({"filename": tmp.to_str().unwrap()})).unwrap();
        assert_eq!(result["entries"], 1);

        let content = std::fs::read_to_string(&tmp).unwrap();
        let har: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(har["log"]["version"], "1.2");
        assert_eq!(har["log"]["entries"][0]["request"]["method"], "GET");
        let _ = std::fs::remove_file(tmp);
    }
}
