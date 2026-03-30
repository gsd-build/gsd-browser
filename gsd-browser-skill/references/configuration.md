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
path = "/usr/bin/chromium"
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

All config values can be overridden with `GSD_BROWSER_` prefix. Use `__` as section separator:

| Variable | Equivalent config |
|----------|------------------|
| `GSD_BROWSER_BROWSER__PATH` | `browser.path` |
| `GSD_BROWSER_BROWSER__HEADLESS` | `browser.headless` |
| `GSD_BROWSER_DAEMON__PORT` | `daemon.port` |
| `GSD_BROWSER_DAEMON__HOST` | `daemon.host` |
| `GSD_BROWSER_SCREENSHOT__QUALITY` | `screenshot.quality` |
| `GSD_BROWSER_SCREENSHOT__FORMAT` | `screenshot.format` |
| `GSD_BROWSER_SETTLE__TIMEOUT_MS` | `settle.timeout_ms` |
| `GSD_BROWSER_ARTIFACTS__DIR` | `artifacts.dir` |
| `GSD_BROWSER_VAULT_KEY` | Vault encryption key (no section separator) |

</environment_variables>

<config_locations>

| Location | Purpose |
|----------|---------|
| `~/.gsd-browser/config.toml` | User-level defaults (browser path, preferences) |
| `./gsd-browser.toml` | Project-level settings (committed to repo) |

User config is for machine-specific settings (browser path). Project config is for shared settings (screenshot quality, timeouts).

</config_locations>
