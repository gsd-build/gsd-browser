<required_reading>
**Read these reference files NOW:**
1. references/command-reference.md (Diagnostics section)
2. references/error-recovery.md
</required_reading>

<process>

**Step 1: Get a debug bundle**

The fastest way to diagnose any issue — captures screenshot, console logs, network logs, timeline, and accessibility tree in one call:

```bash
gsd-browser debug-bundle
```

**Step 2: Check console logs**

```bash
gsd-browser console                    # Read and clear buffer
gsd-browser console --no-clear         # Read without clearing
```

Console buffer starts fresh on each navigation. Check logs **before** navigating away from the page where the issue occurred.

**Step 3: Check network logs**

```bash
gsd-browser network
```

Look for failed requests (4xx/5xx status), slow requests, or missing requests.

**Step 4: Check dialog events**

```bash
gsd-browser dialog
```

Captures `alert()`, `confirm()`, and `prompt()` dialog events.

**Step 5: Review the action timeline**

```bash
gsd-browser timeline
```

Shows a chronological log of all actions taken in the session — useful for understanding what happened before an error.

**Step 6: Review narrated viewer history**

Use the live viewer when a human needs to inspect the shared browser state, action history, target overlays, or failure markers.

```bash
gsd-browser view
gsd-browser view --history
```

The normal viewer hydrates recent history on reload. `view --history` opens a history-focused view without running a new page action.

**Step 7: Session summary**

```bash
gsd-browser session-summary
```

Diagnostic summary: daemon status, browser info, active page, session duration, action count.

**Step 8: Export for offline analysis**

```bash
gsd-browser har-export --filename "debug-session.har"     # Network as HAR 1.2
gsd-browser trace-start && gsd-browser trace-stop         # CDP performance trace
gsd-browser screenshot --output "debug-screenshot.png"    # Visual state
```

**Step 9: Investigate specific elements**

```bash
gsd-browser find --text "Error"                    # Find error messages
gsd-browser find --selector ".error, .alert"       # Find by selector
gsd-browser snapshot --mode errors                 # Snapshot error elements
gsd-browser accessibility-tree                     # Full semantic tree
gsd-browser eval 'document.querySelector(".error").textContent'  # Extract specific text
```

</process>

<common_patterns>

<pattern name="investigate_failed_action">
```bash
# Something failed — get the full picture
gsd-browser debug-bundle
gsd-browser timeline              # What happened before the failure?
gsd-browser view --history        # Human-readable narrated history
gsd-browser console               # Any JS errors?
gsd-browser network               # Any failed requests?
```
</pattern>

<pattern name="check_page_state">
```bash
gsd-browser screenshot --output state.png
gsd-browser accessibility-tree
gsd-browser snapshot --mode errors
```
</pattern>

<pattern name="export_debug_artifacts">
```bash
gsd-browser debug-bundle
gsd-browser har-export --filename "session.har"
gsd-browser screenshot --full-page --output "full-page.png"
```
</pattern>

</common_patterns>

<success_criteria>
Debugging workflow is complete when:
- Root cause of the issue is identified
- Relevant logs and artifacts are captured
- Corrective action is determined (fix selector, wait longer, handle overlay, etc.)
</success_criteria>
