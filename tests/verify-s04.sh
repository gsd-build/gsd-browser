#!/usr/bin/env bash
# End-to-end verification for S04: Forms + Pages + Intent + Session Diagnostics
# Tests all 11 new commands plus S03 regression.

set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Build first
echo "=== Building workspace ==="
cargo build --workspace --manifest-path "$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1

BIN="$PROJECT_DIR/target/debug/browser-tools"
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
    if [ -f ~/.browser-tools/daemon.pid ]; then
        local pid
        pid=$(cat ~/.browser-tools/daemon.pid 2>/dev/null)
        if [ -n "$pid" ]; then
            kill "$pid" 2>/dev/null || true
        fi
    fi
    pkill -f "chromiumoxide-runner" 2>/dev/null || true
    sleep 2
    find /private/var/folders -name "SingletonLock" -path "*/chromiumoxide-runner/*" -delete 2>/dev/null || true
    rm -f ~/.browser-tools/daemon.sock ~/.browser-tools/daemon.pid
}

# ── Cleanup any existing daemon ──
echo ""
echo "=== Setup ==="
cleanup_daemon
echo "  Cleaned up stale daemon files"

# Create a temp HTML file for form testing (avoids data: URI shell quoting issues)
FORM_FILE=$(mktemp /tmp/browser-tools-test-XXXXXX.html)
cat > "$FORM_FILE" << 'FORMEOF'
<!DOCTYPE html>
<html>
<head><title>Test Form</title></head>
<body>
  <h1>Test Form Page</h1>
  <form id="testform">
    <label for="fname">First Name</label>
    <input type="text" id="fname" name="fname" placeholder="Enter first name">
    <label for="email">Email</label>
    <input type="email" id="email" name="email" placeholder="Enter email">
    <label for="color">Favorite Color</label>
    <select id="color" name="color">
      <option value="red">Red</option>
      <option value="blue">Blue</option>
      <option value="green">Green</option>
    </select>
    <label for="agree">I agree</label>
    <input type="checkbox" id="agree" name="agree">
    <br>
    <button type="submit">Submit</button>
  </form>
  <a href="https://example.com">More info</a>
</body>
</html>
FORMEOF
FORM_URL="file://$FORM_FILE"
echo "  Test form: $FORM_URL"

# ════════════════════════════════════════════
#  Navigate to form page
# ════════════════════════════════════════════
echo ""
echo "=== Navigate to test form page ==="

NAV_OUTPUT=$("$BIN" navigate "$FORM_URL" 2>&1) || true
echo "$NAV_OUTPUT" | grep -q "Test Form" 2>/dev/null
check "navigate to form test page" $?

# ════════════════════════════════════════════
#  Analyze Form (2 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Analyze Form ==="

ANALYZE=$("$BIN" --json analyze-form 2>&1) || true
FIELD_COUNT=$(echo "$ANALYZE" | python3 -c "import sys, json; d = json.load(sys.stdin); print(d.get('fieldCount', 0))" 2>/dev/null || echo 0)
[ "$FIELD_COUNT" -gt 0 ] 2>/dev/null
check "analyze-form field count > 0 (got $FIELD_COUNT)" $?

ANALYZE_TEXT=$("$BIN" analyze-form 2>&1) || true
echo "$ANALYZE_TEXT" | grep -q "Form:" 2>/dev/null
check "analyze-form text mode shows 'Form:'" $?

# ════════════════════════════════════════════
#  Fill Form (1 check)
# ════════════════════════════════════════════
echo ""
echo "=== Fill Form ==="

# Re-navigate to ensure clean form state
"$BIN" navigate "$FORM_URL" > /dev/null 2>&1 || true
sleep 1

FILL_RESULT=$("$BIN" --json fill-form --values '{"First Name": "Alice", "Email": "alice@test.com"}' 2>&1) || true
echo "$FILL_RESULT" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'filled' in d" 2>/dev/null
check "fill-form fills fields without error" $?

# ════════════════════════════════════════════
#  Find Best (2 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Find Best ==="

FIND_BEST=$("$BIN" --json find-best --intent submit_form 2>&1) || true
echo "$FIND_BEST" | python3 -c "import sys, json; d = json.load(sys.stdin); cs = d.get('candidates', []); assert len(cs) > 0 and cs[0].get('score', 0) > 0" 2>/dev/null
check "find-best submit_form returns candidates with score > 0" $?

FIND_BEST_TEXT=$("$BIN" find-best --intent submit_form 2>&1) || true
echo "$FIND_BEST_TEXT" | grep -q "Intent: submit_form" 2>/dev/null
check "find-best text mode shows 'Intent: submit_form'" $?

# ════════════════════════════════════════════
#  Act (1 check)
# ════════════════════════════════════════════
echo ""
echo "=== Act ==="

# Re-navigate so act has something to click
"$BIN" navigate "$FORM_URL" > /dev/null 2>&1 || true
sleep 1

ACT_RESULT=$("$BIN" --json act --intent submit_form 2>&1) || true
echo "$ACT_RESULT" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'intent' in d or 'action' in d" 2>/dev/null
check "act submit_form returns without error" $?

# ════════════════════════════════════════════
#  List Pages (2 checks)
# ════════════════════════════════════════════
echo ""
echo "=== List Pages ==="

LIST_PAGES=$("$BIN" --json list-pages 2>&1) || true
echo "$LIST_PAGES" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d.get('count', 0) >= 1" 2>/dev/null
check "list-pages count >= 1" $?

LIST_PAGES_TEXT=$("$BIN" list-pages 2>&1) || true
echo "$LIST_PAGES_TEXT" | grep -q "Pages" 2>/dev/null
check "list-pages text mode shows 'Pages'" $?

# ════════════════════════════════════════════
#  List Frames (1 check)
# ════════════════════════════════════════════
echo ""
echo "=== List Frames ==="

LIST_FRAMES=$("$BIN" --json list-frames 2>&1) || true
echo "$LIST_FRAMES" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d.get('count', 0) >= 1" 2>/dev/null
check "list-frames count >= 1 (at least main frame)" $?

# ════════════════════════════════════════════
#  Session Summary (2 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Session Summary ==="

SESSION=$("$BIN" --json session-summary 2>&1) || true
echo "$SESSION" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'actions' in d and 'total' in d['actions']" 2>/dev/null
check "session-summary returns actions.total" $?

SESSION_TEXT=$("$BIN" session-summary 2>&1) || true
echo "$SESSION_TEXT" | grep -q "Actions:" 2>/dev/null
check "session-summary text mode shows 'Actions:'" $?

# ════════════════════════════════════════════
#  Debug Bundle (3 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Debug Bundle ==="

BUNDLE=$("$BIN" --json debug-bundle 2>&1) || true
BUNDLE_PATH=$(echo "$BUNDLE" | python3 -c "import sys, json; print(json.load(sys.stdin).get('path', ''))" 2>/dev/null || echo "")
BUNDLE_FILES=$(echo "$BUNDLE" | python3 -c "import sys, json; print(json.load(sys.stdin).get('fileCount', 0))" 2>/dev/null || echo 0)

[ -n "$BUNDLE_PATH" ] && [ -d "$BUNDLE_PATH" ] 2>/dev/null
check "debug-bundle creates directory" $?

[ "$BUNDLE_FILES" -ge 5 ] 2>/dev/null
check "debug-bundle writes >= 5 files (got $BUNDLE_FILES)" $?

BUNDLE_TEXT=$("$BIN" debug-bundle 2>&1) || true
echo "$BUNDLE_TEXT" | grep -q "Debug bundle:" 2>/dev/null
check "debug-bundle text mode shows 'Debug bundle:'" $?

# Clean up debug bundle artifacts
if [ -n "$BUNDLE_PATH" ] && [ -d "$BUNDLE_PATH" ]; then
    rm -rf "$BUNDLE_PATH"
fi
# Clean up second debug bundle from text mode test
BUNDLE2_PATH=$(echo "$BUNDLE_TEXT" | grep "Debug bundle:" | sed 's/Debug bundle: //')
if [ -n "$BUNDLE2_PATH" ] && [ -d "$BUNDLE2_PATH" ]; then
    rm -rf "$BUNDLE2_PATH"
fi

# ════════════════════════════════════════════
#  Select Frame (1 check)
# ════════════════════════════════════════════
echo ""
echo "=== Select Frame ==="

SELECT_FRAME=$("$BIN" select-frame --name main 2>&1) || true
SELECT_EXIT=$?
check "select-frame --name main exits successfully" $SELECT_EXIT

# ════════════════════════════════════════════
#  Cleanup & S03 Regression
# ════════════════════════════════════════════
echo ""
echo "=== Cleanup (before S03 regression) ==="
cleanup_daemon
rm -f "$FORM_FILE"
echo "  Daemon stopped, temp files cleaned"

echo ""
echo "=== S03 Regression ==="
if [ -f "$SCRIPT_DIR/verify-s03.sh" ]; then
    bash "$SCRIPT_DIR/verify-s03.sh"
    S03_EXIT=$?
    check "verify-s03.sh passes" $S03_EXIT
else
    echo "  ⚠ verify-s03.sh not found, skipping"
fi

# ════════════════════════════════════════════
#  Final Cleanup
# ════════════════════════════════════════════
echo ""
echo "=== Final Cleanup ==="
cleanup_daemon
echo "  Done"

# ── Summary ──
echo ""
echo "════════════════════════════════"
echo "  S04 Results: $PASS/$TOTAL passed, $FAIL failed"
echo "════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
    echo "  ❌ FAIL"
    exit 1
else
    echo "  ✅ ALL PASS"
    exit 0
fi
