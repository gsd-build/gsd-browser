//! Configuration loading with 5-layer merge precedence:
//!
//! 1. Compiled defaults (matching all current hardcoded values)
//! 2. User config: `~/.gsd-browser/config.toml`
//! 3. Project config: `./gsd-browser.toml`
//! 4. Environment variables: `GSD_BROWSER_*`
//! 5. CLI flags (applied by caller after loading)

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Top-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub browser: BrowserConfig,
    pub daemon: DaemonConfig,
    pub screenshot: ScreenshotConfig,
    pub settle: SettleConfig,
    pub logs: LogsConfig,
    pub artifacts: ArtifactsConfig,
    pub timeline: TimelineConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            browser: BrowserConfig::default(),
            daemon: DaemonConfig::default(),
            screenshot: ScreenshotConfig::default(),
            settle: SettleConfig::default(),
            logs: LogsConfig::default(),
            artifacts: ArtifactsConfig::default(),
            timeline: TimelineConfig::default(),
        }
    }
}

/// Browser launch configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    /// Path to Chrome/Chromium binary. None = auto-detect.
    pub path: Option<String>,
    /// Extra arguments passed to Chrome on launch.
    pub args: Vec<String>,
    /// Whether to launch headless (default: false — visible by default).
    pub headless: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            path: None,
            args: Vec::new(),
            headless: false,
        }
    }
}

/// Daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    /// Host to bind the daemon to (unused for now — unix socket only).
    pub host: String,
    /// Port for future TCP mode.
    pub port: u16,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9222,
        }
    }
}

/// Screenshot defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenshotConfig {
    /// JPEG quality 1-100 (default: 80).
    pub quality: u32,
    /// Default image format: "jpeg" or "png".
    pub format: String,
    /// Capture full scrollable page by default.
    pub full_page: bool,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            quality: 80,
            format: "jpeg".to_string(),
            full_page: false,
        }
    }
}

/// Adaptive DOM settle options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SettleConfig {
    /// Maximum time to wait for DOM to settle (ms).
    pub timeout_ms: u64,
    /// Poll interval during settle (ms).
    pub poll_ms: u64,
    /// Quiet window required for "settled" determination (ms).
    pub quiet_window_ms: u64,
}

impl Default for SettleConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 500,
            poll_ms: 40,
            quiet_window_ms: 100,
        }
    }
}

/// Log buffer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogsConfig {
    /// Maximum number of entries per log buffer.
    pub max_buffer_size: usize,
}

impl Default for LogsConfig {
    fn default() -> Self {
        Self {
            max_buffer_size: 1000,
        }
    }
}

/// Artifact output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtifactsConfig {
    /// Base directory for artifacts (screenshots, PDFs, traces, etc.).
    /// Default: `~/.gsd-browser/artifacts`
    pub dir: Option<String>,
}

impl Default for ArtifactsConfig {
    fn default() -> Self {
        Self { dir: None }
    }
}

/// Timeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TimelineConfig {
    /// Whether to record the action timeline (default: true).
    pub enabled: bool,
    /// Maximum timeline entries before oldest are dropped.
    pub max_entries: usize,
}

impl Default for TimelineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 500,
        }
    }
}

// ── Loading ──

impl Config {
    /// Load configuration with 5-layer merge (CLI flags applied by caller):
    ///
    /// 1. Compiled defaults
    /// 2. User config: `~/.gsd-browser/config.toml`
    /// 3. Project config: `./gsd-browser.toml`
    /// 4. `GSD_BROWSER_*` environment variables
    ///
    /// Returns the default config if no files exist and no env vars are set.
    /// Logs warnings on parse errors but never panics.
    pub fn load() -> Self {
        let mut config = Config::default();

        // Layer 2: User config
        if let Some(home) = dirs::home_dir() {
            let user_path = home.join(".gsd-browser").join("config.toml");
            config.merge_file(&user_path, "user");
        }

        // Layer 3: Project config
        let project_path = PathBuf::from("gsd-browser.toml");
        config.merge_file(&project_path, "project");

        // Layer 4: Environment variable overrides
        config.apply_env_overrides();

        config
    }

    /// Merge a TOML config file into the current config.
    /// Only fields present in the file overwrite the current values.
    fn merge_file(&mut self, path: &Path, label: &str) {
        let contents = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!(
                        "[config] failed to read {} config at {:?}: {}",
                        label, path, e
                    );
                }
                return;
            }
        };

        debug!("[config] loading {} config from {:?}", label, path);

        // Parse into an intermediate TOML table, then selectively overwrite.
        let table: toml::Value = match toml::from_str(&contents) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "[config] failed to parse {} config at {:?}: {}",
                    label, path, e
                );
                return;
            }
        };

        // Parse the file into a partial Config and merge non-default fields.
        // Using serde with #[serde(default)] means missing fields get defaults,
        // so we use the raw TOML table to know which sections were actually specified.
        let partial: Config = match toml::from_str(&contents) {
            Ok(c) => c,
            Err(e) => {
                warn!("[config] failed to deserialize {} config: {}", label, e);
                return;
            }
        };

        // Merge each top-level section only if it was present in the file.
        if table.get("browser").is_some() {
            self.browser = partial.browser;
        }
        if table.get("daemon").is_some() {
            self.daemon = partial.daemon;
        }
        if table.get("screenshot").is_some() {
            self.screenshot = partial.screenshot;
        }
        if table.get("settle").is_some() {
            self.settle = partial.settle;
        }
        if table.get("logs").is_some() {
            self.logs = partial.logs;
        }
        if table.get("artifacts").is_some() {
            self.artifacts = partial.artifacts;
        }
        if table.get("timeline").is_some() {
            self.timeline = partial.timeline;
        }

        debug!("[config] merged {} config successfully", label);
    }

    /// Apply `GSD_BROWSER_*` environment variable overrides.
    ///
    /// Naming convention: `GSD_BROWSER_<SECTION>_<FIELD>` in SCREAMING_SNAKE_CASE.
    /// Example: `GSD_BROWSER_SCREENSHOT_QUALITY=50`
    fn apply_env_overrides(&mut self) {
        // Browser
        if let Ok(v) = std::env::var("GSD_BROWSER_BROWSER_PATH") {
            self.browser.path = Some(v);
        }
        if let Ok(v) = std::env::var("GSD_BROWSER_BROWSER_HEADLESS") {
            if let Ok(b) = v.parse::<bool>() {
                self.browser.headless = b;
            }
        }

        // Screenshot
        if let Ok(v) = std::env::var("GSD_BROWSER_SCREENSHOT_QUALITY") {
            if let Ok(q) = v.parse::<u32>() {
                self.screenshot.quality = q;
            }
        }
        if let Ok(v) = std::env::var("GSD_BROWSER_SCREENSHOT_FORMAT") {
            self.screenshot.format = v;
        }
        if let Ok(v) = std::env::var("GSD_BROWSER_SCREENSHOT_FULL_PAGE") {
            if let Ok(b) = v.parse::<bool>() {
                self.screenshot.full_page = b;
            }
        }

        // Settle
        if let Ok(v) = std::env::var("GSD_BROWSER_SETTLE_TIMEOUT_MS") {
            if let Ok(n) = v.parse::<u64>() {
                self.settle.timeout_ms = n;
            }
        }
        if let Ok(v) = std::env::var("GSD_BROWSER_SETTLE_POLL_MS") {
            if let Ok(n) = v.parse::<u64>() {
                self.settle.poll_ms = n;
            }
        }
        if let Ok(v) = std::env::var("GSD_BROWSER_SETTLE_QUIET_WINDOW_MS") {
            if let Ok(n) = v.parse::<u64>() {
                self.settle.quiet_window_ms = n;
            }
        }

        // Logs
        if let Ok(v) = std::env::var("GSD_BROWSER_LOGS_MAX_BUFFER_SIZE") {
            if let Ok(n) = v.parse::<usize>() {
                self.logs.max_buffer_size = n;
            }
        }

        // Artifacts
        if let Ok(v) = std::env::var("GSD_BROWSER_ARTIFACTS_DIR") {
            self.artifacts.dir = Some(v);
        }

        // Daemon
        if let Ok(v) = std::env::var("GSD_BROWSER_DAEMON_HOST") {
            self.daemon.host = v;
        }
        if let Ok(v) = std::env::var("GSD_BROWSER_DAEMON_PORT") {
            if let Ok(n) = v.parse::<u16>() {
                self.daemon.port = n;
            }
        }

        // Timeline
        if let Ok(v) = std::env::var("GSD_BROWSER_TIMELINE_ENABLED") {
            if let Ok(b) = v.parse::<bool>() {
                self.timeline.enabled = b;
            }
        }
        if let Ok(v) = std::env::var("GSD_BROWSER_TIMELINE_MAX_ENTRIES") {
            if let Ok(n) = v.parse::<usize>() {
                self.timeline.max_entries = n;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    // Env var tests must be serialized — env is process-global.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn default_config_preserves_hardcoded_values() {
        let config = Config::default();

        // SettleOptions: timeout_ms:500, poll_ms:40, quiet_window_ms:100
        assert_eq!(config.settle.timeout_ms, 500);
        assert_eq!(config.settle.poll_ms, 40);
        assert_eq!(config.settle.quiet_window_ms, 100);

        // LogBuffer: MAX_BUFFER_SIZE = 1000
        assert_eq!(config.logs.max_buffer_size, 1000);

        // Screenshot quality: 80
        assert_eq!(config.screenshot.quality, 80);
        assert_eq!(config.screenshot.format, "jpeg");
        assert_eq!(config.screenshot.full_page, false);

        // Browser
        assert!(config.browser.path.is_none());
        assert!(!config.browser.headless);

        // Timeline
        assert!(config.timeline.enabled);
        assert_eq!(config.timeline.max_entries, 500);
    }

    #[test]
    fn default_config_roundtrip_toml() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.settle.timeout_ms, 500);
        assert_eq!(parsed.screenshot.quality, 80);
        assert_eq!(parsed.logs.max_buffer_size, 1000);
    }

    #[test]
    fn merge_file_overrides_specified_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[screenshot]
quality = 50
format = "png"

[settle]
timeout_ms = 1000
"#
        )
        .unwrap();

        let mut config = Config::default();
        config.merge_file(&path, "test");

        // Overridden values
        assert_eq!(config.screenshot.quality, 50);
        assert_eq!(config.screenshot.format, "png");
        assert_eq!(config.settle.timeout_ms, 1000);

        // Settle defaults for fields not specified in the section
        // (serde default fills them since the section was present)
        assert_eq!(config.settle.poll_ms, 40);
        assert_eq!(config.settle.quiet_window_ms, 100);

        // Sections not in the file remain unchanged
        assert_eq!(config.logs.max_buffer_size, 1000);
        assert!(!config.browser.headless);
    }

    #[test]
    fn merge_file_project_overrides_user() {
        let dir = tempfile::tempdir().unwrap();

        // User config: screenshot quality 50
        let user_path = dir.path().join("user.toml");
        std::fs::write(
            &user_path,
            r#"
[screenshot]
quality = 50

[settle]
timeout_ms = 800
"#,
        )
        .unwrap();

        // Project config: screenshot quality 90 (should win)
        let project_path = dir.path().join("project.toml");
        std::fs::write(
            &project_path,
            r#"
[screenshot]
quality = 90
"#,
        )
        .unwrap();

        let mut config = Config::default();
        config.merge_file(&user_path, "user");
        config.merge_file(&project_path, "project");

        // Project's screenshot wins
        assert_eq!(config.screenshot.quality, 90);
        // User's settle still applies (project didn't specify it)
        assert_eq!(config.settle.timeout_ms, 800);
    }

    #[test]
    fn env_var_overrides() {
        let _lock = ENV_LOCK.lock().unwrap();

        // Set env vars
        std::env::set_var("GSD_BROWSER_SCREENSHOT_QUALITY", "42");
        std::env::set_var("GSD_BROWSER_SETTLE_TIMEOUT_MS", "2000");
        std::env::set_var("GSD_BROWSER_LOGS_MAX_BUFFER_SIZE", "500");
        std::env::set_var("GSD_BROWSER_BROWSER_PATH", "/usr/bin/chromium");

        let mut config = Config::default();
        config.apply_env_overrides();

        assert_eq!(config.screenshot.quality, 42);
        assert_eq!(config.settle.timeout_ms, 2000);
        assert_eq!(config.logs.max_buffer_size, 500);
        assert_eq!(config.browser.path.as_deref(), Some("/usr/bin/chromium"));

        // Clean up
        std::env::remove_var("GSD_BROWSER_SCREENSHOT_QUALITY");
        std::env::remove_var("GSD_BROWSER_SETTLE_TIMEOUT_MS");
        std::env::remove_var("GSD_BROWSER_LOGS_MAX_BUFFER_SIZE");
        std::env::remove_var("GSD_BROWSER_BROWSER_PATH");
    }

    #[test]
    fn env_var_invalid_value_ignored() {
        let _lock = ENV_LOCK.lock().unwrap();

        std::env::set_var("GSD_BROWSER_SCREENSHOT_QUALITY", "not_a_number");
        let mut config = Config::default();
        config.apply_env_overrides();
        // Should remain at default because parse failed
        assert_eq!(config.screenshot.quality, 80);
        std::env::remove_var("GSD_BROWSER_SCREENSHOT_QUALITY");
    }

    #[test]
    fn missing_file_returns_defaults() {
        let mut config = Config::default();
        config.merge_file(Path::new("/nonexistent/config.toml"), "test");
        assert_eq!(config.settle.timeout_ms, 500);
        assert_eq!(config.screenshot.quality, 80);
    }

    #[test]
    fn invalid_toml_warns_but_continues() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "this is not valid = [toml syntax").unwrap();

        let mut config = Config::default();
        config.merge_file(&path, "test");
        // Should remain at defaults
        assert_eq!(config.settle.timeout_ms, 500);
    }

    #[test]
    fn full_five_layer_merge() {
        let _lock = ENV_LOCK.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();

        // Layer 2: User config
        let user_path = dir.path().join("user.toml");
        std::fs::write(
            &user_path,
            r#"
[screenshot]
quality = 50

[settle]
timeout_ms = 800
poll_ms = 60
"#,
        )
        .unwrap();

        // Layer 3: Project config — overrides screenshot quality
        let project_path = dir.path().join("project.toml");
        std::fs::write(
            &project_path,
            r#"
[screenshot]
quality = 70
"#,
        )
        .unwrap();

        // Start with defaults (layer 1)
        let mut config = Config::default();
        // Layer 2
        config.merge_file(&user_path, "user");
        // Layer 3
        config.merge_file(&project_path, "project");

        // Layer 4: env var overrides settle timeout
        std::env::set_var("GSD_BROWSER_SETTLE_TIMEOUT_MS", "1500");
        config.apply_env_overrides();

        // Final result:
        // screenshot.quality = 70 (project wins over user's 50)
        assert_eq!(config.screenshot.quality, 70);
        // settle.timeout_ms = 1500 (env wins over user's 800)
        assert_eq!(config.settle.timeout_ms, 1500);
        // settle.poll_ms = 60 (user's override; project didn't touch settle)
        assert_eq!(config.settle.poll_ms, 60);
        // settle.quiet_window_ms = 100 (untouched default)
        assert_eq!(config.settle.quiet_window_ms, 100);
        // logs.max_buffer_size = 1000 (untouched default)
        assert_eq!(config.logs.max_buffer_size, 1000);

        std::env::remove_var("GSD_BROWSER_SETTLE_TIMEOUT_MS");
    }
}
