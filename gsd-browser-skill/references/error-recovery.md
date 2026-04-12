<overview>
Common errors and their fixes. When an error occurs, match it against these patterns before attempting custom debugging.
</overview>

<error name="stale_refs">

**Error:** `resolve_ref: JS evaluation failed: ref @v1:e3 not found`

**Cause:** Refs become stale after page changes (navigation, form submission, dynamic content).

**Fix:** Re-snapshot and use the new version:

```bash
gsd-browser snapshot
gsd-browser click-ref @v2:e1       # Use fresh version number
```

</error>

<error name="click_timeout">

**Error:** `click timed out after 10s for: #submit-btn`

**Cause:** Element not visible, behind an overlay, or doesn't exist.

**Fix:**

```bash
gsd-browser find --selector "#submit-btn"                            # Verify exists
gsd-browser scroll --direction down                                  # Scroll into view
gsd-browser wait-for --condition selector_visible --value "#submit-btn"  # Wait for it
gsd-browser click "#submit-btn"                                      # Retry
```

If blocked by an overlay:

```bash
gsd-browser act --intent accept_cookies
gsd-browser act --intent dismiss
```

</error>

<error name="empty_logs">

**Error:** Console or network logs return empty.

**Cause:** Buffers start fresh on each navigation. Logs from the previous page are gone.

**Fix:** Check logs **before** navigating away:

```bash
gsd-browser navigate https://example.com
gsd-browser eval "fetch('/api/data')"
gsd-browser network                    # Check BEFORE next navigation
```

</error>

<error name="session_about_blank">

**Error:** `session-summary` or `list-pages` shows `about:blank` after navigating.

**Cause:** On versions before v0.1.7, the page registry did not sync metadata after navigation. The browser was on the correct page but the registry still reported the initial `about:blank`.

**Fix:** Upgrade to v0.1.7+. If stuck on an older version, use batch to keep everything in one daemon connection:

```bash
gsd-browser batch --steps '[
  {"action": "navigate", "url": "https://example.com"},
  {"action": "assert", "checks": [{"kind": "url_contains", "text": "example"}]}
]'
```

</error>

<error name="daemon_wont_start">

**Error:** `daemon did not start within 10s`

**Cause:** Stale Chrome lock file or zombie process.

**Fix:**

```bash
gsd-browser daemon stop
rm -f /tmp/chromiumoxide-runner/SingletonLock
gsd-browser daemon health              # Retry (auto-starts)
```

</error>

<error name="vault_not_encrypted">

**Error:** Vault operations fail or credentials stored in plaintext.

**Cause:** `GSD_BROWSER_VAULT_KEY` not set, or set after daemon started.

**Fix:**

```bash
export GSD_BROWSER_VAULT_KEY="your-encryption-key"
gsd-browser daemon stop                # Stop existing daemon
gsd-browser vault-save --profile ...   # Daemon restarts with key
```

</error>

<error name="device_emulation_lost_state">

**Error:** Cookies and page state gone after device emulation.

**Cause:** `emulate-device` recreates the browser context.

**Fix:** Save state before emulating, restore after:

```bash
gsd-browser save-state --name "pre-emulation"
gsd-browser emulate-device "iPhone 15"
gsd-browser restore-state --name "pre-emulation"
```

</error>

<error name="selector_not_found">

**Error:** Element not found by selector.

**Cause:** Wrong selector, element not loaded yet, or element is in an iframe.

**Fix:**

```bash
# Wait for it
gsd-browser wait-for --condition selector_visible --value "#target" --timeout 30000

# Check if it's in an iframe
gsd-browser list-frames
gsd-browser select-frame --name "content-frame"
gsd-browser find --selector "#target"

# Use accessibility tree to discover the right selector
gsd-browser accessibility-tree
```

</error>

<general_debugging_strategy>

When something fails unexpectedly:

1. **Get the full picture:** `gsd-browser debug-bundle`
2. **Check the timeline:** `gsd-browser timeline` — what happened before the error?
3. **Check console:** `gsd-browser console` — any JS errors?
4. **Check network:** `gsd-browser network` — any failed requests?
5. **Take a screenshot:** `gsd-browser screenshot --output debug.png` — what does the page look like?
6. **Check for overlays:** `gsd-browser act --intent dismiss` — is something blocking interaction?

</general_debugging_strategy>
