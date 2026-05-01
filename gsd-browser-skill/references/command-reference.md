<overview>
Complete syntax reference for gsd-browser commands. Argument syntax: `<arg>` = required positional, `[arg]` = optional positional, `--flag` = named option. Do NOT add `--` prefix to positional args.
</overview>

<navigation>

```bash
gsd-browser navigate <url>
gsd-browser back
gsd-browser forward
gsd-browser reload
```

</navigation>

<interaction>

All selectors are **positional** — do NOT use `--selector`.

```bash
gsd-browser click <selector>
gsd-browser click --x 100 --y 200                         # Click by coordinates

gsd-browser type <selector> <text>
gsd-browser type <selector> <text> --slowly                # Character-by-character
gsd-browser type <selector> <text> --clear-first           # Clear before typing
gsd-browser type <selector> <text> --submit                # Press Enter after

gsd-browser press <key>                                    # Enter, Escape, Tab, Meta+A
gsd-browser hover <selector>
gsd-browser scroll --direction down                        # Default 300px
gsd-browser scroll --direction up --amount 500
gsd-browser select-option <selector> <option>              # Dropdown by label/value
gsd-browser set-checked <selector> --checked               # Omit --checked to uncheck
gsd-browser drag <source-selector> <target-selector>
gsd-browser upload-file <selector> <file>...
gsd-browser set-viewport --preset mobile                   # mobile, tablet, desktop, wide
gsd-browser set-viewport --width 1920 --height 1080
```

</interaction>

<snapshot_and_refs>

```bash
gsd-browser snapshot
gsd-browser snapshot --selector "form"
gsd-browser snapshot --mode <mode>                         # See snapshot modes below
gsd-browser snapshot --limit 80                            # Default: 40

gsd-browser get-ref <ref>
gsd-browser click-ref <ref>
gsd-browser hover-ref <ref>
gsd-browser fill-ref <ref> <text>
```

**Snapshot modes:** `interactive` (default), `form`, `dialog`, `navigation`, `errors`, `headings`, `visible_only`

</snapshot_and_refs>

<inspection>

```bash
gsd-browser accessibility-tree
gsd-browser find --text "Sign In"
gsd-browser find --role button
gsd-browser find --selector ".my-class"
gsd-browser find --role link --limit 50                    # Default: 20
gsd-browser page-source
gsd-browser page-source --selector "main"
gsd-browser eval '<js-expression>'
```

</inspection>

<assertions>

```bash
gsd-browser assert --checks '[
  {"kind": "url_contains", "text": "/dashboard"},
  {"kind": "text_visible", "text": "Welcome"},
  {"kind": "selector_visible", "selector": "#user-menu"},
  {"kind": "value_equals", "selector": "input[name=email]", "value": "user@test.com"},
  {"kind": "no_console_errors"},
  {"kind": "no_failed_requests"},
  {"kind": "element_count", "selector": ".item", "min": 5}
]'
```

**All kinds (17):** `url_contains`, `text_visible`, `text_hidden`, `selector_visible`, `selector_hidden`, `value_equals`, `checked`, `no_console_errors`, `no_failed_requests`, `request_url_seen`, `response_status`, `console_message_matches`, `network_count`, `console_count`, `element_count`, `no_console_errors_since`, `no_failed_requests_since`

</assertions>

<batch_execution>

```bash
gsd-browser batch --steps '[
  {"action": "navigate", "url": "https://example.com"},
  {"action": "wait_for", "condition": "network_idle"},
  {"action": "click", "selector": "#login-btn"},
  {"action": "type", "selector": "input[name=email]", "text": "user@test.com"},
  {"action": "assert", "checks": [{"kind": "url_contains", "text": "/dashboard"}]}
]'
gsd-browser batch --steps '[...]' --summary-only
```

**Actions:** `navigate`, `click`, `type`, `key_press`, `wait_for`, `assert`, `click_ref`, `fill_ref`

</batch_execution>

<wait_conditions>

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
gsd-browser wait-for --condition selector_visible --value "#slow" --timeout 30000
```

Default timeout: 10000ms.

</wait_conditions>

<forms>

```bash
gsd-browser analyze-form
gsd-browser analyze-form --selector "#signup-form"
gsd-browser fill-form --values '{"Email": "a@b.com", "Password": "secret"}'
gsd-browser fill-form --values '{"Email": "a@b.com"}' --submit
gsd-browser fill-form --values '{"Email": "a@b.com"}' --selector "#login-form"
```

Fields are matched by label, name, placeholder, or aria-label.

</forms>

<semantic_intents>

```bash
gsd-browser find-best --intent submit_form
gsd-browser find-best --intent accept_cookies --scope "#modal"
gsd-browser act --intent submit_form
gsd-browser act --intent accept_cookies
```

See `references/semantic-intents.md` for all 15 intents.

</semantic_intents>

<pages_and_frames>

Page and frame IDs are **positional**.

```bash
gsd-browser list-pages
gsd-browser switch-page <id>
gsd-browser close-page <id>
gsd-browser list-frames
gsd-browser select-frame --name "iframe-name"
gsd-browser select-frame --url-pattern "embed"
gsd-browser select-frame --index 0
gsd-browser select-frame --name main                      # Return to main frame
```

</pages_and_frames>

<live_viewer_and_narration>

The live viewer is a localhost screen-sharing surface for the active browser session. Browser actions still run through CLI commands. The viewer displays live frames, narrated history, ref overlays, target rings, click ripples, failure markers, and page-following across navigation or tab changes.

```bash
gsd-browser view                                         # Open the live viewer
gsd-browser view --print-only                            # Print URL only
gsd-browser view --history                               # Open history-focused viewer
gsd-browser view --history --print-only                  # Print history URL only

gsd-browser goal "Find the checkout button"              # Set viewer goal banner
gsd-browser goal --clear                                 # Clear goal banner

gsd-browser pause                                        # Pause before next narrated action
gsd-browser resume                                       # Resume actions
gsd-browser step                                         # Allow one action, then pause
gsd-browser abort                                        # Abort next gated action
```

Use `--session <name>` consistently so the viewer and commands attach to the same browser:

```bash
gsd-browser --session demo navigate https://example.com
gsd-browser --session demo view --print-only
gsd-browser --session demo click "h1"
```

Use `--no-narration-delay` for fast agent-only runs that keep narration events/history without lead-time sleeps:

```bash
gsd-browser --session demo --no-narration-delay click "h1"
```

</live_viewer_and_narration>

<diagnostics>

```bash
gsd-browser console
gsd-browser console --no-clear
gsd-browser network
gsd-browser dialog
gsd-browser timeline
gsd-browser session-summary
gsd-browser debug-bundle
```

</diagnostics>

<visual>

```bash
gsd-browser screenshot
gsd-browser screenshot --output page.png
gsd-browser screenshot --format png
gsd-browser screenshot --full-page
gsd-browser screenshot --selector "#hero"                  # Always PNG
gsd-browser screenshot --quality 50                        # JPEG 1-100, default 80

gsd-browser zoom-region --x 100 --y 200 --width 400 --height 300
gsd-browser zoom-region --x 0 --y 0 --width 200 --height 200 --scale 3

gsd-browser save-pdf
gsd-browser save-pdf --output report.pdf
gsd-browser save-pdf --format Letter                       # A4, Letter, Legal, Tabloid

gsd-browser visual-diff --name "homepage"
gsd-browser visual-diff --name "homepage" --threshold 0.05
gsd-browser visual-diff --selector "#hero" --name "hero"
gsd-browser visual-diff --name "homepage" --update-baseline
```

</visual>

<data_extraction>

```bash
gsd-browser extract --schema '{
  "type": "object",
  "properties": {
    "title": {"_selector": "h1", "_attribute": "textContent"},
    "price": {"_selector": ".price", "_attribute": "textContent"}
  }
}'

gsd-browser extract --selector ".product" --multiple --schema '{...}'
```

</data_extraction>

<network_mocking>

```bash
gsd-browser mock-route --url "**/api/users*" --body '[{"name":"Alice"}]' --status 200
gsd-browser mock-route --url "**/api/data" --body '{"ok":true}' --delay 3000
gsd-browser block-urls "**/analytics*" "**/ads*"
gsd-browser clear-routes
```

</network_mocking>

<device_emulation>

```bash
gsd-browser emulate-device <device-name>
gsd-browser emulate-device "iPhone 15"
gsd-browser emulate-device "Pixel 7"
gsd-browser emulate-device "iPad Pro 11"
gsd-browser emulate-device list
```

**Warning:** Recreates browser context. Page state and cookies are lost.

</device_emulation>

<state_and_auth>

```bash
gsd-browser save-state --name "logged-in"
gsd-browser restore-state --name "logged-in"

gsd-browser vault-save --profile github --url https://github.com/login \
  --username user --password "secret"
gsd-browser vault-login --profile github
gsd-browser vault-list
```

Vault requires `GSD_BROWSER_VAULT_KEY` env var set **before daemon starts**.

</state_and_auth>

<tracing_and_recording>

```bash
gsd-browser trace-start
gsd-browser trace-start --name "checkout-flow"
gsd-browser trace-stop
gsd-browser trace-stop --name "checkout.json"

gsd-browser har-export
gsd-browser har-export --filename "session.har"

gsd-browser generate-test
gsd-browser generate-test --name "login-flow" --output tests/login.spec.ts
```

</tracing_and_recording>

<security>

```bash
gsd-browser check-injection
gsd-browser check-injection --include-hidden
```

</security>

<action_cache>

```bash
gsd-browser action-cache --action stats
gsd-browser action-cache --action get --intent submit_form
gsd-browser action-cache --action put --intent submit_form --selector "#submit-btn" --score 0.95
gsd-browser action-cache --action clear
```

</action_cache>

<daemon_management>

```bash
gsd-browser daemon health
gsd-browser daemon stop
gsd-browser daemon start
```

The daemon auto-starts on browser commands. `daemon health` is read-only and does not start a session. `daemon stop` is idempotent — safe to call even if the process is already dead.

</daemon_management>
