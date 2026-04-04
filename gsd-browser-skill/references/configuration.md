<overview>
gsd-browser uses a 5-layer configuration merge. Higher layers override lower ones.
</overview>

<merge_precedence>

1. **Compiled defaults** — sensible values for all settings
2. **User config** — `~/.gsd-browser/config.toml`
3. **Project config** — `./gsd-browser.toml` in project root
4. **Environment variables** — `GSD_BROWSER_*` prefix
5. **CLI flags** — highest priority, override everything

</merge_precedence>

<config_file_format>

```toml
[browser]
# macOS / Linux
path = "/usr/bin/chromium"
# Windows (uncomment one)
# path = "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"
# path = "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe"
headless = true

[daemon]
port = 9222
host = "127.0.0.1"

[screenshot]
quality = 90
format = "png"
full_page = false

[settle]
timeout_ms = 500
poll_ms = 40
quiet_window_ms = 100

[logs]
max_buffer_size = 1000

[artifacts]
dir = "./browser-artifacts"

[timeline]
max_entries = 500
```

</config_file_format>

<environment_variables>

Supported environment variable overrides use `GSD_BROWSER_<SECTION>_<FIELD>` naming:

| Variable | Equivalent config |
|----------|------------------|
| `GSD_BROWSER_BROWSER_PATH` | `browser.path` |
| `GSD_BROWSER_BROWSER_HEADLESS` | `browser.headless` |
| `GSD_BROWSER_DAEMON_PORT` | `daemon.port` |
| `GSD_BROWSER_DAEMON_HOST` | `daemon.host` |
| `GSD_BROWSER_SCREENSHOT_QUALITY` | `screenshot.quality` |
| `GSD_BROWSER_SCREENSHOT_FORMAT` | `screenshot.format` |
| `GSD_BROWSER_SCREENSHOT_FULL_PAGE` | `screenshot.full_page` |
| `GSD_BROWSER_SETTLE_TIMEOUT_MS` | `settle.timeout_ms` |
| `GSD_BROWSER_SETTLE_POLL_MS` | `settle.poll_ms` |
| `GSD_BROWSER_SETTLE_QUIET_WINDOW_MS` | `settle.quiet_window_ms` |
| `GSD_BROWSER_LOGS_MAX_BUFFER_SIZE` | `logs.max_buffer_size` |
| `GSD_BROWSER_ARTIFACTS_DIR` | `artifacts.dir` |
| `GSD_BROWSER_TIMELINE_ENABLED` | `timeline.enabled` |
| `GSD_BROWSER_TIMELINE_MAX_ENTRIES` | `timeline.max_entries` |
| `GSD_BROWSER_VAULT_KEY` | Vault encryption key |

</environment_variables>

<config_locations>

| Location | Purpose |
|----------|---------|
| `~/.gsd-browser/config.toml` | User-level defaults (browser path, preferences) |
| `./gsd-browser.toml` | Project-level settings (committed to repo) |

User config is for machine-specific settings (browser path). Project config is for shared settings (screenshot quality, timeouts).

</config_locations>
