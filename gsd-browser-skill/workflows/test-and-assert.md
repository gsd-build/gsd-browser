<required_reading>
**Read these reference files NOW:**
1. references/command-reference.md (Assertions, Batch Execution, Wait Conditions, Visual, Network Mocking, Tracing sections)
2. references/error-recovery.md
</required_reading>

<process>

**Step 1: Navigate and set up test state**

```bash
gsd-browser navigate <url>
gsd-browser wait-for --condition network_idle
```

Optionally restore saved state for authenticated tests:

```bash
gsd-browser restore-state --name "logged-in"
gsd-browser navigate https://app.example.com/dashboard
```

**Step 2: Run assertions**

Assertions run explicit pass/fail checks against the current page:

```bash
gsd-browser assert --checks '[
  {"kind": "url_contains", "text": "/dashboard"},
  {"kind": "text_visible", "text": "Welcome"},
  {"kind": "selector_visible", "selector": "#user-menu"},
  {"kind": "no_console_errors"},
  {"kind": "no_failed_requests"}
]'
```

**Assertion kinds:**

| Kind | Parameters | Checks |
|------|-----------|--------|
| `url_contains` | `text` | Current URL contains string |
| `text_visible` | `text` | Text is visible on page |
| `text_hidden` | `text` | Text is NOT visible |
| `selector_visible` | `selector` | Element is visible |
| `selector_hidden` | `selector` | Element is NOT visible |
| `value_equals` | `selector`, `value` | Input value matches |
| `checked` | `selector` | Checkbox/radio is checked |
| `element_count` | `selector`, `min`/`max`/`exact` | Element count in range |
| `no_console_errors` | - | No console errors logged |
| `no_failed_requests` | - | No failed network requests |
| `no_console_errors_since` | `since` (ms timestamp) | No errors since time |
| `no_failed_requests_since` | `since` (ms timestamp) | No failures since time |
| `request_url_seen` | `url` | A request to URL was made |
| `response_status` | `url`, `status` | Response had expected status |
| `console_message_matches` | `pattern` | Console message matches regex |
| `network_count` | `min`/`max`/`exact` | Network request count |
| `console_count` | `min`/`max`/`exact` | Console message count |

**Step 3: Batch execution for multi-step tests**

Execute multiple steps in one call. Stops on first failure:

```bash
gsd-browser batch --steps '[
  {"action": "navigate", "url": "https://example.com"},
  {"action": "wait_for", "condition": "network_idle"},
  {"action": "click", "selector": "#login-btn"},
  {"action": "type", "selector": "input[name=email]", "text": "user@test.com"},
  {"action": "type", "selector": "input[name=password]", "text": "secret", "submit": true},
  {"action": "wait_for", "condition": "url_contains", "value": "/dashboard"},
  {"action": "assert", "checks": [{"kind": "text_visible", "text": "Welcome"}]}
]'
```

Use `--summary-only` to reduce output for long test sequences.

Batch actions: `navigate`, `click`, `type`, `key_press`, `wait_for`, `assert`, `click_ref`, `fill_ref`.

**Step 4: Visual regression testing**

Compare screenshots against baselines:

```bash
# First run saves baseline
gsd-browser visual-diff --name "homepage"

# Subsequent runs compare against baseline
gsd-browser visual-diff --name "homepage"
gsd-browser visual-diff --name "homepage" --threshold 0.05    # Stricter
gsd-browser visual-diff --selector "#hero" --name "hero"      # Scope to element
gsd-browser visual-diff --name "homepage" --update-baseline   # Reset baseline
```

**Step 5: Network mocking**

Intercept requests to test error states, loading states, or mock API responses:

```bash
# Mock an API endpoint
gsd-browser mock-route --url "**/api/users" --body '[{"name":"Alice"}]' --status 200

# Simulate server error
gsd-browser mock-route --url "**/api/data" --body '{"error":"server error"}' --status 500

# Simulate slow response
gsd-browser mock-route --url "**/api/data" --body '{"ok":true}' --delay 3000

# Block analytics/ads
gsd-browser block-urls "**/analytics*" "**/ads*"

# Clean up all mocks
gsd-browser clear-routes
```

**Step 6: Device emulation**

Test across device viewports:

```bash
gsd-browser emulate-device "iPhone 15"
gsd-browser navigate https://example.com
gsd-browser visual-diff --name "homepage-mobile"
```

**Warning:** Device emulation recreates the browser context. Page state and cookies are lost.

**Step 7: Test generation**

Record actions and export as Playwright tests:

```bash
# After performing actions...
gsd-browser generate-test --name "login-flow" --output tests/login.spec.ts
```

**Step 8: Performance tracing**

```bash
gsd-browser trace-start --name "checkout-flow"
# ... perform actions ...
gsd-browser trace-stop --name "checkout.json"
gsd-browser har-export --filename "network.har"
```

**Step 9: Prompt injection scanning**

Check untrusted pages for injection attempts:

```bash
gsd-browser navigate https://untrusted-page.com
gsd-browser check-injection
```

</process>

<common_patterns>

<pattern name="full_test_flow">
```bash
gsd-browser navigate https://app.example.com
gsd-browser wait-for --condition network_idle
gsd-browser assert --checks '[
  {"kind": "selector_visible", "selector": "#login-form"},
  {"kind": "no_console_errors"}
]'
gsd-browser fill-form --values '{"Email": "test@example.com", "Password": "secret"}' --submit
gsd-browser wait-for --condition url_contains --value "/dashboard"
gsd-browser assert --checks '[
  {"kind": "text_visible", "text": "Welcome"},
  {"kind": "no_failed_requests"}
]'
```
</pattern>

<pattern name="error_state_testing">
```bash
gsd-browser mock-route --url "**/api/users" --body '{"error":"not found"}' --status 404
gsd-browser navigate https://app.example.com/users
gsd-browser assert --checks '[{"kind": "text_visible", "text": "not found"}]'
gsd-browser clear-routes
```
</pattern>

<pattern name="responsive_testing">
```bash
gsd-browser set-viewport --preset mobile
gsd-browser navigate https://example.com
gsd-browser visual-diff --name "home-mobile"
gsd-browser set-viewport --preset desktop
gsd-browser navigate https://example.com
gsd-browser visual-diff --name "home-desktop"
```
</pattern>

</common_patterns>

<success_criteria>
Testing workflow is complete when:
- All assertions pass
- Visual diffs are within threshold
- Network mocks are cleaned up (`clear-routes`)
- Test artifacts are saved (traces, HARs, generated tests)
- Device-specific tests cover required viewports
</success_criteria>
