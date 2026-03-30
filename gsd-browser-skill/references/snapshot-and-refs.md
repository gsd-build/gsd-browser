<overview>
Snapshots assign versioned refs to interactive page elements. Refs are the primary mechanism for deterministic element interaction — they eliminate fragile CSS selectors by giving each element a stable, versioned identifier.
</overview>

<how_refs_work>

Running `gsd-browser snapshot` scans the page and assigns refs like `@v1:e1`, `@v1:e2`, etc.

- The **version** (`v1`, `v2`, ...) increments with each snapshot call
- The **element** (`e1`, `e2`, ...) is a unique ID within that version
- Refs map to specific DOM elements at snapshot time

```
@v1:e1  [input type="email"] placeholder="Email"
@v1:e2  [input type="password"] placeholder="Password"
@v1:e3  [button] "Sign In"
@v1:e4  [a] "Forgot password?"
```

</how_refs_work>

<staleness_rule>

**Refs become stale when the page changes.** This includes:
- Navigation to a new URL
- Form submission
- Dynamic content loading (AJAX, SPA transitions)
- Modal/dialog open or close

After any of these, **always re-snapshot before interacting**:

```bash
gsd-browser click-ref @v1:e3            # Submit form
gsd-browser wait-for --condition network_idle
gsd-browser snapshot                     # Get fresh refs (@v2:eN)
gsd-browser click-ref @v2:e1            # Use new version
```

If you use a stale ref, you'll get: `Error: resolve_ref: JS evaluation failed: ref @v1:e3 not found`

</staleness_rule>

<snapshot_modes>

| Mode | What it captures | Use when |
|------|-----------------|----------|
| `interactive` | Buttons, inputs, links, selects | General interaction (default) |
| `form` | Form fields with labels and current values | Filling out forms |
| `dialog` | Elements inside open dialogs/modals | Interacting with modals |
| `navigation` | Links and nav elements | Finding navigation paths |
| `errors` | Error messages, validation warnings | Checking for errors |
| `headings` | Heading elements (h1-h6) | Understanding page structure |
| `visible_only` | All visible elements | When you need everything |

```bash
gsd-browser snapshot --mode form --selector "#signup-form"
```

</snapshot_modes>

<scoping_and_limits>

Scope a snapshot to a specific section:

```bash
gsd-browser snapshot --selector "#login-form"
gsd-browser snapshot --selector "main"
```

Increase the element limit (default: 40):

```bash
gsd-browser snapshot --limit 80
```

</scoping_and_limits>

<ref_commands>

| Command | Purpose |
|---------|---------|
| `get-ref <ref>` | Get metadata for a ref (role, name, selector) |
| `click-ref <ref>` | Click the element |
| `hover-ref <ref>` | Hover over the element |
| `fill-ref <ref> <text>` | Type text into the element |

All ref args are **positional** — no `--ref` flag.

</ref_commands>
