use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
use chromiumoxide::Page;
use serde_json::{json, Value};
use std::path::PathBuf;
use tracing::debug;

/// Get the artifacts directory at ~/.gsd-browser/artifacts/
fn artifacts_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".gsd-browser")
        .join("artifacts")
}

/// Handle save PDF — render current page as PDF and write to disk.
///
/// Params:
///   format           (string, default "A4") — page format: A4, Letter, Legal, Tabloid
///   print_background (bool, default true) — include background graphics
///   filename         (string, optional) — output filename
///   output           (string, optional) — full output path (overrides default dir)
pub async fn handle_save_pdf(page: &Page, params: &Value) -> Result<Value, String> {
    let format = params
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("A4");
    let print_bg = params
        .get("print_background")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let filename = params.get("filename").and_then(|v| v.as_str());
    let output_path = params.get("output").and_then(|v| v.as_str());

    debug!("[save_pdf] format={format}, print_background={print_bg}");

    // Map format to paper dimensions (in inches)
    let (paper_width, paper_height) = match format.to_lowercase().as_str() {
        "a4" => (8.27, 11.69),
        "letter" => (8.5, 11.0),
        "legal" => (8.5, 14.0),
        "tabloid" => (11.0, 17.0),
        custom => {
            // Try to parse custom format like "8.5in x 11in"
            let parts: Vec<&str> = custom.split('x').collect();
            if parts.len() == 2 {
                let w = parts[0]
                    .trim()
                    .trim_end_matches("in")
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("invalid custom format width: {}", parts[0]))?;
                let h = parts[1]
                    .trim()
                    .trim_end_matches("in")
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("invalid custom format height: {}", parts[1]))?;
                (w, h)
            } else {
                return Err(format!(
                    "unknown format '{format}'. Use: A4, Letter, Legal, Tabloid, or 'WxH' in inches"
                ));
            }
        }
    };

    let pdf_params = PrintToPdfParams::builder()
        .paper_width(paper_width)
        .paper_height(paper_height)
        .print_background(print_bg)
        .build();

    let pdf_bytes = page
        .pdf(pdf_params)
        .await
        .map_err(|e| format!("PDF generation failed: {e}"))?;

    // Determine output path
    let out_path = if let Some(p) = output_path {
        PathBuf::from(p)
    } else {
        let dir = artifacts_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create artifacts dir: {e}"))?;

        let fname = if let Some(f) = filename {
            f.to_string()
        } else {
            // Auto-generate from page title + timestamp
            let title = page
                .get_title()
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "page".to_string());
            let sanitized = title
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .take(50)
                .collect::<String>();
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            format!("{sanitized}-{ts}.pdf")
        };

        dir.join(fname)
    };

    // Ensure parent dir exists for custom paths
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create output dir: {e}"))?;
    }

    let byte_len = pdf_bytes.len();
    std::fs::write(&out_path, &pdf_bytes)
        .map_err(|e| format!("failed to write PDF to '{}': {e}", out_path.display()))?;

    Ok(json!({
        "path": out_path.to_string_lossy(),
        "format": format,
        "paperSize": format!("{paper_width}x{paper_height}in"),
        "printBackground": print_bg,
        "byteLength": byte_len,
    }))
}
