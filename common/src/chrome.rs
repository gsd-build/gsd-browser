use std::path::PathBuf;

/// Discover Chrome/Chromium binary path.
///
/// Priority:
/// 1. Explicit override (--browser-path)
/// 2. Platform-specific standard paths
/// 3. PATH search via `which`
pub fn find_chrome(override_path: Option<&str>) -> Result<PathBuf, String> {
    // 1. Explicit override
    if let Some(path) = override_path {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        return Err(format!(
            "Chrome not found at specified path: {}. Verify the path exists.",
            path
        ));
    }

    // 2. Platform-specific standard paths
    let platform_paths: &[&str] = if cfg!(target_os = "macos") {
        &["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"]
    } else if cfg!(target_os = "linux") {
        &[
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium-browser",
            "/usr/bin/chromium",
            "/snap/bin/chromium",
        ]
    } else {
        &[]
    };

    for path in platform_paths {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 3. PATH search
    let candidates = ["google-chrome", "google-chrome-stable", "chromium-browser", "chromium"];
    for name in &candidates {
        if let Ok(path) = which::which(name) {
            return Ok(path);
        }
    }

    Err(
        "Chrome not found. Install Google Chrome or pass --browser-path /path/to/chrome".to_string(),
    )
}
