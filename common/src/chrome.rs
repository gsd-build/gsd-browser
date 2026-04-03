use std::path::PathBuf;

/// Discover Chrome/Chromium binary path.
///
/// Priority:
/// 1. Explicit override (--browser-path)
/// 2. Platform-specific standard Chrome paths
/// 3. PATH search via `which` (platform-appropriate candidates)
/// 4. Windows-only: Edge fallback (strict fallback, only after all Chrome options exhausted)
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

    // 2. Platform-specific standard Chrome paths
    let chrome_paths: Vec<String> = if cfg!(target_os = "macos") {
        vec!["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".to_string()]
    } else if cfg!(target_os = "linux") {
        vec![
            "/usr/bin/google-chrome".to_string(),
            "/usr/bin/google-chrome-stable".to_string(),
            "/usr/bin/chromium-browser".to_string(),
            "/usr/bin/chromium".to_string(),
            "/snap/bin/chromium".to_string(),
        ]
    } else if cfg!(target_os = "windows") {
        let mut paths = Vec::new();
        if let Ok(pf) = std::env::var("ProgramFiles") {
            paths.push(format!(r"{}\Google\Chrome\Application\chrome.exe", pf));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            paths.push(format!(r"{}\Google\Chrome\Application\chrome.exe", pf86));
        }
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            paths.push(format!(r"{}\Google\Chrome\Application\chrome.exe", localappdata));
        }
        paths
    } else {
        vec![]
    };

    for path in &chrome_paths {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 3. PATH search for Chrome (platform-appropriate candidates)
    let chrome_candidates: &[&str] = if cfg!(target_os = "windows") {
        &["chrome.exe"]
    } else {
        &["google-chrome", "google-chrome-stable", "chromium-browser", "chromium"]
    };

    for name in chrome_candidates {
        if let Ok(path) = which::which(name) {
            return Ok(path);
        }
    }

    // 4. Windows-only: Edge fallback (tried AFTER all Chrome options exhausted)
    if cfg!(target_os = "windows") {
        let mut edge_paths = Vec::new();
        if let Ok(pf) = std::env::var("ProgramFiles") {
            edge_paths.push(format!(r"{}\Microsoft\Edge\Application\msedge.exe", pf));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            edge_paths.push(format!(r"{}\Microsoft\Edge\Application\msedge.exe", pf86));
        }
        for path in &edge_paths {
            let p = PathBuf::from(path);
            if p.exists() {
                return Ok(p);
            }
        }
        // Also try which for Edge
        if let Ok(path) = which::which("msedge.exe") {
            return Ok(path);
        }
    }

    // 5. Not found
    let msg = if cfg!(target_os = "windows") {
        "Chrome not found. Install Google Chrome or Microsoft Edge, or pass --browser-path C:\\path\\to\\chrome.exe"
    } else {
        "Chrome not found. Install Google Chrome or pass --browser-path /path/to/chrome"
    };
    Err(msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_path_nonexistent_returns_error() {
        let result = find_chrome(Some("/nonexistent/chrome"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found at specified path"));
    }

    #[test]
    fn override_path_existing_returns_path() {
        // Use the current executable as a stand-in for an existing path
        let exe = std::env::current_exe().unwrap();
        let exe_str = exe.to_str().unwrap();
        let result = find_chrome(Some(exe_str));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), exe);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_finds_chrome_or_edge() {
        // On a real Windows machine, at least Chrome or Edge should be found
        let result = find_chrome(None);
        // This may fail on minimal CI without browsers, so just check it doesn't panic
        match result {
            Ok(path) => {
                let path_str = path.to_string_lossy().to_lowercase();
                assert!(
                    path_str.contains("chrome") || path_str.contains("edge"),
                    "Expected chrome or edge in path, got: {}",
                    path.display()
                );
            }
            Err(msg) => {
                assert!(msg.contains("Chrome not found"), "Unexpected error: {}", msg);
            }
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_chrome_before_edge() {
        // If Chrome exists, it should be returned (not Edge)
        let chrome_path = std::env::var("ProgramFiles")
            .map(|pf| format!(r"{}\Google\Chrome\Application\chrome.exe", pf))
            .unwrap_or_default();
        if std::path::Path::new(&chrome_path).exists() {
            let result = find_chrome(None).unwrap();
            let result_str = result.to_string_lossy().to_lowercase();
            assert!(result_str.contains("chrome"), "Expected Chrome, got Edge: {}", result.display());
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_candidates_unchanged() {
        // Verify Linux still works -- find_chrome should not error on platform path logic
        // (may fail to find Chrome if not installed, but should not panic)
        let _ = find_chrome(None);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_candidates_unchanged() {
        let _ = find_chrome(None);
    }
}
