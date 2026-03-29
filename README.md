# browser-tools

A fast, native browser automation CLI powered by Chrome DevTools Protocol. 63 commands covering navigation, interaction, screenshots, accessibility, network mocking, visual diffing, test generation, and more — all from a single binary.

Built in Rust for speed and reliability. Designed for AI agents, CI pipelines, and developers who need programmatic browser control without the overhead of a full testing framework.

## Install

### npm (recommended)

```bash
npm install -g @gsd-build/browser-tools
```

### Cargo (from source)

```bash
cargo install browser-tools
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/anthropics/browser-tools/releases) for your platform:

| Platform | Binary |
|----------|--------|
| macOS (Apple Silicon) | `browser-tools-darwin-arm64` |
| macOS (Intel) | `browser-tools-darwin-x64` |
| Linux (ARM64) | `browser-tools-linux-arm64` |
| Linux (x64) | `browser-tools-linux-x64` |
| Windows (x64) | `browser-tools-win-x64.exe` |

## Quick Start

```bash
# Navigate to a page
browser-tools navigate https://example.com

# Take a screenshot
browser-tools screenshot --output page.png --format png

# Get the accessibility tree
browser-tools accessibility-tree

# Click a button by CSS selector
browser-tools click --selector "button.submit"

# Extract structured data
browser-tools extract --schema '{"title": {"_selector": "h1"}}'
```

## Features

### 63 Commands in 15 Categories

| Category | Commands |
|----------|----------|
| **Navigation** | `navigate`, `back`, `forward`, `reload` |
| **Console & Network** | `console`, `network`, `dialog` |
| **JavaScript** | `eval` |
| **Interaction** | `click`, `type`, `press`, `hover`, `scroll`, `select-option`, `set-checked`, `drag`, `set-viewport`, `upload-file` |
| **Screenshots & Visual** | `screenshot`, `zoom-region`, `visual-diff`, `save-pdf` |
| **Accessibility** | `accessibility-tree`, `find`, `page-source` |
| **Waits & Timing** | `wait-for` |
| **Timeline & Debug** | `timeline`, `session-summary`, `debug-bundle` |
| **Refs (Element Tracking)** | `snapshot`, `click-ref`, `hover-ref`, `fill-ref`, `get-ref` |
| **Semantic Actions** | `find-best`, `act` |
| **Forms** | `analyze-form`, `fill-form` |
| **Data Extraction** | `extract` |
| **Network Mocking** | `mock-route`, `block-urls`, `clear-routes` |
| **Device & State** | `emulate-device`, `save-state`, `restore-state` |
| **Auth Vault** | `vault-save`, `vault-login`, `vault-list` |
| **Test & Tracing** | `generate-test`, `action-cache`, `check-injection`, `har-export`, `trace-start`, `trace-stop` |
| **Pages & Frames** | `list-pages`, `switch-page`, `close-page`, `list-frames`, `select-frame` |
| **Daemon** | `daemon start`, `daemon stop`, `daemon health` |

### Key Differentiators

- **Single binary** — no Node.js runtime, no browser driver downloads
- **63 commands** — covers the full browser automation surface
- **Deterministic element refs** — `snapshot` assigns versioned refs (`@v1:e1`) for stable interaction
- **Semantic actions** — `find-best` and `act` resolve intent to elements automatically
- **Network mocking** — intercept requests, block URLs, export HAR files
- **Visual regression** — pixel-level diffing against baselines
- **Encrypted auth vault** — save and replay login credentials securely
- **Test generation** — record actions and export as Playwright tests
- **AI agent native** — JSON output mode (`--json`), SKILL.md for agent discovery

## Configuration

browser-tools uses a 5-layer configuration merge:

1. **Built-in defaults** — sensible values for all settings
2. **User config** — `~/.browser-tools/config.toml`
3. **Project config** — `./browser-tools.toml` in your project root
4. **Environment variables** — `BROWSER_TOOLS_*` prefix
5. **CLI flags** — highest priority, override everything

Example `browser-tools.toml`:

```toml
[browser]
path = "/usr/bin/chromium"
headless = true

[daemon]
port = 9222
host = "127.0.0.1"

[screenshot]
quality = 90
full_page = false

[settle]
timeout_ms = 500
poll_ms = 40
quiet_window_ms = 100
```

Environment variable override example:

```bash
export BROWSER_TOOLS_BROWSER_PATH=/usr/bin/chromium
export BROWSER_TOOLS_DAEMON_PORT=9333
```

## Architecture

```
┌─────────────┐     IPC/HTTP      ┌──────────┐      CDP       ┌─────────┐
│  CLI (clap)  │ ───────────────→ │  Daemon   │ ────────────→ │ Chrome  │
│  browser-    │ ←─────────────── │  (tokio)  │ ←──────────── │  (CDP)  │
│  tools       │   JSON response  │           │   events       │         │
└─────────────┘                   └──────────┘                └─────────┘
       │                                │
       │ --json flag                    │ manages
       ▼                                ▼
  Structured                     Browser lifecycle
  JSON output                    Session isolation
  for agents                     Page/frame routing
```

The **CLI** parses commands and delegates to the **daemon** process over a local HTTP channel. The daemon maintains a persistent connection to Chrome via CDP (Chrome DevTools Protocol), manages browser lifecycle, and routes commands to the correct page/frame context. The daemon starts automatically on first use and persists across commands for fast execution.

## JSON Output

Every command supports `--json` for structured output:

```bash
browser-tools navigate https://example.com --json
# {"title":"Example Domain","url":"https://example.com/","status":"ok"}

browser-tools find --text "More information" --json
# {"elements":[{"role":"link","name":"More information...","selector":"a"}]}
```

## For AI Agents

browser-tools ships with `SKILL.md` and `AGENTS.md` for automatic agent discovery. Agents can:

1. Read `SKILL.md` for complete command reference and workflow patterns
2. Use `--json` output for structured data parsing
3. Use `snapshot` → `click-ref`/`fill-ref` for deterministic element interaction
4. Use `find-best`/`act` for semantic intent resolution
5. Use `wait-for` to handle async state changes reliably

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
