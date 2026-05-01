---
name: gsd-browser
description: >
  Native Rust browser automation CLI for AI agents. Use when the user needs to
  interact with websites — navigating pages, filling forms, clicking buttons,
  taking screenshots, sharing a live browser view, narrating browser actions,
  extracting structured data, running assertions, testing web apps, or
  automating any browser task. Triggers include requests to "open a website",
  "fill out a form", "click a button", "take a screenshot", "show me the
  browser", "share the screen", "pause the browser", "step through this",
  "scrape data from a page", "test this web app", "login to a site",
  "automate browser actions", "visual regression test", "check for prompt
  injection", or any task requiring programmatic web interaction.
allowed-tools: Bash(gsd-browser:*), Bash(gsd-browser *)
---

# Browser Automation with gsd-browser

## Critical Rules

1. **The daemon auto-starts on browser commands.** `daemon health` only reports state; it does not start a session. Use `daemon start` only when you want to pre-warm or verify daemon lifecycle explicitly.
2. **Always re-snapshot after page changes.** Refs are versioned (`@v1:e1`). After navigation, form submission, or dynamic content loading, old refs are stale. Run `gsd-browser snapshot` to get fresh refs.
3. **Use `--json` when parsing output.** Use text mode when reading output yourself. Use `--json` when you need to extract values programmatically (e.g., checking assertion results, parsing snapshot refs).
4. **Positional args have no flag prefix.** Commands like `click`, `type`, `hover` take positional args — do NOT add `--selector`. See exact syntax in command reference below.
5. **Use `batch` for atomic multi-step flows.** Batch reduces round trips and keeps pass/fail checks in one call. Use separate commands when you need intermediate output (e.g., snapshot to discover refs).
6. **Use `view` when the user wants to watch or direct the browser.** The live viewer is the shared screen and control surface; keep running actions through CLI commands.

## Core Workflow

Every browser automation follows this pattern:

1. **Navigate**: `gsd-browser navigate <url>`
2. **Snapshot**: `gsd-browser snapshot` (get versioned refs like `@v1:e1`, `@v1:e2`)
3. **Interact**: Use refs to click, fill, hover
4. **Re-snapshot**: After navigation or DOM changes, get fresh refs

```bash
gsd-browser navigate https://example.com/form
gsd-browser snapshot
# Output: @v1:e1 [input type="email"], @v1:e2 [input type="password"], @v1:e3 [button] "Submit"

gsd-browser fill-ref @v1:e1 "user@example.com"
gsd-browser fill-ref @v1:e2 "password123"
gsd-browser click-ref @v1:e3
gsd-browser wait-for --condition network_idle
gsd-browser snapshot  # REQUIRED — old refs are now stale
```

## Command Chaining

Commands can be chained with `&&` in a single shell invocation. Browser state also persists across separate invocations through the background daemon when you stay on the same session.

```bash
# Chain navigate + wait + snapshot
gsd-browser navigate https://example.com && gsd-browser wait-for --condition network_idle && gsd-browser snapshot

# Chain multiple interactions
gsd-browser fill-ref @v1:e1 "user@example.com" && gsd-browser fill-ref @v1:e2 "password123" && gsd-browser click-ref @v1:e3
```

**When to chain:** Use `&&` when you don't need intermediate output. Run commands separately when you need to parse output first (e.g., snapshot to discover refs, then interact).

---

## Command Reference

Argument syntax: `<arg>` = required positional, `[arg]` = optional positional, `--flag` = named option. Do NOT add `--` prefix to positional args.

### Navigation

```bash
gsd-browser navigate <url>                        # Navigate to a URL
gsd-browser back                                   # Go back in browser history
gsd-browser forward                                # Go forward in browser history
gsd-browser reload                                 # Reload the current page
```

### Interaction

All selectors are **positional** — do NOT use `--selector`.

```bash
# Click
gsd-browser click <selector>                       # Click by CSS selector
gsd-browser click --x 100 --y 200                  # Click by coordinates (no selector)

# Type — positional: <selector> <text>
gsd-browser type <selector> <text>                 # Atomic fill (replaces content)
gsd-browser type <selector> <text> --slowly        # Character-by-character
gsd-browser type <selector> <text> --clear-first   # Clear field before typing
gsd-browser type <selector> <text> --submit        # Press Enter after typing

# Other interaction — all positional selectors
gsd-browser press <key>                            # Press key: Enter, Escape, Tab, Meta+A
gsd-browser hover <selector>                       # Hover over element
gsd-browser scroll --direction down                # Scroll down 300px (default)
gsd-browser scroll --direction up --amount 500     # Scroll up 500px
gsd-browser select-option <selector> <option>      # Select dropdown by label or value
gsd-browser set-checked <selector> --checked       # Check checkbox/radio (omit --checked to uncheck)
gsd-browser drag <source-selector> <target-selector>  # Drag-and-drop
gsd-browser upload-file <selector> <file>...       # Set files on <input type="file">
gsd-browser set-viewport --preset mobile           # Preset: mobile, tablet, desktop, wide
gsd-browser set-viewport --width 1920 --height 1080  # Custom dimensions
```

### Snapshot & Refs

Refs are versioned (`@v1:e1`, `@v2:e3`). The version increments each snapshot. **Old refs become stale after page changes — always re-snapshot.**

```bash
# Take snapshot
gsd-browser snapshot                               # Snapshot interactive elements, assign refs
gsd-browser snapshot --selector "form"             # Scope to a CSS selector
gsd-browser snapshot --mode <mode>                 # Semantic mode (see below)
gsd-browser snapshot --limit 80                    # Increase element limit (default: 40)

# Use refs — all positional
gsd-browser get-ref <ref>                          # Get metadata for a ref
gsd-browser click-ref <ref>                        # Click element by ref
gsd-browser hover-ref <ref>                        # Hover element by ref
gsd-browser fill-ref <ref> <text>                  # Type into element by ref
```

**Snapshot modes** (`--mode`):

| Mode | What it captures |
|------|-----------------|
| `interactive` | Buttons, inputs, links, selects (default) |
| `form` | Form fields with labels and current values |
| `dialog` | Elements inside open dialogs/modals |
| `navigation` | Links and nav elements |
| `errors` | Error messages, validation warnings |
| `headings` | Heading elements (h1-h6) for page structure |
| `visible_only` | All visible elements regardless of interactivity |

### Inspection

```bash
gsd-browser accessibility-tree                     # Full accessibility tree (roles, names, states)
gsd-browser find --text "Sign In"                  # Find elements by text (case-insensitive)
gsd-browser find --role button                     # Find by ARIA role
gsd-browser find --selector ".my-class"            # Find by CSS selector
gsd-browser find --role link --limit 50            # Increase limit (default: 20)
gsd-browser page-source                            # Get raw HTML of page
gsd-browser page-source --selector "main"          # Scoped HTML source
gsd-browser eval '<js-expression>'                 # Evaluate JavaScript in page context
```

### Assertions

Run explicit pass/fail checks against the current page state. Prefer this over inferring success from output.

```bash
gsd-browser assert --checks '[
  {"kind": "url_contains", "text": "/dashboard"},
  {"kind": "text_visible", "text": "Welcome"},
  {"kind": "selector_visible", "selector": "#user-menu"},
  {"kind": "value_equals", "selector": "input[name=email]", "value": "user@test.com"},
  {"kind": "no_console_errors"},
  {"kind": "no_failed_requests"}
]'
```

**Assertion kinds (17):** `url_contains`, `text_visible`, `text_hidden`, `selector_visible`, `selector_hidden`, `value_equals`, `checked`, `no_console_errors`, `no_failed_requests`, `request_url_seen`, `response_status`, `console_message_matches`, `network_count`, `console_count`, `element_count`, `no_console_errors_since`, `no_failed_requests_since`.

### Batch Execution

Execute multiple steps in one call to reduce round trips. Stops on first failure by default.

```bash
gsd-browser batch --steps '[
  {"action": "navigate", "url": "https://example.com"},
  {"action": "wait_for", "condition": "network_idle"},
  {"action": "click", "selector": "#login-btn"},
  {"action": "type", "selector": "input[name=email]", "text": "user@test.com"},
  {"action": "type", "selector": "input[name=password]", "text": "secret", "submit": true},
  {"action": "assert", "checks": [{"kind": "url_contains", "text": "/dashboard"}]}
]'

# With --summary-only to reduce output
gsd-browser batch --steps '[...]' --summary-only
```

**Batch actions:** `navigate`, `click`, `type`, `key_press`, `wait_for`, `assert`, `click_ref`, `fill_ref`.

### Wait Conditions

```bash
gsd-browser wait-for --condition selector_visible --value "#content"
gsd-browser wait-for --condition selector_hidden --value ".spinner"
gsd-browser wait-for --condition url_contains --value "/dashboard"
gsd-browser wait-for --condition network_idle
gsd-browser wait-for --condition delay --value 2000
gsd-browser wait-for --condition text_visible --value "Success"
gsd-browser wait-for --condition text_hidden --value "Loading"
gsd-browser wait-for --condition request_completed --value "/api/data"
gsd-browser wait-for --condition console_message --value "ready"
gsd-browser wait-for --condition element_count --value ".item" --threshold ">=5"
gsd-browser wait-for --condition region_stable --value "#content"

# Custom timeout (default: 10000ms)
gsd-browser wait-for --condition selector_visible --value "#slow" --timeout 30000
```

### Forms (Smart Fill)

Analyze forms and fill them by field label, name, placeholder, or aria-label — no selectors needed.

```bash
# Analyze form structure
gsd-browser analyze-form
gsd-browser analyze-form --selector "#signup-form"

# Fill by field identifiers (resolved: label -> name -> placeholder -> aria-label)
gsd-browser fill-form --values '{"Email": "a@b.com", "Password": "secret", "Country": "US"}'
gsd-browser fill-form --values '{"Email": "a@b.com"}' --submit  # Fill and click submit
gsd-browser fill-form --values '{"Email": "a@b.com"}' --selector "#login-form"
```

### Intent-Based Interaction

Find and act on elements by semantic intent — no selectors or refs needed. Intents are predefined categories, not free-form text.

```bash
# Find top candidates for an intent (returns scored matches with selectors)
gsd-browser find-best --intent submit_form
gsd-browser find-best --intent accept_cookies
gsd-browser find-best --intent primary_cta --scope "#modal"

# Act: find best match and click/focus it in one call
gsd-browser act --intent submit_form
gsd-browser act --intent accept_cookies
gsd-browser act --intent fill_email
```

**Intents (15):**

| Intent | Action | Description |
|--------|--------|-------------|
| `submit_form` | click | Submit buttons, form actions |
| `close_dialog` | click | Modal/dialog close buttons |
| `primary_cta` | click | Primary call-to-action elements |
| `search_field` | focus | Search inputs and searchboxes |
| `next_step` | click | Next/continue/proceed buttons |
| `dismiss` | click | Dismiss overlays, banners, toasts |
| `auth_action` | click | Login/signup/register buttons |
| `back_navigation` | click | Back/previous navigation links |
| `fill_email` | focus | Email input fields |
| `fill_password` | focus | Password input fields |
| `fill_username` | focus | Username/login input fields |
| `accept_cookies` | click | Cookie consent accept buttons |
| `main_content` | click | Main content area (`<main>`, `<article>`, semantic markup required) |
| `pagination_next` | click | Next page in pagination |
| `pagination_prev` | click | Previous page in pagination |

### Pages & Frames

Page and frame IDs are **positional** — do NOT use `--id`.

```bash
gsd-browser list-pages                             # List all open tabs
gsd-browser switch-page <id>                       # Switch to tab by ID (positional)
gsd-browser close-page <id>                        # Close a tab by ID (positional)

gsd-browser list-frames                            # List all frames (main + iframes)
gsd-browser select-frame --name "iframe-name"      # Select frame by name
gsd-browser select-frame --url-pattern "embed"     # Select frame by URL substring
gsd-browser select-frame --index 0                 # Select by index
gsd-browser select-frame --name main               # Return to main frame
```

### Diagnostics

```bash
gsd-browser console                                # Get console log entries (clears buffer)
gsd-browser console --no-clear                     # Read without clearing
gsd-browser network                                # Get network log entries
gsd-browser dialog                                 # Get dialog events (alert, confirm, prompt)
gsd-browser timeline                               # Query the action timeline
gsd-browser session-summary                        # Diagnostic summary of current session
gsd-browser debug-bundle                           # Full debug bundle: screenshot + logs + timeline + a11y tree
```

### Live Viewer & Narration

The live viewer is a localhost screen-sharing surface for the active browser session. Browser actions still run through CLI commands. The viewer displays live frames, narrated action history, ref overlays, target rings, click ripples, failure markers, and page-following across navigation or tab changes.

```bash
gsd-browser view                                   # Open the live viewer
gsd-browser view --print-only                      # Print URL only
gsd-browser view --history                         # Open history-focused viewer
gsd-browser view --history --print-only            # Print history URL only

gsd-browser goal "Find the checkout button"        # Set viewer goal banner
gsd-browser goal --clear                           # Clear goal banner

gsd-browser pause                                  # Pause before next narrated action
gsd-browser resume                                 # Resume actions
gsd-browser step                                   # Allow one action, then pause
gsd-browser abort                                  # Abort next gated action
```

Use one named session for the whole shared-screen flow:

```bash
gsd-browser --session demo navigate https://example.com
gsd-browser --session demo view --print-only
gsd-browser --session demo click "h1"
```

Viewer controls:

| Control | Effect |
|---------|--------|
| Pause | Blocks before the next narrated action |
| Resume | Allows actions to continue |
| Step | Allows one action, then returns to paused mode |
| Abort | Aborts the next gated action |
| Refs overlay | Shows or hides target boxes/labels |

Keyboard shortcuts: Space pauses/resumes, Right Arrow steps, Escape aborts, `R` toggles refs.

Use `--no-narration-delay` for fast agent-only runs that keep narration events/history without lead-time sleeps:

```bash
gsd-browser --session demo --no-narration-delay click "h1"
```

### Visual

```bash
gsd-browser screenshot                             # Screenshot to stdout (JPEG base64)
gsd-browser screenshot --output page.png           # Write to file
gsd-browser screenshot --format png                # Force PNG format
gsd-browser screenshot --full-page                 # Full scrollable page
gsd-browser screenshot --selector "#hero"          # Crop to element (always PNG)
gsd-browser screenshot --quality 50                # JPEG quality 1-100 (default: 80)

gsd-browser zoom-region --x 100 --y 200 --width 400 --height 300  # Capture region
gsd-browser zoom-region --x 0 --y 0 --width 200 --height 200 --scale 3  # Upscale

gsd-browser save-pdf                               # Save page as PDF (A4 default)
gsd-browser save-pdf --output report.pdf           # Custom output path
gsd-browser save-pdf --format Letter               # Page format: A4, Letter, Legal, Tabloid
```

### Visual Regression

```bash
# First run: saves baseline screenshot
gsd-browser visual-diff --name "homepage"

# Subsequent runs: compare against baseline, report mismatch
gsd-browser visual-diff --name "homepage"
gsd-browser visual-diff --name "homepage" --threshold 0.05   # Stricter tolerance (0-1)
gsd-browser visual-diff --selector "#hero" --name "hero"     # Scope to element
gsd-browser visual-diff --name "homepage" --update-baseline  # Reset baseline
```

### Structured Data Extraction

```bash
# Single item
gsd-browser extract --schema '{
  "type": "object",
  "properties": {
    "title": {"_selector": "h1", "_attribute": "textContent"},
    "price": {"_selector": ".price", "_attribute": "textContent"},
    "image": {"_selector": "img.product", "_attribute": "src"}
  }
}'

# Array of items
gsd-browser extract --selector ".product-card" --multiple --schema '{
  "type": "object",
  "properties": {
    "name": {"_selector": "h3", "_attribute": "textContent"},
    "price": {"_selector": ".price", "_attribute": "textContent"}
  }
}'
```

### Network Mocking

```bash
gsd-browser mock-route --url "**/api/users*" --body '[{"name":"Alice"}]' --status 200
gsd-browser mock-route --url "**/api/data" --body '{"ok":true}' --delay 3000  # Slow response
gsd-browser block-urls "**/analytics*" "**/ads*"   # Block URL patterns (positional)
gsd-browser clear-routes                           # Remove all mocks and blocks
```

### Device Emulation

```bash
gsd-browser emulate-device <device-name>           # Full device emulation (positional)
gsd-browser emulate-device "iPhone 15"
gsd-browser emulate-device "Pixel 7"
gsd-browser emulate-device "iPad Pro 11"
gsd-browser emulate-device list                    # Show all available presets
```

**Warning:** Device emulation recreates the browser context — current page state and cookies are lost.

### State & Auth

```bash
# Save/restore browser state (cookies, localStorage, sessionStorage)
gsd-browser save-state --name "logged-in"
gsd-browser restore-state --name "logged-in"

# Auth vault (encrypted credentials)
gsd-browser vault-save --profile github --url https://github.com/login \
  --username user --password "secret"
gsd-browser vault-login --profile github
gsd-browser vault-list                             # List profiles (no secrets shown)
```

Vault encryption requires `GSD_BROWSER_VAULT_KEY` env var set **before the daemon starts**. If the daemon is already running, stop it first, set the var, then run your vault command.

### Tracing & Recording

```bash
gsd-browser trace-start                            # Start CDP performance trace
gsd-browser trace-start --name "checkout-flow"     # Named trace
gsd-browser trace-stop                             # Stop and write trace to disk
gsd-browser trace-stop --name "checkout.json"      # Custom output filename

gsd-browser har-export                             # Export network as HAR 1.2 JSON
gsd-browser har-export --filename "session.har"    # Custom output path

gsd-browser generate-test                          # Generate Playwright test from timeline
gsd-browser generate-test --name "login-flow" --output tests/login.spec.ts
```

### Security

```bash
gsd-browser check-injection                        # Scan for prompt injection in page content
gsd-browser check-injection --include-hidden       # Include hidden/invisible text (default: true)
```

### Action Cache

Reduce repeated element lookups by caching intent-to-selector mappings.

```bash
gsd-browser action-cache --action stats            # Show cache metrics
gsd-browser action-cache --action get --intent submit_form  # Look up cached selector
gsd-browser action-cache --action put --intent submit_form --selector "#submit-btn" --score 0.95
gsd-browser action-cache --action clear            # Flush cache
```

### Daemon Management

The daemon auto-starts on browser commands. These are for explicit lifecycle control.

```bash
gsd-browser daemon stop                            # Stop daemon (idempotent — safe on dead processes)
gsd-browser daemon health                          # Health check (read-only, does not auto-start)
gsd-browser daemon start                           # Explicit start (pre-warm browser before commands)
gsd-browser update                                 # Install the current release
```

---

## Global Options

Available on all commands:

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON (use when parsing output programmatically) |
| `--browser-path <path>` | Path to Chrome/Chromium binary |
| `--cdp-url <url>` | Attach to an already-running Chrome (e.g. `http://localhost:9222`) |
| `--session <name>` | Named session for parallel browser instances |
| `--no-narration-delay` | Skip narration lead-time sleeps while keeping history/events |

---

## Error Recovery

### Stale refs

```
Error: resolve_ref: JS evaluation failed: ref @v1:e3 not found
```

Refs become stale after page changes. Fix: re-snapshot and use the new version.

```bash
gsd-browser snapshot        # Get fresh refs (@v2:eN)
gsd-browser click-ref @v2:e1
```

### Click/type timeouts

```
Error: click timed out after 10s for: #submit-btn
```

The element may not be visible, may be behind an overlay, or may not exist. Try:

```bash
gsd-browser find --selector "#submit-btn"          # Verify element exists
gsd-browser scroll --direction down                # Scroll it into view
gsd-browser wait-for --condition selector_visible --value "#submit-btn"  # Wait for it
gsd-browser click "#submit-btn"                    # Retry
```

### Empty console/network logs

Console and network buffers start fresh each navigation. If you need logs from a specific action, check them **before** navigating away:

```bash
gsd-browser navigate https://example.com
gsd-browser eval "fetch('/api/data')"
gsd-browser network                                # Check network BEFORE navigating away
```

### Cookie banners / overlays blocking interaction

Many sites show consent banners that block clicks. Dismiss them first:

```bash
gsd-browser act --intent accept_cookies            # Auto-find and click accept button
gsd-browser act --intent dismiss                   # Or dismiss generic overlays
```

### Session is stopped, unhealthy, or opens a fresh blank page

If `daemon health` reports `stopped` or `unhealthy`, or a named session no longer has the page you expected, that session does not currently map to a live daemon/browser pair.

```bash
gsd-browser --session site1 daemon health
gsd-browser --session site1 daemon stop
gsd-browser --session site1 navigate https://example.com
```

Use the same `--session` value on every follow-up command. `batch` is still useful for atomic flows, but separate invocations are supported when the session is healthy.

### Daemon won't start

```
Error: daemon did not start within 10s
```

Usually the session is unhealthy, startup exited early, or browser launch state is stale. Fix:

```bash
gsd-browser daemon health                          # Inspect current state
gsd-browser daemon stop                            # Clear the current session state
gsd-browser daemon start                           # Retry explicit startup
```

---

## Common Patterns

### Form Submission

```bash
gsd-browser navigate https://example.com/signup
gsd-browser analyze-form                           # Discover field labels and types
gsd-browser fill-form --values '{"Full Name": "Jane Doe", "Email": "jane@example.com", "State": "California"}' --submit
gsd-browser wait-for --condition network_idle
gsd-browser assert --checks '[{"kind": "text_visible", "text": "Welcome"}]'
```

### Login Flow (Refs)

```bash
gsd-browser navigate https://app.example.com/login
gsd-browser act --intent accept_cookies            # Dismiss cookie banner if present
gsd-browser snapshot
# Read snapshot output to find email, password, and submit refs
gsd-browser fill-ref @v1:e1 "$USERNAME"
gsd-browser fill-ref @v1:e2 "$PASSWORD"
gsd-browser click-ref @v1:e3
gsd-browser wait-for --condition url_contains --value "/dashboard"
gsd-browser save-state --name "myapp-auth"         # Save for reuse
```

### Login Flow (Vault)

```bash
# Save credentials once (encrypted at rest)
gsd-browser vault-save --profile myapp \
  --url https://app.example.com/login \
  --username user@example.com \
  --password "$PASSWORD"

# Login in any future session
gsd-browser vault-login --profile myapp
gsd-browser wait-for --condition url_contains --value "/dashboard"
```

### Reuse Saved Auth

```bash
gsd-browser restore-state --name "myapp-auth"
gsd-browser navigate https://app.example.com/dashboard  # Already logged in
```

### Data Scraping

```bash
gsd-browser navigate https://example.com/products
gsd-browser extract --selector ".product" --multiple --schema '{
  "type": "object",
  "properties": {
    "name": {"_selector": ".title", "_attribute": "textContent"},
    "price": {"_selector": ".price", "_attribute": "textContent"},
    "link": {"_selector": "a", "_attribute": "href"}
  }
}'
```

### Visual Regression Testing

```bash
# Establish baseline (first run)
gsd-browser navigate https://example.com
gsd-browser visual-diff --name "home-page"

# Later: compare current state
gsd-browser navigate https://example.com
gsd-browser visual-diff --name "home-page"
# Output: similarity %, diff pixel count, diff image path
```

### Network Mocking for Testing

```bash
gsd-browser mock-route --url "**/api/users" --body '{"error":"server error"}' --status 500
gsd-browser navigate https://app.example.com
gsd-browser assert --checks '[{"kind": "text_visible", "text": "Something went wrong"}]'
gsd-browser clear-routes                           # Clean up mocks
```

### Parallel Sessions

```bash
gsd-browser --session site1 navigate https://site-a.com
gsd-browser --session site2 navigate https://site-b.com

gsd-browser --session site1 snapshot
gsd-browser --session site2 snapshot

# Clean up both
gsd-browser --session site1 daemon stop
gsd-browser --session site2 daemon stop
```

### Performance Audit

```bash
gsd-browser navigate https://example.com
gsd-browser trace-start --name "perf-audit"
# ... interact with the page ...
gsd-browser trace-stop --name "perf-audit.json"
gsd-browser har-export --filename "network.har"
```

### Prompt Injection Scanning

```bash
gsd-browser navigate https://untrusted-page.com
gsd-browser check-injection
# Returns severity-rated findings for visible and hidden injection patterns
```

---

## Configuration

### Config Files (TOML)

gsd-browser loads config with 5-layer merge precedence:

1. Compiled defaults
2. User config: `~/.gsd-browser/config.toml`
3. Project config: `./gsd-browser.toml`
4. Environment variables: `GSD_BROWSER_*`
5. CLI flags (highest priority)

Example `gsd-browser.toml`:

```toml
[browser]
path = "/usr/bin/chromium"

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

### Environment Variables

Supported config overrides use `GSD_BROWSER_<SECTION>_<FIELD>` naming:

```bash
GSD_BROWSER_BROWSER_PATH=/usr/bin/chromium
GSD_BROWSER_DAEMON_PORT=9223
GSD_BROWSER_SCREENSHOT_QUALITY=90
GSD_BROWSER_SETTLE_TIMEOUT_MS=1000
GSD_BROWSER_VAULT_KEY=your-encryption-key
```

---

## Session Cleanup

Always stop the daemon when done to avoid leaked Chrome processes:

```bash
gsd-browser daemon stop
```

For parallel sessions:

```bash
gsd-browser --session agent1 daemon stop
gsd-browser --session agent2 daemon stop
```
