---
name: gsd-browser
description: >
  Native Rust browser automation CLI for AI agents. Use when the user needs to
  interact with websites, navigate pages, fill forms, click buttons, take
  screenshots, extract structured data, run assertions, test web apps, mock
  network requests, or automate any browser task. Triggers include "open a
  website", "fill out a form", "take a screenshot", "scrape data", "test this
  web app", "login to a site", "visual regression test", or any task requiring
  programmatic web interaction.
allowed-tools: Bash(gsd-browser:*), Bash(gsd-browser *)
---

<essential_principles>

**The daemon auto-starts.** Never run `gsd-browser daemon start`. It launches on first command. Only use `daemon stop` to clean up when finished.

**Always re-snapshot after page changes.** Refs are versioned (`@v1:e1`, `@v2:e3`). After navigation, form submission, or dynamic content loading, old refs are stale. Run `gsd-browser snapshot` to get fresh refs before interacting.

**Use `--json` when parsing output.** Use text mode when reading output yourself. Use `--json` when you need to extract values programmatically.

**Positional args have no flag prefix.** Commands like `click`, `type`, `hover` take positional args. Do NOT add `--selector`:
- `gsd-browser click "button.submit"` (correct)
- `gsd-browser click --selector "button.submit"` (WRONG)

**Core workflow pattern:** Every browser automation follows: navigate -> snapshot -> interact -> re-snapshot (after DOM changes).

```bash
gsd-browser navigate https://example.com
gsd-browser snapshot
# Read snapshot output: @v1:e1 [input type="email"], @v1:e2 [button] "Submit"
gsd-browser fill-ref @v1:e1 "user@example.com"
gsd-browser click-ref @v1:e2
gsd-browser wait-for --condition network_idle
gsd-browser snapshot  # REQUIRED - old refs are now stale
```

**Command chaining:** Use `&&` when you don't need intermediate output. Run separately when you need to parse output first (e.g., snapshot to discover refs, then interact).

**Global options** available on all commands:

| Flag | Purpose |
|------|---------|
| `--json` | Structured JSON output |
| `--browser-path <path>` | Path to Chrome/Chromium |
| `--session <name>` | Named session for parallel instances |

</essential_principles>

<routing>

Based on what the user needs, read the appropriate workflow:

| User intent | Workflow |
|-------------|----------|
| Navigate, click, type, fill forms, interact with pages | `workflows/navigate-and-interact.md` |
| Scrape data, extract content, read page structure | `workflows/scrape-and-extract.md` |
| Test pages, run assertions, visual regression, mock network | `workflows/test-and-assert.md` |
| Debug issues, check logs, diagnose problems | `workflows/debug-and-diagnose.md` |
| Install, configure, set up sessions | `workflows/setup-and-configure.md` |

**After reading the workflow, follow it. Load references only when the workflow directs you to.**

</routing>

<reference_index>

All domain knowledge in `references/`:

**Commands:** command-reference.md (all 63 commands with exact syntax)
**Snapshots:** snapshot-and-refs.md (versioned refs, snapshot modes)
**Intents:** semantic-intents.md (15 predefined intents for find-best/act)
**Errors:** error-recovery.md (common errors and fixes)
**Config:** configuration.md (TOML config, env vars, 5-layer merge)

</reference_index>

<workflows_index>

| Workflow | Purpose |
|----------|---------|
| navigate-and-interact.md | Page navigation, clicking, typing, forms, intents |
| scrape-and-extract.md | Data extraction, accessibility tree, page source |
| test-and-assert.md | Assertions, visual regression, network mocking, test generation |
| debug-and-diagnose.md | Console/network logs, timeline, debug bundles |
| setup-and-configure.md | Installation, configuration, sessions, daemon management |

</workflows_index>

<success_criteria>

Browser automation task is complete when:
- Target page state is achieved and verified (via assertions or visual confirmation)
- Daemon is stopped if no further browser work is needed (`gsd-browser daemon stop`)
- Extracted data is returned in the expected format
- Any saved state (auth, cookies) is persisted for reuse if appropriate

</success_criteria>
