<overview>
Semantic intents allow interaction by purpose rather than selector. `find-best` returns scored candidates. `act` finds the best match and clicks/focuses it in one call. Intents are predefined categories, not free-form text.
</overview>

<intent_table>

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
| `main_content` | click | Main content area (requires semantic markup) |
| `pagination_next` | click | Next page in pagination |
| `pagination_prev` | click | Previous page in pagination |

</intent_table>

<usage>

**Find candidates (returns scored matches with selectors):**

```bash
gsd-browser find-best --intent submit_form
gsd-browser find-best --intent accept_cookies --scope "#modal"
```

**Act (find + click/focus in one call):**

```bash
gsd-browser act --intent submit_form
gsd-browser act --intent accept_cookies
gsd-browser act --intent auth_action
gsd-browser act --intent close_dialog
```

</usage>

<when_to_use_intents>

Use intents when:
- You don't know the exact selector (cookie banners vary per site)
- You want resilient interaction that adapts to different page layouts
- The intent maps cleanly to a predefined category

Use refs or selectors when:
- You need precision (specific field among many)
- The element doesn't map to any predefined intent
- You've already snapshotted and have the ref

</when_to_use_intents>

<action_cache>

Intents can be cached to avoid repeated lookups on the same page:

```bash
gsd-browser action-cache --action put --intent submit_form --selector "#submit-btn" --score 0.95
gsd-browser action-cache --action get --intent submit_form
gsd-browser action-cache --action stats
gsd-browser action-cache --action clear
```

Cache is per-session and cleared on navigation.

</action_cache>
