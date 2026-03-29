---
name: browser-tools
description: >
  Native Rust browser automation CLI for AI agents. Use when the user needs to
  interact with websites — navigating pages, filling forms, clicking buttons,
  taking screenshots, extracting structured data, running assertions, testing
  web apps, or automating any browser task. Triggers include requests to
  "open a website", "fill out a form", "click a button", "take a screenshot",
  "scrape data from a page", "test this web app", "login to a site",
  "automate browser actions", "visual regression test", "check for prompt
  injection", or any task requiring programmatic web interaction.
allowed-tools: Bash(browser-tools:*), Bash(browser-tools *)
---

# Browser Automation with browser-tools

## Core Workflow

Every browser automation follows this pattern:

1. **Navigate**: `browser-tools navigate <url>`
2. **Snapshot**: `browser-tools snapshot` (get versioned refs like `@v1:e1`, `@v1:e2`)
3. **Interact**: Use refs to click, fill, hover
4. **Re-snapshot**: After navigation or DOM changes, get fresh refs

```bash
browser-tools navigate https://example.com/form
browser-tools snapshot
# Output: @v1:e1 [input type="email"], @v1:e2 [input type="password"], @v1:e3 [button] "Submit"

browser-tools fill-ref @v1:e1 "user@example.com"
browser-tools fill-ref @v1:e2 "password123"
browser-tools click-ref @v1:e3
browser-tools wait-for --condition network_idle
browser-tools snapshot  # Fresh refs after navigation
```

## Command Chaining

Commands can be chained with `&&` in a single shell invocation. The browser persists between commands via a background daemon, so chaining is safe and efficient.

```bash
# Chain navigate + wait + snapshot
browser-tools navigate https://example.com && browser-tools wait-for --condition network_idle && browser-tools snapshot

# Chain multiple interactions
browser-tools fill-ref @v1:e1 "user@example.com" && browser-tools fill-ref @v1:e2 "password123" && browser-tools click-ref @v1:e3

# Navigate and capture
browser-tools navigate https://example.com && browser-tools wait-for --condition network_idle && browser-tools screenshot --output page.png
```

**When to chain:** Use `&&` when you don't need intermediate output. Run commands separately when you need to parse output first (e.g., snapshot to discover refs, then interact).

---

## Command Reference

### Navigation

```bash
browser-tools navigate <url>                      # Navigate to a URL (--screenshot to capture)
browser-tools back                                 # Go back in browser history
browser-tools forward                              # Go forward in browser history
browser-tools reload                               # Reload the current page
```

### Interaction (CSS selectors)

```bash
browser-tools click <selector>                     # Click by CSS selector
browser-tools click --x 100 --y 200                # Click by coordinates
browser-tools type <selector> <text>               # Type into input (atomic fill)
browser-tools type <selector> <text> --slowly       # Type character-by-character
browser-tools type <selector> <text> --clear-first  # Clear field first
browser-tools type <selector> <text> --submit       # Press Enter after typing
browser-tools press <key>                           # Press key (Enter, Escape, Tab, Meta+A)
browser-tools hover <selector>                      # Hover over element
browser-tools scroll --direction down               # Scroll down 300px (default)
browser-tools scroll --direction up --amount 500    # Scroll up 500px
browser-tools select-option <selector> <option>     # Select dropdown option by label/value
browser-tools set-checked <selector> true           # Check/uncheck checkbox or radio
browser-tools drag <source> <target>                # Drag-and-drop between elements
browser-tools upload-file <selector> <file>...      # Set files on <input type="file">
browser-tools set-viewport --preset mobile          # Preset: mobile, tablet, desktop, wide
browser-tools set-viewport --width 1920 --height 1080  # Custom dimensions
```

### Snapshot & Refs

Refs are versioned (`@v1:e1`, `@v2:e3`) and invalidated when the page changes. Always re-snapshot after navigation, form submission, or dynamic content loading.

```bash
browser-tools snapshot                              # Snapshot interactive elements, assign refs
browser-tools snapshot --selector "form"            # Scope to a CSS selector
browser-tools snapshot --mode form                  # Semantic mode: form, dialog, navigation, errors, headings
browser-tools snapshot --limit 80                   # Increase element limit (default: 40)
browser-tools snapshot --interactive-only            # Only interactive elements (default: true)

browser-tools get-ref @v1:e1                        # Get metadata for a specific ref
browser-tools click-ref @v1:e3                      # Click element by ref
browser-tools hover-ref @v1:e2                      # Hover element by ref
browser-tools fill-ref @v1:e1 "text"                # Type into element by ref
```

### Inspection

```bash
browser-tools accessibility-tree                    # Full accessibility tree (roles, names, states)
browser-tools find --text "Sign In"                 # Find elements by text (case-insensitive contains)
browser-tools find --role button                    # Find by ARIA role
browser-tools find --selector ".my-class"           # Find by CSS selector
browser-tools find --role link --limit 50           # Increase limit (default: 20)
browser-tools page-source                           # Get raw HTML of page
browser-tools page-source --selector "main"         # Scoped HTML source
browser-tools eval 'document.title'                 # Evaluate JavaScript in page context
```

### Assertions

Run explicit pass/fail checks against the current page state. Prefer this over inferring success from output.

```bash
browser-tools assert --checks '[
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
browser-tools batch --steps '[
  {"action": "navigate", "url": "https://example.com"},
  {"action": "wait_for", "condition": "network_idle"},
  {"action": "click", "selector": "#login-btn"},
  {"action": "type", "selector": "input[name=email]", "text": "user@test.com"},
  {"action": "type", "selector": "input[name=password]", "text": "secret", "submit": true},
  {"action": "assert", "checks": [{"kind": "url_contains", "text": "/dashboard"}]}
]'

# With --summary-only to reduce output
browser-tools batch --steps '[...]' --summary-only
```

**Batch actions:** `navigate`, `click`, `type`, `key_press`, `wait_for`, `assert`, `click_ref`, `fill_ref`.

### Wait Conditions

```bash
browser-tools wait-for --condition selector_visible --value "#content"
browser-tools wait-for --condition selector_hidden --value ".spinner"
browser-tools wait-for --condition url_contains --value "/dashboard"
browser-tools wait-for --condition network_idle
browser-tools wait-for --condition delay --value 2000
browser-tools wait-for --condition text_visible --value "Success"
browser-tools wait-for --condition text_hidden --value "Loading"
browser-tools wait-for --condition request_completed --value "/api/data"
browser-tools wait-for --condition console_message --value "ready"
browser-tools wait-for --condition element_count --value ".item" --threshold ">=5"
browser-tools wait-for --condition region_stable --value "#content"

# Custom timeout (default: 10000ms)
browser-tools wait-for --condition selector_visible --value "#slow" --timeout 30000
```

### Forms (Smart Fill)

Analyze forms and fill them by field label, name, placeholder, or aria-label — no selectors needed.

```bash
# Analyze form structure
browser-tools analyze-form
browser-tools analyze-form --selector "#signup-form"

# Fill by field identifiers (resolved: label → name → placeholder → aria-label)
browser-tools fill-form --values '{"Email": "a@b.com", "Password": "secret", "Country": "US"}'
browser-tools fill-form --values '{"Email": "a@b.com"}' --submit  # Fill and click submit
browser-tools fill-form --values '{"Email": "a@b.com"}' --selector "#login-form"
```

### Intent-Based Interaction

Find and act on elements by semantic intent — no selectors or refs needed.

```bash
# Find top candidates for an intent (returns scored matches with selectors)
browser-tools find-best --intent submit_form
browser-tools find-best --intent close_dialog
browser-tools find-best --intent primary_cta --scope "#modal"
browser-tools find-best --intent accept_cookies

# Act: find best match and click/focus it in one call
browser-tools act --intent submit_form
browser-tools act --intent close_dialog
browser-tools act --intent search_field
browser-tools act --intent fill_email
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
browser-tools list-pages                            # List all open tabs
browser-tools switch-page --id 2                    # Switch to tab by ID
browser-tools close-page --id 3                     # Close a tab

browser-tools list-frames                           # List all frames (main + iframes)
browser-tools select-frame --name "iframe-name"     # Select frame by name
browser-tools select-frame --url-pattern "embed"    # Select frame by URL substring
browser-tools select-frame --index 0                # Select by index
browser-tools select-frame --name main              # Return to main frame
```

### Diagnostics

```bash
browser-tools console                               # Get console log entries
browser-tools network                               # Get network log entries
browser-tools dialog                                # Get dialog events (alert, confirm, prompt)
browser-tools timeline                              # Query the action timeline
browser-tools session-summary                       # Diagnostic summary of current session
browser-tools debug-bundle                          # Full debug bundle: screenshot + logs + timeline + a11y tree
```

### Visual

```bash
browser-tools screenshot                            # Screenshot to stdout (JPEG)
browser-tools screenshot --output page.png          # Write to file
browser-tools screenshot --full-page                # Full scrollable page
browser-tools screenshot --selector "#hero"         # Crop to element (PNG)
browser-tools screenshot --quality 50               # JPEG quality 1-100 (default: 80)
browser-tools screenshot --format png               # Force PNG format

browser-tools zoom-region --x 100 --y 200 --width 400 --height 300  # Capture region
browser-tools zoom-region --x 0 --y 0 --width 200 --height 200 --scale 3  # Upscale for detail

browser-tools save-pdf                              # Save page as PDF
browser-tools save-pdf --output report.pdf          # Custom output path
```

### Visual Regression

```bash
# First run: saves baseline screenshot
browser-tools visual-diff --name "homepage"

# Subsequent runs: compare against baseline, report mismatch
browser-tools visual-diff --name "homepage"
browser-tools visual-diff --name "homepage" --threshold 0.05   # Stricter tolerance
browser-tools visual-diff --selector "#hero" --name "hero"     # Scope to element
browser-tools visual-diff --name "homepage" --update-baseline   # Reset baseline
```

### Structured Data Extraction

Extract data from pages using CSS selectors with JSON Schema validation.

```bash
# Single item
browser-tools extract --schema '{
  "type": "object",
  "properties": {
    "title": {"_selector": "h1", "_attribute": "textContent"},
    "price": {"_selector": ".price", "_attribute": "textContent"},
    "image": {"_selector": "img.product", "_attribute": "src"}
  }
}'

# Array of items (container mode)
browser-tools extract --selector ".product-card" --multiple --schema '{
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
browser-tools mock-route --url "**/api/users*" --body '[{"name":"Alice"}]' --status 200

# Simulate slow responses
browser-tools mock-route --url "**/api/data" --body '{"ok":true}' --delay 3000

# Block URLs (analytics, ads)
browser-tools block-urls "**/analytics*" "**/ads*"

# Remove all mocks and blocks
browser-tools clear-routes
```

### Device Emulation

```bash
browser-tools emulate-device "iPhone 15"            # Full device emulation
browser-tools emulate-device "Pixel 7"              # Android device
browser-tools emulate-device "iPad Pro 11"           # Tablet
browser-tools emulate-device list                    # Show all available presets
```

**Note:** Device emulation recreates the browser context — current page state is lost.

### State & Auth

```bash
# Save/restore browser state (cookies, localStorage, sessionStorage)
browser-tools save-state --name "logged-in"
browser-tools restore-state --name "logged-in"

# Auth vault (encrypted credentials)
browser-tools vault-save --profile github --url https://github.com/login \
  --username user --password "secret"
browser-tools vault-login --profile github
browser-tools vault-list                             # List profiles (no secrets shown)
```

Vault encryption uses `BROWSER_TOOLS_VAULT_KEY` env var.

### Tracing & Recording

```bash
browser-tools trace-start                            # Start CDP performance trace
browser-tools trace-start --name "checkout-flow"     # Named trace
browser-tools trace-stop                             # Stop and write trace to disk
browser-tools trace-stop --name "checkout.json"      # Custom output filename

browser-tools har-export                             # Export network as HAR 1.2 JSON
browser-tools har-export --filename "session.har"    # Custom output path

browser-tools generate-test                          # Generate Playwright test from timeline
browser-tools generate-test --name "login-flow" --output tests/login.spec.ts
```

### Security

```bash
# Scan for prompt injection in page content
browser-tools check-injection
browser-tools check-injection --include-hidden       # Include hidden/invisible text (default: true)
```

### Action Cache

Reduce token cost on repeat visits by caching page-structure → selector mappings.

```bash
browser-tools action-cache --action stats            # Show cache metrics
browser-tools action-cache --action get --intent submit_form  # Look up cached selector
browser-tools action-cache --action put --intent submit_form --selector "#submit-btn" --score 0.95
browser-tools action-cache --action clear            # Flush cache
```

### Daemon Management

The daemon auto-starts on first command. Manual control:

```bash
browser-tools daemon start                           # Start daemon explicitly
browser-tools daemon stop                            # Stop daemon
browser-tools daemon health                          # Health check
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
browser-tools click-ref @v1:e5           # Navigates to new page
browser-tools snapshot                    # MUST re-snapshot — old refs are stale
browser-tools click-ref @v2:e1           # Use new refs (version incremented)
```

---

## Configuration

### Config Files (TOML)

browser-tools loads config from TOML files with 5-layer merge precedence:

1. Compiled defaults
2. User config: `~/.browser-tools/config.toml`
3. Project config: `./browser-tools.toml`
4. Environment variables: `BROWSER_TOOLS_*`
5. CLI flags (highest priority)

Example `browser-tools.toml`:

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

All config values can be overridden with `BROWSER_TOOLS_` prefix and `__` section separator:

```bash
BROWSER_TOOLS_BROWSER__PATH=/usr/bin/chromium
BROWSER_TOOLS_DAEMON__PORT=9223
BROWSER_TOOLS_SCREENSHOT__QUALITY=90
BROWSER_TOOLS_SETTLE__TIMEOUT_MS=1000
BROWSER_TOOLS_LOGS__MAX_BUFFER_SIZE=2000
BROWSER_TOOLS_ARTIFACTS__DIR=./artifacts
BROWSER_TOOLS_VAULT_KEY=your-encryption-key
```

---

## Common Patterns

### Form Submission

```bash
browser-tools navigate https://example.com/signup
browser-tools analyze-form                          # Discover field labels
browser-tools fill-form --values '{"Full Name": "Jane Doe", "Email": "jane@example.com", "State": "California"}' --submit
browser-tools wait-for --condition network_idle
browser-tools assert --checks '[{"kind": "text_visible", "text": "Welcome"}]'
```

### Auth Flow (Vault)

```bash
# Save credentials once (encrypted at rest)
browser-tools vault-save --profile myapp \
  --url https://app.example.com/login \
  --username user@example.com \
  --password "$PASSWORD"

# Login in any future session
browser-tools vault-login --profile myapp
browser-tools wait-for --condition url_contains --value "/dashboard"
```

### Auth Flow (State Persistence)

```bash
# Login and save state
browser-tools navigate https://app.example.com/login
browser-tools snapshot
browser-tools fill-ref @v1:e1 "$USERNAME" && browser-tools fill-ref @v1:e2 "$PASSWORD"
browser-tools click-ref @v1:e3
browser-tools wait-for --condition url_contains --value "/dashboard"
browser-tools save-state --name "myapp-auth"

# Reuse in future sessions
browser-tools restore-state --name "myapp-auth"
browser-tools navigate https://app.example.com/dashboard
```

### Data Extraction

```bash
browser-tools navigate https://example.com/products
browser-tools extract --selector ".product" --multiple --schema '{
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
browser-tools navigate https://example.com
browser-tools visual-diff --name "home-page"

# Later: compare current state
browser-tools navigate https://example.com
browser-tools visual-diff --name "home-page"
# Output: similarity score, diff pixel count, pass/fail
```

### Network Mocking for Testing

```bash
# Mock API to test error handling
browser-tools mock-route --url "**/api/users" --body '{"error":"server error"}' --status 500
browser-tools navigate https://app.example.com
browser-tools assert --checks '[{"kind": "text_visible", "text": "Something went wrong"}]'
browser-tools clear-routes
```

### Multi-Tab Workflows

```bash
browser-tools navigate https://site-a.com
browser-tools list-pages
# Switch between tabs
browser-tools switch-page --id 2
browser-tools snapshot
browser-tools switch-page --id 1
```

### Parallel Sessions

```bash
browser-tools --session site1 navigate https://site-a.com
browser-tools --session site2 navigate https://site-b.com

browser-tools --session site1 snapshot
browser-tools --session site2 snapshot
```

### Performance Tracing

```bash
browser-tools navigate https://example.com
browser-tools trace-start --name "perf-audit"
# ... interact with the page ...
browser-tools trace-stop --name "perf-audit.json"
browser-tools har-export --filename "network.har"
```

### Prompt Injection Scanning

```bash
browser-tools navigate https://untrusted-page.com
browser-tools check-injection
# Returns findings with severity levels for any injection attempts
```

---

## Session Management

Always close sessions when done to avoid leaked processes:

```bash
browser-tools daemon stop                            # Stop default session daemon
```

For parallel sessions, use `--session` to isolate:

```bash
browser-tools --session agent1 navigate https://site-a.com
browser-tools --session agent2 navigate https://site-b.com
browser-tools --session agent1 daemon stop
browser-tools --session agent2 daemon stop
```

## JSON Output

All commands support `--json` for machine-readable output:

```bash
browser-tools snapshot --json
browser-tools find --role button --json
browser-tools assert --checks '[...]' --json
```

Use JSON output when you need to parse results programmatically.
