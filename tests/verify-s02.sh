#!/usr/bin/env bash
# End-to-end verification for S02: Interaction + Inspection + Screenshots
# Tests log infrastructure, interaction commands, screenshots, inspection commands,
# error cases, and a realistic agent workflow pipeline against live pages.

set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Build first
echo "=== Building workspace ==="
cargo build --workspace --manifest-path "$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1

BIN="$PROJECT_DIR/target/debug/gsd-browser"
PASS=0
FAIL=0
TOTAL=0

check() {
    local desc="$1"
    local result="$2"  # 0 = pass, nonzero = fail
    TOTAL=$((TOTAL + 1))
    if [ "$result" -eq 0 ]; then
        PASS=$((PASS + 1))
        echo "  ✅ $desc"
    else
        FAIL=$((FAIL + 1))
        echo "  ❌ $desc"
    fi
}

cleanup_daemon() {
    # Kill daemon by PID file
    if [ -f ~/.gsd-browser/daemon.pid ]; then
        local pid
        pid=$(cat ~/.gsd-browser/daemon.pid 2>/dev/null)
        if [ -n "$pid" ]; then
            kill "$pid" 2>/dev/null || true
        fi
    fi
    # Kill any stale Chrome instances from chromiumoxide
    pkill -f "chromiumoxide-runner" 2>/dev/null || true
    sleep 2
    # Remove singleton lock (macOS-specific path varies)
    find /private/var/folders -name "SingletonLock" -path "*/chromiumoxide-runner/*" -delete 2>/dev/null || true
    rm -f ~/.gsd-browser/daemon.sock ~/.gsd-browser/daemon.pid
}

cleanup_temp_files() {
    rm -f /tmp/bt-test-viewport.jpg /tmp/bt-test-element.png /tmp/bt-test-full.jpg
}

# ── Cleanup any existing daemon ──
echo ""
echo "=== Setup ==="
cleanup_daemon
cleanup_temp_files
echo "  Cleaned up stale daemon files and temp screenshots"

# ── Navigate to baseline page (cold start — daemon auto-starts) ──
echo ""
echo "=== Baseline Navigation ==="
NAV_OUTPUT=$("$BIN" navigate https://example.com 2>&1) || true
echo "$NAV_OUTPUT" | grep -q "Example Domain" 2>/dev/null
check "navigate to example.com (baseline)" $?

# ════════════════════════════════════════════
#  Log Infrastructure (4 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Log Infrastructure ==="

# eval "document.title" returns expected title
EVAL_TITLE=$("$BIN" eval "document.title" 2>&1) || true
echo "$EVAL_TITLE" | grep -q "Example Domain" 2>/dev/null
check "eval document.title returns 'Example Domain'" $?

# eval "1+1" returns 2
EVAL_MATH=$("$BIN" eval "1+1" 2>&1) || true
echo "$EVAL_MATH" | grep -q "2" 2>/dev/null
check "eval 1+1 returns 2" $?

# console --json returns valid JSON with entries array
CONSOLE_JSON=$("$BIN" --json console 2>&1) || true
echo "$CONSOLE_JSON" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'entries' in d" 2>/dev/null
check "console --json returns valid JSON with entries array" $?

# network --json returns valid JSON with entries array
NETWORK_JSON=$("$BIN" --json network 2>&1) || true
echo "$NETWORK_JSON" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'entries' in d" 2>/dev/null
check "network --json returns valid JSON with entries array" $?

# ════════════════════════════════════════════
#  Interaction Commands (9 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Interaction Commands ==="

# Ensure we're on example.com before clicking
"$BIN" navigate https://example.com > /dev/null 2>&1 || true

# click 'a' — clicks the "More information..." link
CLICK_OUTPUT=$("$BIN" click a 2>&1) || true
echo "$CLICK_OUTPUT" | grep -qi "clicked\|page summary\|title" 2>/dev/null
check "click 'a' returns successfully with output" $?

# Navigate back for more tests
"$BIN" navigate https://example.com > /dev/null 2>&1 || true

# scroll --direction down --amount 300
SCROLL_OUTPUT=$("$BIN" scroll --direction down --amount 300 2>&1) || true
echo "$SCROLL_OUTPUT" | grep -qi "scroll\|position\|px" 2>/dev/null
check "scroll down 300px returns scroll info" $?

# press Enter — verify returns successfully
PRESS_OUTPUT=$("$BIN" press Enter 2>&1)
PRESS_EXIT=$?
check "press Enter exits successfully" $PRESS_EXIT

# set-viewport --preset mobile — verify width=375
VIEWPORT_MOBILE=$("$BIN" set-viewport --preset mobile 2>&1) || true
echo "$VIEWPORT_MOBILE" | grep -q "375" 2>/dev/null
check "set-viewport --preset mobile returns width 375" $?

# set-viewport --width 1280 --height 720 — verify width=1280
VIEWPORT_CUSTOM=$("$BIN" set-viewport --width 1280 --height 720 2>&1) || true
echo "$VIEWPORT_CUSTOM" | grep -q "1280" 2>/dev/null
check "set-viewport --width 1280 --height 720 returns width 1280" $?

# hover 'a' — verify returns successfully
HOVER_OUTPUT=$("$BIN" hover a 2>&1)
HOVER_EXIT=$?
check "hover 'a' exits successfully" $HOVER_EXIT

# hover 'a' output contains hovered state
echo "$HOVER_OUTPUT" | grep -qi "hover\|page summary\|title" 2>/dev/null
check "hover 'a' output contains state info" $?

# --json mode check for an interaction command (click)
"$BIN" navigate https://example.com > /dev/null 2>&1 || true
CLICK_JSON=$("$BIN" --json click a 2>&1) || true
echo "$CLICK_JSON" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'clicked' in d or 'state' in d" 2>/dev/null
check "click --json returns valid JSON with click/state data" $?

# ════════════════════════════════════════════
#  Screenshot Commands (4 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Screenshot Commands ==="

# Navigate to a known page for screenshots
"$BIN" navigate https://example.com > /dev/null 2>&1 || true
sleep 1

# screenshot --output viewport JPEG
"$BIN" screenshot --output /tmp/bt-test-viewport.jpg 2>&1 || true
if [ -f /tmp/bt-test-viewport.jpg ] && file /tmp/bt-test-viewport.jpg | grep -qi "jpeg\|JFIF" 2>/dev/null; then
    check "screenshot --output viewport.jpg exists and is JPEG" 0
else
    check "screenshot --output viewport.jpg exists and is JPEG" 1
fi

# screenshot --selector 'h1' element PNG
"$BIN" screenshot --selector h1 --output /tmp/bt-test-element.png --format png 2>&1 || true
if [ -f /tmp/bt-test-element.png ] && file /tmp/bt-test-element.png | grep -qi "png\|PNG" 2>/dev/null; then
    check "screenshot --selector h1 element.png exists and is PNG" 0
else
    check "screenshot --selector h1 element.png exists and is PNG" 1
fi

# screenshot --full-page
"$BIN" screenshot --full-page --output /tmp/bt-test-full.jpg 2>&1 || true
if [ -f /tmp/bt-test-full.jpg ] && file /tmp/bt-test-full.jpg | grep -qi "jpeg\|JFIF" 2>/dev/null; then
    check "screenshot --full-page full.jpg exists and is JPEG" 0
else
    check "screenshot --full-page full.jpg exists and is JPEG" 1
fi

# screenshot --json — verify JSON has data and mimeType fields
SCREENSHOT_JSON=$("$BIN" --json screenshot 2>&1) || true
echo "$SCREENSHOT_JSON" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'data' in d and 'mimeType' in d" 2>/dev/null
check "screenshot --json returns JSON with data and mimeType" $?

# ════════════════════════════════════════════
#  Inspection Commands (5 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Inspection Commands ==="

# Navigate to known page for inspection
"$BIN" navigate https://example.com > /dev/null 2>&1 || true
sleep 1

# accessibility-tree — verify output contains recognizable roles
A11Y_OUTPUT=$("$BIN" accessibility-tree 2>&1) || true
echo "$A11Y_OUTPUT" | grep -qi "link\|heading" 2>/dev/null
check "accessibility-tree contains recognizable roles (link/heading)" $?

# accessibility-tree --json — verify JSON has tree and nodeCount
A11Y_JSON=$("$BIN" --json accessibility-tree 2>&1) || true
echo "$A11Y_JSON" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'tree' in d and 'nodeCount' in d" 2>/dev/null
check "accessibility-tree --json has tree and nodeCount" $?

# find --role link — verify at least one result
FIND_ROLE=$("$BIN" find --role link 2>&1) || true
echo "$FIND_ROLE" | grep -qi "link\|element" 2>/dev/null
check "find --role link returns at least one result" $?

# find --text "Example" — verify at least one result
FIND_TEXT=$("$BIN" find --text Example 2>&1) || true
echo "$FIND_TEXT" | grep -qi "example\|element" 2>/dev/null
check "find --text 'Example' returns at least one result" $?

# page-source — verify output contains <html or <!doctype
PAGE_SRC=$("$BIN" page-source 2>&1) || true
echo "$PAGE_SRC" | grep -qi "<html\|<!doctype" 2>/dev/null
check "page-source contains <html or <!doctype" $?

# ════════════════════════════════════════════
#  Error Cases (3 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Error Cases ==="

# click --selector '.nonexistent-element-xyz' — should exit non-zero
# Note: click uses a positional arg for selector, but we can pass the CSS selector directly
"$BIN" click '.nonexistent-element-xyz' > /dev/null 2>&1
CLICK_ERR_EXIT=$?
if [ "$CLICK_ERR_EXIT" -ne 0 ]; then
    check "click nonexistent selector exits non-zero" 0
else
    check "click nonexistent selector exits non-zero (got exit $CLICK_ERR_EXIT)" 1
fi

# eval with no expression — should exit non-zero (clap validation)
"$BIN" eval > /dev/null 2>&1
EVAL_ERR_EXIT=$?
if [ "$EVAL_ERR_EXIT" -ne 0 ]; then
    check "eval with no expression exits non-zero" 0
else
    check "eval with no expression exits non-zero (got exit $EVAL_ERR_EXIT)" 1
fi

# screenshot --selector '.nonexistent' — should exit non-zero
"$BIN" screenshot --selector '.nonexistent-element-xyz' > /dev/null 2>&1
SCREENSHOT_ERR_EXIT=$?
if [ "$SCREENSHOT_ERR_EXIT" -ne 0 ]; then
    check "screenshot nonexistent selector exits non-zero" 0
else
    check "screenshot nonexistent selector exits non-zero (got exit $SCREENSHOT_ERR_EXIT)" 1
fi

# ════════════════════════════════════════════
#  Realistic Agent Pipeline (4 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Agent Workflow Pipeline ==="

# Navigate → interact → screenshot → inspect logs
# Simulates: navigate to page, hover element, screenshot it, check network & console

"$BIN" navigate https://example.com > /dev/null 2>&1 || true
sleep 1

# Hover over the link
PIPE_HOVER=$("$BIN" hover a 2>&1)
PIPE_HOVER_EXIT=$?
check "pipeline: hover element succeeds" $PIPE_HOVER_EXIT

# Screenshot the result
"$BIN" screenshot --output /tmp/bt-test-viewport.jpg > /dev/null 2>&1 || true
if [ -f /tmp/bt-test-viewport.jpg ]; then
    check "pipeline: screenshot after interaction saved" 0
else
    check "pipeline: screenshot after interaction saved" 1
fi

# Inspect network logs after pipeline
PIPE_NET=$("$BIN" --json network 2>&1) || true
echo "$PIPE_NET" | python3 -c "import sys, json; d = json.load(sys.stdin); assert isinstance(d.get('entries'), list)" 2>/dev/null
check "pipeline: network --json returns entries list" $?

# Inspect console logs after pipeline
PIPE_CON=$("$BIN" --json console 2>&1) || true
echo "$PIPE_CON" | python3 -c "import sys, json; d = json.load(sys.stdin); assert isinstance(d.get('entries'), list)" 2>/dev/null
check "pipeline: console --json returns entries list" $?

# ════════════════════════════════════════════
#  Cleanup
# ════════════════════════════════════════════
echo ""
echo "=== Cleanup ==="
cleanup_daemon
cleanup_temp_files
echo "  Daemon stopped and temp files removed"

# ── Summary ──
echo ""
echo "════════════════════════════════"
echo "  Results: $PASS/$TOTAL passed, $FAIL failed"
echo "════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
    echo "  ❌ FAIL"
    exit 1
else
    echo "  ✅ ALL PASS"
    exit 0
fi
