<required_reading>
**Read these reference files NOW:**
1. references/command-reference.md (Navigation, Interaction, Snapshot & Refs, Forms sections)
2. references/snapshot-and-refs.md
3. references/semantic-intents.md (if using intent-based interaction)
</required_reading>

<process>

**Step 1: Navigate to the target page**

```bash
gsd-browser navigate <url>
```

Wait for the page to settle if it has dynamic content:

```bash
gsd-browser wait-for --condition network_idle
```

**Step 2: Handle overlays and cookie banners**

Many sites show consent banners that block interaction. Dismiss them first:

```bash
gsd-browser act --intent accept_cookies
gsd-browser act --intent dismiss          # For generic overlays
```

**Step 3: Snapshot the page**

```bash
gsd-browser snapshot
```

Read the output to discover versioned refs like `@v1:e1`, `@v1:e2`. Each ref maps to an interactive element (buttons, inputs, links).

Use snapshot modes to focus on specific element types:

| Mode | Use when |
|------|----------|
| `interactive` | General interaction (default) |
| `form` | Filling out forms — shows labels and current values |
| `dialog` | A modal/dialog is open |
| `navigation` | Finding nav links |
| `errors` | Checking for validation errors |

```bash
gsd-browser snapshot --mode form
gsd-browser snapshot --selector "#login-form"    # Scope to a section
gsd-browser snapshot --limit 80                  # Increase from default 40
```

**Step 4: Interact using refs**

```bash
gsd-browser fill-ref @v1:e1 "user@example.com"
gsd-browser fill-ref @v1:e2 "password123"
gsd-browser click-ref @v1:e3
```

Or interact by CSS selector:

```bash
gsd-browser click "button.submit"
gsd-browser type "input[name=email]" "user@example.com"
gsd-browser hover ".menu-trigger"
```

**Step 5: Wait for page changes**

After clicking or submitting, wait for the result:

```bash
gsd-browser wait-for --condition url_contains --value "/dashboard"
gsd-browser wait-for --condition text_visible --value "Welcome"
gsd-browser wait-for --condition selector_hidden --value ".spinner"
gsd-browser wait-for --condition network_idle
```

**Step 6: Re-snapshot**

After any page change, old refs are stale. Always re-snapshot:

```bash
gsd-browser snapshot
```

**Step 7: Smart form filling (alternative to refs)**

For forms, skip manual ref-finding with `fill-form`:

```bash
gsd-browser analyze-form                    # Discover field labels
gsd-browser fill-form --values '{"Email": "a@b.com", "Password": "secret"}' --submit
```

Fields are matched by label, name, placeholder, or aria-label. No selectors needed.

**Step 8: Intent-based interaction (alternative to refs/selectors)**

For common actions, use semantic intents:

```bash
gsd-browser act --intent submit_form
gsd-browser act --intent auth_action        # Login/signup buttons
gsd-browser act --intent close_dialog
gsd-browser act --intent next_step
```

See `references/semantic-intents.md` for all 15 intents.

</process>

<common_patterns>

<pattern name="login_flow_with_refs">
```bash
gsd-browser navigate https://app.example.com/login
gsd-browser act --intent accept_cookies
gsd-browser snapshot
gsd-browser fill-ref @v1:e1 "$USERNAME"
gsd-browser fill-ref @v1:e2 "$PASSWORD"
gsd-browser click-ref @v1:e3
gsd-browser wait-for --condition url_contains --value "/dashboard"
gsd-browser save-state --name "myapp-auth"
```
</pattern>

<pattern name="login_flow_with_vault">
```bash
# Save once:
gsd-browser vault-save --profile myapp \
  --url https://app.example.com/login \
  --username user@example.com \
  --password "$PASSWORD"

# Reuse in any session:
gsd-browser vault-login --profile myapp
gsd-browser wait-for --condition url_contains --value "/dashboard"
```
</pattern>

<pattern name="form_submission">
```bash
gsd-browser navigate https://example.com/signup
gsd-browser analyze-form
gsd-browser fill-form --values '{"Full Name": "Jane Doe", "Email": "jane@example.com", "State": "California"}' --submit
gsd-browser wait-for --condition network_idle
```
</pattern>

<pattern name="multi_page_navigation">
```bash
gsd-browser navigate https://example.com
gsd-browser click "a.products-link"
gsd-browser wait-for --condition network_idle
gsd-browser snapshot
gsd-browser click-ref @v2:e5
gsd-browser wait-for --condition text_visible --value "Product Details"
```
</pattern>

<pattern name="reuse_saved_auth">
```bash
gsd-browser restore-state --name "myapp-auth"
gsd-browser navigate https://app.example.com/dashboard
```
</pattern>

</common_patterns>

<success_criteria>
Navigation and interaction workflow is complete when:
- Target page is loaded and verified
- All form fields are filled correctly
- Expected navigation occurred (URL changed, content visible)
- State is saved if login/auth was performed
</success_criteria>
