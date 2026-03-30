# gsd-browser

Native Rust browser automation CLI for Chrome/Chromium via CDP. `gsd-browser` keeps a persistent background daemon, auto-starts on first use, and exposes 63 top-level commands for navigation, interaction, snapshots with versioned refs, assertions, structured extraction, network control, visual diffing, tracing, and stateful auth flows.

Built for AI agents, CI pipelines, and developers who want deterministic browser control without adopting a full browser test framework.

## Install

### Recommended: installer (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/gsd-build/gsd-browser/main/install.sh | bash
```

The installer downloads the correct `gsd-browser` binary for your platform. If Chrome or Chromium is already installed, it uses that. Otherwise it downloads Chromium automatically when Chrome for Testing is available for your platform.

### Pre-built binaries

Download from [GitHub Releases](https://github.com/gsd-build/gsd-browser/releases):

| Platform | Asset |
|----------|-------|
| macOS (Apple Silicon) | `gsd-browser-darwin-arm64` |
| macOS (Intel) | `gsd-browser-darwin-x64` |
| Linux (ARM64) | `gsd-browser-linux-arm64` |
| Linux (x64) | `gsd-browser-linux-x64` |

### Build from source

```bash
git clone https://github.com/gsd-build/gsd-browser.git
cd gsd-browser
cargo install --path cli
```

### Package registries

The public npm package (`@gsd-build/gsd-browser`) and crates.io package (`gsd-browser`) are not published yet. Use the installer, GitHub release assets, or a source build.

## Quick Start

The daemon starts automatically on first use.

```bash
# Navigate to a page
gsd-browser navigate https://example.com

# Snapshot interactive elements and assign refs like @v1:e1
gsd-browser snapshot

# On example.com the only interactive element is the "More information..." link
gsd-browser click-ref @v1:e1

# Wait for navigation and assert the result
gsd-browser wait-for --condition network_idle
gsd-browser assert --checks '[{"kind":"url_contains","text":"iana.org"}]'

# Capture a PNG
gsd-browser screenshot --output page.png --format png
```

## Command Surface

`gsd-browser` currently exposes 63 top-level commands:

| Area | Commands |
|------|----------|
| Navigation | `navigate`, `back`, `forward`, `reload` |
| Logs & JavaScript | `console`, `network`, `dialog`, `eval` |
| Interaction | `click`, `type`, `press`, `hover`, `scroll`, `select-option`, `set-checked`, `drag`, `set-viewport`, `upload-file` |
| Inspection | `accessibility-tree`, `find`, `page-source` |
| Waits | `wait-for` |
| Snapshots & refs | `snapshot`, `get-ref`, `click-ref`, `hover-ref`, `fill-ref` |
| Assertions & batching | `assert`, `diff`, `batch` |
| Pages & frames | `list-pages`, `switch-page`, `close-page`, `list-frames`, `select-frame` |
| Forms & semantic actions | `analyze-form`, `fill-form`, `find-best`, `act` |
| Diagnostics | `timeline`, `session-summary`, `debug-bundle` |
| Screenshots & document output | `screenshot`, `zoom-region`, `save-pdf` |
| Visual regression | `visual-diff` |
| Structured extraction | `extract` |
| Network control | `mock-route`, `block-urls`, `clear-routes` |
| Device & browser state | `emulate-device`, `save-state`, `restore-state` |
| Auth vault | `vault-save`, `vault-login`, `vault-list` |
| Recording & traces | `generate-test`, `har-export`, `trace-start`, `trace-stop` |
| Safety, caching & daemon management | `action-cache`, `check-injection`, `daemon` |

## Highlights

- Persistent daemon with automatic startup for fast repeated commands
- Versioned refs from `snapshot` for deterministic interaction (`@v1:e1`, `@v2:e3`)
- Explicit assertions with `assert` and multi-step automation with `batch`
- Semantic `find-best` and `act` flows covering 15 built-in intents
- Named sessions via `--session` for isolated parallel browser workers
- Structured JSON output on every command via `--json`
- Visual diffing, HAR export, PDF generation, and CDP tracing in the same tool
- Saved browser state plus encrypted credential replay through the auth vault
- Prompt injection scanning for agent-facing browsing workflows

## Configuration

`gsd-browser` merges configuration in this order:

1. Built-in defaults
2. User config: `~/.gsd-browser/config.toml`
3. Project config: `./gsd-browser.toml`
4. Environment variables: `GSD_BROWSER_*`
5. CLI flags

Example `gsd-browser.toml`:

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
enabled = true
max_entries = 500
```

Supported environment variable overrides use `GSD_BROWSER_<SECTION>_<FIELD>` naming:

```bash
export GSD_BROWSER_BROWSER_PATH=/usr/bin/chromium
export GSD_BROWSER_BROWSER_HEADLESS=true
export GSD_BROWSER_DAEMON_PORT=9333
export GSD_BROWSER_SCREENSHOT_QUALITY=90
export GSD_BROWSER_SETTLE_TIMEOUT_MS=1000
export GSD_BROWSER_ARTIFACTS_DIR=./browser-artifacts
export GSD_BROWSER_VAULT_KEY=your-encryption-key
```

## How It Works

- The CLI parses commands and sends them to a local daemon over a loopback HTTP channel.
- The daemon maintains the browser lifecycle, page/frame routing, network hooks, and action timeline.
- `--session <name>` creates isolated daemon and browser instances for parallel workflows.

## For AI Agents

- The daemon auto-starts. You almost never need `gsd-browser daemon start`.
- Use `--json` when you need structured output.
- Prefer `snapshot` then `click-ref` or `fill-ref` for stable interaction, and re-snapshot after page changes.
- Use `assert` and `batch` when you need deterministic pass/fail automation.
- `find-best` and `act` cover 15 built-in semantic intents for common navigation, form, dialog, auth, and pagination actions.
- Read [SKILL.md](./SKILL.md) for the full command reference and workflow patterns.

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
