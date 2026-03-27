#!/usr/bin/env bash
# End-to-end verification for S03: Refs + Assertions + Batch + Wait + Timeline
# Tests wait conditions, snapshot/refs, assertions, diff, timeline, batch,
# error cases, and S02 regression against live pages.

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

# ════════════════════════════════════════════
#  S02 Regression Spot-Checks (4 checks)
# ════════════════════════════════════════════
echo ""
echo "=== S02 Regression Spot-Checks ==="

# Navigate to baseline page (cold start — daemon auto-starts)
NAV_OUTPUT=$("$BIN" navigate https://example.com 2>&1) || true
echo "$NAV_OUTPUT" | grep -q "Example Domain" 2>/dev/null
check "navigate to example.com (baseline)" $?

# eval works
EVAL_TITLE=$("$BIN" eval "document.title" 2>&1) || true
echo "$EVAL_TITLE" | grep -q "Example Domain" 2>/dev/null
check "eval document.title returns 'Example Domain'" $?

# console --json returns entries array
CONSOLE_JSON=$("$BIN" --json console 2>&1) || true
echo "$CONSOLE_JSON" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'entries' in d" 2>/dev/null
check "console --json returns entries array" $?

# click exits successfully
"$BIN" navigate https://example.com > /dev/null 2>&1 || true
CLICK_OUTPUT=$("$BIN" click a 2>&1) || true
echo "$CLICK_OUTPUT" | grep -qi "clicked\|page summary\|title" 2>/dev/null
check "click 'a' returns successfully" $?

# ════════════════════════════════════════════
#  Wait Conditions (4 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Wait Conditions ==="

"$BIN" navigate https://example.com > /dev/null 2>&1 || true

# wait-for text_visible (text mode)
WAIT_TEXT=$("$BIN" wait-for --condition text_visible --value "Example Domain" 2>&1) || true
echo "$WAIT_TEXT" | grep -q "met" 2>/dev/null
check "wait-for text_visible 'Example Domain' — met (text)" $?

# wait-for text_visible (JSON mode)
WAIT_JSON=$("$BIN" --json wait-for --condition text_visible --value "Example Domain" 2>&1) || true
echo "$WAIT_JSON" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d['met'] == True" 2>/dev/null
check "wait-for text_visible --json returns met=true" $?

# wait-for delay
WAIT_DELAY=$("$BIN" wait-for --condition delay --value 200 2>&1) || true
echo "$WAIT_DELAY" | grep -q "met" 2>/dev/null
check "wait-for delay 200ms — met" $?

# wait-for selector_visible
WAIT_SEL=$("$BIN" --json wait-for --condition selector_visible --value "h1" 2>&1) || true
echo "$WAIT_SEL" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d['met'] == True" 2>/dev/null
check "wait-for selector_visible 'h1' --json met=true" $?

# ════════════════════════════════════════════
#  Snapshot + Refs (4 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Snapshot + Refs ==="

"$BIN" navigate https://example.com > /dev/null 2>&1 || true
sleep 1

# snapshot — verify version and count
SNAP=$("$BIN" --json snapshot 2>&1) || true
echo "$SNAP" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'version' in d and d['version'] >= 1; assert 'count' in d and d['count'] >= 1" 2>/dev/null
check "snapshot returns version >= 1 and count >= 1" $?

# get version and first ref key from snapshot
VER=$(echo "$SNAP" | python3 -c "import sys, json; print(json.load(sys.stdin).get('version', 1))" 2>/dev/null || echo 1)
REF_KEY=$(echo "$SNAP" | python3 -c "import sys, json; d = json.load(sys.stdin); refs = d.get('refs', {}); print(sorted(refs.keys(), key=lambda k: int(k[1:]))[0] if refs else 'e1')" 2>/dev/null || echo e1)

# get-ref — verify metadata
GET_REF=$("$BIN" --json get-ref "@v${VER}:${REF_KEY}" 2>&1) || true
echo "$GET_REF" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'tag' in d" 2>/dev/null
check "get-ref @v${VER}:${REF_KEY} returns tag metadata" $?

# snapshot text mode shows version
SNAP_TEXT=$("$BIN" snapshot 2>&1) || true
echo "$SNAP_TEXT" | grep -q "Snapshot v" 2>/dev/null
check "snapshot text mode shows 'Snapshot v'" $?

# click-ref on a link (should navigate or succeed)
"$BIN" navigate https://example.com > /dev/null 2>&1 || true
sleep 1
# Take fresh snapshot for click-ref
SNAP2=$("$BIN" --json snapshot 2>&1) || true
VER2=$(echo "$SNAP2" | python3 -c "import sys, json; print(json.load(sys.stdin).get('version', 1))" 2>/dev/null || echo 1)
REF_KEY2=$(echo "$SNAP2" | python3 -c "import sys, json; d = json.load(sys.stdin); refs = d.get('refs', {}); print(sorted(refs.keys(), key=lambda k: int(k[1:]))[0] if refs else 'e1')" 2>/dev/null || echo e1)
CLICK_REF=$("$BIN" click-ref "@v${VER2}:${REF_KEY2}" 2>&1)
CLICK_REF_EXIT=$?
check "click-ref @v${VER2}:${REF_KEY2} exits successfully" $CLICK_REF_EXIT

# ════════════════════════════════════════════
#  Assertions (5 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Assertions ==="

"$BIN" navigate https://example.com > /dev/null 2>&1 || true
sleep 1

# assert url_contains — pass case
ASSERT_URL=$("$BIN" --json assert --checks '[{"kind":"url_contains","text":"example.com"}]' 2>&1) || true
echo "$ASSERT_URL" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d['verified'] == True" 2>/dev/null
check "assert url_contains 'example.com' verified=true" $?

# assert text_visible — pass case
ASSERT_TEXT=$("$BIN" --json assert --checks '[{"kind":"text_visible","text":"Example Domain"}]' 2>&1) || true
echo "$ASSERT_TEXT" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d['verified'] == True" 2>/dev/null
check "assert text_visible 'Example Domain' verified=true" $?

# assert selector_visible — pass case
ASSERT_SEL=$("$BIN" --json assert --checks '[{"kind":"selector_visible","selector":"h1"}]' 2>&1) || true
echo "$ASSERT_SEL" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d['verified'] == True" 2>/dev/null
check "assert selector_visible 'h1' verified=true" $?

# assert text_visible — fail case (bogus text)
ASSERT_FAIL=$("$BIN" --json assert --checks '[{"kind":"text_visible","text":"ZZZNONEXISTENTZZZZ"}]' 2>&1) || true
echo "$ASSERT_FAIL" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d['verified'] == False" 2>/dev/null
check "assert text_visible bogus text verified=false" $?

# assert text mode — check output contains VERIFIED or FAILED
ASSERT_TEXT_MODE=$("$BIN" assert --checks '[{"kind":"url_contains","text":"example.com"}]' 2>&1) || true
echo "$ASSERT_TEXT_MODE" | grep -q "VERIFIED" 2>/dev/null
check "assert text mode shows VERIFIED" $?

# ════════════════════════════════════════════
#  Diff (3 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Diff ==="

# First navigation sets baseline state
"$BIN" navigate https://example.com > /dev/null 2>&1 || true
sleep 1

# Diff right after same page — should show unchanged (or first-time state)
DIFF1=$("$BIN" --json diff 2>&1) || true
echo "$DIFF1" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'changed' in d" 2>/dev/null
check "diff --json returns 'changed' field" $?

# Navigate to different page, then diff should show changed
"$BIN" navigate https://www.iana.org > /dev/null 2>&1 || true
sleep 1
DIFF2=$("$BIN" --json diff 2>&1) || true
echo "$DIFF2" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d['changed'] == True" 2>/dev/null
check "diff after navigation shows changed=true" $?

# Diff text mode — should contain CHANGED or UNCHANGED
DIFF_TEXT=$("$BIN" diff 2>&1) || true
echo "$DIFF_TEXT" | grep -q "CHANGED\|UNCHANGED" 2>/dev/null
check "diff text mode shows CHANGED or UNCHANGED" $?

# ════════════════════════════════════════════
#  Timeline (2 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Timeline ==="

# timeline --json should have entries from previous actions
TIMELINE=$("$BIN" --json timeline 2>&1) || true
echo "$TIMELINE" | python3 -c "import sys, json; d = json.load(sys.stdin); assert 'entries' in d and len(d['entries']) > 0" 2>/dev/null
check "timeline --json returns entries with > 0 entries" $?

# timeline text mode should show recent actions
TIMELINE_TEXT=$("$BIN" timeline 2>&1) || true
echo "$TIMELINE_TEXT" | grep -q "navigate" 2>/dev/null
check "timeline text mode contains 'navigate'" $?

# ════════════════════════════════════════════
#  Batch (3 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Batch ==="

# Batch: navigate + wait_for + assert
BATCH_STEPS='[{"action":"navigate","url":"https://example.com"},{"action":"wait_for","condition":"text_visible","value":"Example Domain"},{"action":"assert","checks":[{"kind":"url_contains","text":"example.com"}]}]'
BATCH_RESULT=$("$BIN" --json batch --steps "$BATCH_STEPS" 2>&1) || true
echo "$BATCH_RESULT" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d['totalSteps'] == 3 and d['passedSteps'] == 3" 2>/dev/null
check "batch 3-step pipeline totalSteps=3, passedSteps=3" $?

# Batch text mode
BATCH_TEXT=$("$BIN" batch --steps "$BATCH_STEPS" 2>&1) || true
echo "$BATCH_TEXT" | grep -q "3/3 steps passed" 2>/dev/null
check "batch text mode shows '3/3 steps passed'" $?

# Batch with failing step
BATCH_FAIL_STEPS='[{"action":"navigate","url":"https://example.com"},{"action":"assert","checks":[{"kind":"text_visible","text":"NONEXISTENTZZZZ"}]}]'
BATCH_FAIL=$("$BIN" --json batch --steps "$BATCH_FAIL_STEPS" 2>&1) || true
echo "$BATCH_FAIL" | python3 -c "import sys, json; d = json.load(sys.stdin); assert d['passedSteps'] == 1 and 'failedStep' in d" 2>/dev/null
check "batch with failing assert stops and reports failedStep" $?

# ════════════════════════════════════════════
#  Error Cases (3 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Error Cases ==="

# Invalid ref format
"$BIN" get-ref "badformat" > /dev/null 2>&1
INVALID_REF_EXIT=$?
if [ "$INVALID_REF_EXIT" -ne 0 ]; then
    check "get-ref invalid format exits non-zero" 0
else
    check "get-ref invalid format exits non-zero" 1
fi

# Nonexistent wait condition
"$BIN" wait-for --condition "fake_condition" --value "x" > /dev/null 2>&1
INVALID_COND_EXIT=$?
if [ "$INVALID_COND_EXIT" -ne 0 ]; then
    check "wait-for unknown condition exits non-zero" 0
else
    check "wait-for unknown condition exits non-zero" 1
fi

# Malformed checks JSON
"$BIN" assert --checks "not-valid-json" > /dev/null 2>&1
MALFORMED_EXIT=$?
if [ "$MALFORMED_EXIT" -ne 0 ]; then
    check "assert malformed checks JSON exits non-zero" 0
else
    check "assert malformed checks JSON exits non-zero" 1
fi

# ════════════════════════════════════════════
#  Cleanup
# ════════════════════════════════════════════
echo ""
echo "=== Cleanup ==="
cleanup_daemon
echo "  Daemon stopped"

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
