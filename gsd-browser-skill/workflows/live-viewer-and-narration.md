<required_reading>
**Read these reference files NOW:**
1. references/command-reference.md (Live Viewer and Narration section)
2. references/snapshot-and-refs.md
</required_reading>

<process>

**Step 1: Use a named session**

Use the same `--session` value for every command in a shared viewing flow. This keeps the viewer, daemon, browser page, refs, history, and controls attached to the same session.

```bash
gsd-browser --session demo navigate https://example.com
```

**Step 2: Open the live viewer**

```bash
gsd-browser --session demo view
```

Use `--print-only` when another tool or human will open the URL:

```bash
VIEW_URL=$(gsd-browser --session demo view --print-only | tail -1)
echo "$VIEW_URL"
```

The viewer shows live browser frames, narrated action history, target rings, click ripples, failure markers, and ref overlays. It follows the active page when navigation or tab changes occur.

**Step 3: Drive actions from the CLI**

Keep performing browser actions through CLI commands. The viewer is the shared screen and control surface.

```bash
gsd-browser --session demo snapshot
gsd-browser --session demo click-ref @v1:e1
gsd-browser --session demo type "input[name=email]" "lex@example.com"
gsd-browser --session demo act --intent submit_form
```

**Step 4: Use viewer controls for human-directed work**

The viewer exposes:

| Control | Effect |
|---------|--------|
| Pause | Blocks before the next narrated action |
| Resume | Allows actions to continue |
| Step | Allows one action, then returns to paused mode |
| Abort | Aborts the next gated action |
| Refs overlay | Shows or hides target boxes/labels for visible interactive elements |

Keyboard shortcuts in the viewer:

| Key | Effect |
|-----|--------|
| Space | Pause/resume |
| Right Arrow | Step |
| Escape | Abort |
| R | Toggle refs overlay |

**Step 5: Set a visible goal banner**

Use `goal` when a human is watching and needs to understand what the agent is trying to accomplish.

```bash
gsd-browser --session demo goal "Find the pricing link"
gsd-browser --session demo goal --clear
```

**Step 6: Review history**

The normal viewer hydrates recent narrated history on reload. Use the history-focused mode when the user wants a clean action log.

```bash
gsd-browser --session demo view --history
gsd-browser --session demo view --history --print-only
```

History items show pending, success, and failure states. Hover history items in the viewer to preview the captured frame around the action.

**Step 7: Use no-delay for fast agent-only runs**

`--no-narration-delay` keeps narration events/history but skips lead-time sleeps before actions. Use it when speed matters and no human needs the cursor lead-in animation.

```bash
gsd-browser --session demo --no-narration-delay click "h1"
```

**Step 8: Stop cleanly**

```bash
gsd-browser --session demo daemon stop
```

</process>

<common_patterns>

<pattern name="human_watches_agent_work">
```bash
gsd-browser --session watched navigate https://example.com
gsd-browser --session watched view
gsd-browser --session watched goal "Inspect the signup flow"
gsd-browser --session watched snapshot
gsd-browser --session watched click-ref @v1:e1
```
</pattern>

<pattern name="user_directs_one_step_at_a_time">
```bash
gsd-browser --session directed navigate https://example.com
gsd-browser --session directed view
# User clicks Pause in the viewer.
gsd-browser --session directed click "a"
# User clicks Step to allow exactly one action.
```
</pattern>

<pattern name="share_history_without_new_actions">
```bash
gsd-browser --session demo view --history --print-only
```
</pattern>

<pattern name="fast_run_with_history">
```bash
gsd-browser --session fast view --print-only
gsd-browser --session fast --no-narration-delay navigate https://example.com
gsd-browser --session fast --no-narration-delay click "h1"
```
</pattern>

</common_patterns>

<success_criteria>

Live viewer task is complete when:
- The viewer is open or its URL was returned to the caller.
- The requested browser state is visible in the shared view.
- History shows the relevant action outcomes.
- Pause/step/resume/abort state is clear when user-directed control is involved.
- The named session is stopped when no further browser work is needed.

</success_criteria>
