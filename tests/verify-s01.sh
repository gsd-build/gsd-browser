#!/usr/bin/env bash
# End-to-end verification for S01: CLI + Daemon + CDP Foundation
# Tests navigation commands, output formats, error handling, daemon lifecycle.

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
    # Kill daemon by PID file
    if [ -f ~/.browser-tools/daemon.pid ]; then
        local pid
        pid=$(cat ~/.browser-tools/daemon.pid 2>/dev/null)
        if [ -n "$pid" ]; then
            kill "$pid" 2>/dev/null || true
        fi
    fi
    # Kill any stale Chrome instances from chromiumoxide
    pkill -f "chromiumoxide-runner" 2>/dev/null || true
    sleep 2
    # Remove singleton lock (macOS-specific path varies)
    find /private/var/folders -name "SingletonLock" -path "*/chromiumoxide-runner/*" -delete 2>/dev/null || true
    rm -f ~/.browser-tools/daemon.sock ~/.browser-tools/daemon.pid
}

# ── Cleanup any existing daemon ──
echo ""
echo "=== Cleanup ==="
cleanup_daemon
echo "  Cleaned up stale daemon files"

# ── Test 1: Navigate (cold start — daemon auto-starts) ──
echo ""
echo "=== Navigation Tests ==="

COLD_START=$(python3 -c 'import time; print(int(time.time()*1000))')
NAV_OUTPUT=$("$BIN" navigate https://example.com 2>&1) || true
NAV_EXIT=${PIPESTATUS[0]:-$?}
COLD_END=$(python3 -c 'import time; print(int(time.time()*1000))')
COLD_MS=$((COLD_END - COLD_START))

# Check if navigate succeeded
if echo "$NAV_OUTPUT" | grep -q "Example Domain"; then
    check "navigate exits with expected output" 0
else
    check "navigate exits with expected output (got: $(echo "$NAV_OUTPUT" | head -1))" 1
fi

echo "$NAV_OUTPUT" | grep -q "https://example.com" 2>/dev/null
check "navigate output contains URL" $?

echo "$NAV_OUTPUT" | grep -q "Elements:" 2>/dev/null
check "navigate output contains Elements line" $?

echo "$NAV_OUTPUT" | grep -q "Headings:" 2>/dev/null
check "navigate output contains Headings line" $?

echo "  ℹ️  Cold start (daemon auto-start + navigate): ${COLD_MS}ms"

# ── Test 2: Navigate JSON output ──
echo ""
echo "=== JSON Output Tests ==="

NAV_JSON=$("$BIN" --json navigate https://example.com 2>&1) || true

echo "$NAV_JSON" | python3 -c "import sys, json; json.load(sys.stdin)" 2>/dev/null
check "navigate --json produces valid JSON" $?

TITLE=$(echo "$NAV_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin).get('title',''))" 2>/dev/null) || true
if [ "$TITLE" = "Example Domain" ]; then
    check "navigate --json .title = 'Example Domain'" 0
else
    check "navigate --json .title = 'Example Domain' (got: '$TITLE')" 1
fi

# ── Test 3: Warm command timing ──
WARM_START=$(python3 -c 'import time; print(int(time.time()*1000))')
"$BIN" navigate https://example.com > /dev/null 2>&1 || true
WARM_END=$(python3 -c 'import time; print(int(time.time()*1000))')
WARM_MS=$((WARM_END - WARM_START))
echo "  ℹ️  Warm command (navigate): ${WARM_MS}ms"

# ── Test 4: Back ──
echo ""
echo "=== Back/Forward/Reload Tests ==="

BACK_OUTPUT=$("$BIN" back 2>&1) || true

echo "$BACK_OUTPUT" | grep -q "Navigated back to:" 2>/dev/null
check "back output contains 'Navigated back to:'" $?

# ── Test 5: Forward ──
FWD_OUTPUT=$("$BIN" forward 2>&1) || true

echo "$FWD_OUTPUT" | grep -q "Example Domain" 2>/dev/null
check "forward output contains 'Example Domain'" $?

# ── Test 6: Forward again (should error — no more forward) ──
FWD2_OUTPUT=$("$BIN" forward 2>&1)
FWD2_EXIT=$?

if [ "$FWD2_EXIT" -ne 0 ]; then
    check "forward with no forward page exits non-zero" 0
else
    check "forward with no forward page exits non-zero (got exit $FWD2_EXIT)" 1
fi

echo "$FWD2_OUTPUT" | grep -qi "no forward" 2>/dev/null
check "forward error mentions 'no forward'" $?

# ── Test 7: Reload ──
RELOAD_OUTPUT=$("$BIN" reload 2>&1) || true

echo "$RELOAD_OUTPUT" | grep -q "Example Domain" 2>/dev/null
check "reload output contains 'Example Domain'" $?

echo "$RELOAD_OUTPUT" | grep -q "Reloaded:" 2>/dev/null
check "reload output starts with 'Reloaded:'" $?

# ── Test 8: Reload JSON ──
RELOAD_JSON=$("$BIN" --json reload 2>&1) || true

echo "$RELOAD_JSON" | python3 -c "import sys, json; json.load(sys.stdin)" 2>/dev/null
check "reload --json produces valid JSON" $?

# ── Test 9: Daemon health ──
echo ""
echo "=== Daemon Lifecycle Tests ==="

HEALTH_OUTPUT=$("$BIN" daemon health 2>&1) || true

echo "$HEALTH_OUTPUT" | grep -q "ok" 2>/dev/null
check "daemon health shows 'ok'" $?

# ── Test 10: PID and socket files exist ──
if [ -f ~/.browser-tools/daemon.pid ]; then
    check "daemon.pid exists" 0
else
    check "daemon.pid exists" 1
fi

if [ -S ~/.browser-tools/daemon.sock ]; then
    check "daemon.sock exists" 0
else
    check "daemon.sock exists" 1
fi

# ── Test 11: Navigate with no URL (should error from clap) ──
echo ""
echo "=== Negative Tests ==="

"$BIN" navigate 2>/dev/null
NAV_NO_URL_EXIT=$?
if [ "$NAV_NO_URL_EXIT" -ne 0 ]; then
    check "navigate with no URL exits non-zero" 0
else
    check "navigate with no URL exits non-zero" 1
fi

# ── Test 12: Forward JSON error format ──
FWD_ERR_JSON=$("$BIN" --json forward 2>&1) || true
echo "$FWD_ERR_JSON" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'error' in d" 2>/dev/null
check "forward --json error produces valid JSON with error field" $?

# ── Cleanup ──
echo ""
echo "=== Cleanup ==="
cleanup_daemon
echo "  Daemon stopped and cleaned up"

# ── Summary ──
echo ""
echo "════════════════════════════════"
echo "  Results: $PASS/$TOTAL passed, $FAIL failed"
echo "  Cold start: ${COLD_MS}ms | Warm: ${WARM_MS}ms"
echo "════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
    echo "  ❌ FAIL"
    exit 1
else
    echo "  ✅ ALL PASS"
    exit 0
fi
