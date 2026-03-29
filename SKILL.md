---
name: gsd-browser
description: >
  Native Rust browser automation CLI for AI agents. Use when the user needs to
  interact with websites — navigating pages, filling forms, clicking buttons,
  taking screenshots, extracting structured data, running assertions, testing
  web apps, or automating any browser task. Triggers include requests to
  "open a website", "fill out a form", "click a button", "take a screenshot",
  "scrape data from a page", "test this web app", "login to a site",
  "automate browser actions", "visual regression test", "check for prompt
  injection", or any task requiring programmatic web interaction.
allowed-tools: Bash(gsd-browser:*), Bash(gsd-browser *)
---

# Browser Automation with gsd-browser

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
gsd-browser snapshot  # Fresh refs after navigation
```

## Command Chaining

Commands can be chained with `&&` in a single shell invocation. The browser persists between commands via a background daemon, so chaining is safe and efficient.

```bash
# Chain navigate + wait + snapshot
gsd-browser navigate https://example.com && gsd-browser wait-for --condition network_idle && gsd-browser snapshot

# Chain multiple interactions
gsd-browser fill-ref @v1:e1 "user@example.com" && gsd-browser fill-ref @v1:e2 "password123" && gsd-browser click-ref @v1:e3

# Navigate and capture
gsd-browser navigate https://example.com && gsd-browser wait-for --condition network_idle && gsd-browser screenshot --output page.png
```

**When to chain:** Use `&&` when you don't need intermediate output. Run commands separately when you need to parse output first (e.g., snapshot to discover refs, then interact).

---

## Command Reference

### Navigation

```bash
gsd-browser navigate <url>                      # Navigate to a URL (--screenshot to capture)
gsd-browser back                                 # Go back in browser history
gsd-browser forward                              # Go forward in browser history
gsd-browser reload                               # Reload the current page
```

### Interaction (CSS selectors)

```bash
gsd-browser click <selector>                     # Click by CSS selector
gsd-browser click --x 100 --y 200                # Click by coordinates
gsd-browser type <selector> <text>               # Type into input (atomic fill)
gsd-browser type <selector> <text> --slowly       # Type character-by-character
gsd-browser type <selector> <text> --clear-first  # Clear field first
gsd-browser type <selector> <text> --submit       # Press Enter after typing
gsd-browser press <key>                           # Press key (Enter, Escape, Tab, Meta+A)
gsd-browser hover <selector>                      # Hover over element
gsd-browser scroll --direction down               # Scroll down 300px (default)
gsd-browser scroll --direction up --amount 500    # Scroll up 500px
gsd-browser select-option <selector> <option>     # Select dropdown option by label/value
gsd-browser set-checked <selector> true           # Check/uncheck checkbox or radio
gsd-browser drag <source> <target>                # Drag-and-drop between elements
gsd-browser upload-file <selector> <file>...      # Set files on <input type="file">
gsd-browser set-viewport --preset mobile          # Preset: mobile, tablet, desktop, wide
gsd-browser set-viewport --width 1920 --height 1080  # Custom dimensions
```

### Snapshot & Refs

Refs are versioned (`@v1:e1`, `@v2:e3`) and invalidated when the page changes. Always re-snapshot after navigation, form submission, or dynamic content loading.

```bash
gsd-browser snapshot                              # Snapshot interactive elements, assign refs
gsd-browser snapshot --selector "form"            # Scope to a CSS selector
gsd-browser snapshot --mode form                  # Semantic mode: form, dialog, navigation, errors, headings
gsd-browser snapshot --limit 80                   # Increase element limit (default: 40)
gsd-browser snapshot --interactive-only            # Only interactive elements (default: true)

gsd-browser get-ref @v1:e1                        # Get metadata for a specific ref
gsd-browser click-ref @v1:e3                      # Click element by ref
gsd-browser hover-ref @v1:e2                      # Hover element by ref
gsd-browser fill-ref @v1:e1 "text"                # Type into element by ref
```

### Inspection

```bash
gsd-browser accessibility-tree                    # Full accessibility tree (roles, names, states)
gsd-browser find --text "Sign In"                 # Find elements by text (case-insensitive contains)
gsd-browser find --role button                    # Find by ARIA role
gsd-browser find --selector ".my-class"           # Find by CSS selector
gsd-browser find --role link --limit 50           # Increase limit (default: 20)
gsd-browser page-source                           # Get raw HTML of page
gsd-browser page-source --selector "main"         # Scoped HTML source
gsd-browser eval 'document.title'                 # Evaluate JavaScript in page context
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

**Assertion kinds:** `url_contains`, `text_visible`, `text_hidden`, `selector_visible`, `selector_hidden`, `value_equals`, `no_console_errors`, `no_failed_requests`, `request_url_seen`, `response_status`, `console_message_matches`, `network_count`, `console_count`, `no_console_errors_since`, `no_failed_requests_since`, `element_count`.

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

# Fill by field identifiers (resolved: label → name → placeholder → aria-label)
gsd-browser fill-form --values '{"Email": "a@b.com", "Password": "secret", "Country": "US"}'
gsd-browser fill-form --values '{"Email": "a@b.com"}' --submit  # Fill and click submit
gsd-browser fill-form --values '{"Email": "a@b.com"}' --selector "#login-form"
```

### Intent-Based Interaction

Find and act on elements by semantic intent — no selectors or refs needed.

```bash
# Find top candidates for an intent (returns scored matches with selectors)
gsd-browser find-best --intent submit_form
gsd-browser find-best --intent close_dialog
gsd-browser find-best --intent primary_cta --scope "#modal"
gsd-browser find-best --intent accept_cookies

# Act: find best match and click/focus it in one call
gsd-browser act --intent submit_form
gsd-browser act --intent close_dialog
gsd-browser act --intent search_field
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
| `main_content` | click | Main content area (`<main>`, `<article>`) |
| `pagination_next` | click | Next page in pagination |
| `pagination_prev` | click | Previous page in pagination |

### Pages & Frames

```bash
gsd-browser list-pages                            # List all open tabs
gsd-browser switch-page --id 2                    # Switch to tab by ID
gsd-browser close-page --id 3                     # Close a tab

gsd-browser list-frames                           # List all frames (main + iframes)
gsd-browser select-frame --name "iframe-name"     # Select frame by name
gsd-browser select-frame --url-pattern "embed"    # Select frame by URL substring
gsd-browser select-frame --index 0                # Select by index
gsd-browser select-frame --name main              # Return to main frame
```

### Diagnostics

```bash
gsd-browser console                               # Get console log entries
gsd-browser network                               # Get network log entries
gsd-browser dialog                                # Get dialog events (alert, confirm, prompt)
gsd-browser timeline                              # Query the action timeline
gsd-browser session-summary                       # Diagnostic summary of current session
gsd-browser debug-bundle                          # Full debug bundle: screenshot + logs + timeline + a11y tree
```

### Visual

```bash
gsd-browser screenshot                            # Screenshot to stdout (JPEG)
gsd-browser screenshot --output page.png          # Write to file
gsd-browser screenshot --full-page                # Full scrollable page
gsd-browser screenshot --selector "#hero"         # Crop to element (PNG)
gsd-browser screenshot --quality 50               # JPEG quality 1-100 (default: 80)
gsd-browser screenshot --format png               # Force PNG format

gsd-browser zoom-region --x 100 --y 200 --width 400 --height 300  # Capture region
gsd-browser zoom-region --x 0 --y 0 --width 200 --height 200 --scale 3  # Upscale for detail

gsd-browser save-pdf                              # Save page as PDF
gsd-browser save-pdf --output report.pdf          # Custom output path
```

### Visual Regression

```bash
# First run: saves baseline screenshot
gsd-browser visual-diff --name "homepage"

# Subsequent runs: compare against baseline, report mismatch
gsd-browser visual-diff --name "homepage"
gsd-browser visual-diff --name "homepage" --threshold 0.05   # Stricter tolerance
gsd-browser visual-diff --selector "#hero" --name "hero"     # Scope to element
gsd-browser visual-diff --name "homepage" --update-baseline   # Reset baseline
```

### Structured Data Extraction

Extract data from pages using CSS selectors with JSON Schema validation.

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

# Array of items (container mode)
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
# Mock API response
gsd-browser mock-route --url "**/api/users*" --body '[{"name":"Alice"}]' --status 200

# Simulate slow responses
gsd-browser mock-route --url "**/api/data" --body '{"ok":true}' --delay 3000

# Block URLs (analytics, ads)
gsd-browser block-urls "**/analytics*" "**/ads*"

# Remove all mocks and blocks
gsd-browser clear-routes
```

### Device Emulation

```bash
gsd-browser emulate-device "iPhone 15"            # Full device emulation
gsd-browser emulate-device "Pixel 7"              # Android device
gsd-browser emulate-device "iPad Pro 11"           # Tablet
gsd-browser emulate-device list                    # Show all available presets
```

**Note:** Device emulation recreates the browser context — current page state is lost.

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

Vault encryption uses `GSD_BROWSER_VAULT_KEY` env var.

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
# Scan for prompt injection in page content
gsd-browser check-injection
gsd-browser check-injection --include-hidden       # Include hidden/invisible text (default: true)
```

### Action Cache

Reduce token cost on repeat visits by caching page-structure → selector mappings.

```bash
gsd-browser action-cache --action stats            # Show cache metrics
gsd-browser action-cache --action get --intent submit_form  # Look up cached selector
gsd-browser action-cache --action put --intent submit_form --selector "#submit-btn" --score 0.95
gsd-browser action-cache --action clear            # Flush cache
```

### Daemon Management

The daemon auto-starts on first command. Manual control:

```bash
gsd-browser daemon start                           # Start daemon explicitly
gsd-browser daemon stop                            # Stop daemon
gsd-browser daemon health                          # Health check
```

---

## Global Options

Available on all commands:

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON (machine-readable) |
| `--browser-path <path>` | Path to Chrome/Chromium binary |
| `--session <name>` | Named session for parallel instances |

---

## Ref Lifecycle (Important)

Refs use versioned format: `@v1:e1`, `@v1:e2`, `@v2:e1`, etc. The version increments each time you snapshot. Refs are **invalidated** when the page changes. Always re-snapshot after:

- Clicking links or buttons that navigate
- Form submissions
- Dynamic content loading (dropdowns, modals, AJAX)

```bash
gsd-browser click-ref @v1:e5           # Navigates to new page
gsd-browser snapshot                    # MUST re-snapshot — old refs are stale
gsd-browser click-ref @v2:e1           # Use new refs (version incremented)
```

---

## Configuration

### Config Files (TOML)

gsd-browser loads config from TOML files with 5-layer merge precedence:

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
port = 9223
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

All config values can be overridden with `GSD_BROWSER_` prefix and `__` section separator:

```bash
GSD_BROWSER_BROWSER__PATH=/usr/bin/chromium
GSD_BROWSER_DAEMON__PORT=9223
GSD_BROWSER_SCREENSHOT__QUALITY=90
GSD_BROWSER_SETTLE__TIMEOUT_MS=1000
GSD_BROWSER_LOGS__MAX_BUFFER_SIZE=2000
GSD_BROWSER_ARTIFACTS__DIR=./artifacts
GSD_BROWSER_VAULT_KEY=your-encryption-key
```

---

## Common Patterns

### Form Submission

```bash
gsd-browser navigate https://example.com/signup
gsd-browser analyze-form                          # Discover field labels
gsd-browser fill-form --values '{"Full Name": "Jane Doe", "Email": "jane@example.com", "State": "California"}' --submit
gsd-browser wait-for --condition network_idle
gsd-browser assert --checks '[{"kind": "text_visible", "text": "Welcome"}]'
```

### Auth Flow (Vault)

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

### Auth Flow (State Persistence)

```bash
# Login and save state
gsd-browser navigate https://app.example.com/login
gsd-browser snapshot
gsd-browser fill-ref @v1:e1 "$USERNAME" && gsd-browser fill-ref @v1:e2 "$PASSWORD"
gsd-browser click-ref @v1:e3
gsd-browser wait-for --condition url_contains --value "/dashboard"
gsd-browser save-state --name "myapp-auth"

# Reuse in future sessions
gsd-browser restore-state --name "myapp-auth"
gsd-browser navigate https://app.example.com/dashboard
```

### Data Extraction

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
# Establish baseline
gsd-browser navigate https://example.com
gsd-browser visual-diff --name "home-page"

# Later: compare current state
gsd-browser navigate https://example.com
gsd-browser visual-diff --name "home-page"
# Output: similarity score, diff pixel count, pass/fail
```

### Network Mocking for Testing

```bash
# Mock API to test error handling
gsd-browser mock-route --url "**/api/users" --body '{"error":"server error"}' --status 500
gsd-browser navigate https://app.example.com
gsd-browser assert --checks '[{"kind": "text_visible", "text": "Something went wrong"}]'
gsd-browser clear-routes
```

### Multi-Tab Workflows

```bash
gsd-browser navigate https://site-a.com
gsd-browser list-pages
# Switch between tabs
gsd-browser switch-page --id 2
gsd-browser snapshot
gsd-browser switch-page --id 1
```

### Parallel Sessions

```bash
gsd-browser --session site1 navigate https://site-a.com
gsd-browser --session site2 navigate https://site-b.com

gsd-browser --session site1 snapshot
gsd-browser --session site2 snapshot
```

### Performance Tracing

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
# Returns findings with severity levels for any injection attempts
```

---

## Session Management

Always close sessions when done to avoid leaked processes:

```bash
gsd-browser daemon stop                            # Stop default session daemon
```

For parallel sessions, use `--session` to isolate:

```bash
gsd-browser --session agent1 navigate https://site-a.com
gsd-browser --session agent2 navigate https://site-b.com
gsd-browser --session agent1 daemon stop
gsd-browser --session agent2 daemon stop
```

## JSON Output

All commands support `--json` for machine-readable output:

```bash
gsd-browser snapshot --json
gsd-browser find --role button --json
gsd-browser assert --checks '[...]' --json
```

Use JSON output when you need to parse results programmatically.
