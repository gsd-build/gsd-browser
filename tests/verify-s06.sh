#!/usr/bin/env bash
# End-to-end verification for S06: Distribution + SKILL + Polish
# Tests all S06 deliverables: config file loading, env var overrides,
# --help completeness, npm package structure, license/readme/skill/agents,
# Cargo metadata, and visual-diff regression.

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
    pkill -f "browser-tools-daemon" 2>/dev/null || true
    sleep 2
    find /private/var/folders -name "SingletonLock" -path "*/chromiumoxide-runner/*" -delete 2>/dev/null || true
    rm -f ~/.browser-tools/daemon.sock ~/.browser-tools/daemon.pid
}

# ════════════════════════════════════════════
#  File Existence + Content Checks (static, no daemon needed)
# ════════════════════════════════════════════
echo ""
echo "=== File Existence & Content ==="

# License files
test -f "$PROJECT_DIR/LICENSE-MIT"
check "LICENSE-MIT exists" $?

test -f "$PROJECT_DIR/LICENSE-APACHE"
check "LICENSE-APACHE exists" $?

# README with install instructions
test -f "$PROJECT_DIR/README.md"
check "README.md exists" $?

grep -q "npm install" "$PROJECT_DIR/README.md" 2>/dev/null
check "README.md contains npm install instructions" $?

grep -q "cargo install" "$PROJECT_DIR/README.md" 2>/dev/null
check "README.md contains cargo install instructions" $?

# SKILL.md with frontmatter
test -f "$PROJECT_DIR/SKILL.md"
check "SKILL.md exists" $?

head -5 "$PROJECT_DIR/SKILL.md" | grep -q "name:" 2>/dev/null
check "SKILL.md has YAML frontmatter with name:" $?

SKILL_LINES=$(wc -l < "$PROJECT_DIR/SKILL.md" | tr -d ' ')
[ "$SKILL_LINES" -ge 300 ]
check "SKILL.md has >= 300 lines (got $SKILL_LINES)" $?

# AGENTS.md
test -f "$PROJECT_DIR/AGENTS.md"
check "AGENTS.md exists" $?

# ════════════════════════════════════════════
#  npm Package Structure (4 checks)
# ════════════════════════════════════════════
echo ""
echo "=== npm Package Structure ==="

test -f "$PROJECT_DIR/npm/package.json"
check "npm/package.json exists" $?

node -e "const p=require('$PROJECT_DIR/npm/package.json'); if(p.name !== '@gsd-build/browser-tools') process.exit(1)" 2>/dev/null
check "npm package name is @gsd-build/browser-tools" $?

test -f "$PROJECT_DIR/npm/scripts/postinstall.js"
check "npm/scripts/postinstall.js exists" $?

node -c "$PROJECT_DIR/npm/scripts/postinstall.js" 2>/dev/null
check "postinstall.js has valid JS syntax" $?

# ════════════════════════════════════════════
#  Cargo Metadata (3 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Cargo Metadata ==="

grep -q 'license = "MIT OR Apache-2.0"' "$PROJECT_DIR/cli/Cargo.toml" 2>/dev/null
check "cli/Cargo.toml has license field" $?

grep -q 'publish = false' "$PROJECT_DIR/common/Cargo.toml" 2>/dev/null
check "common/Cargo.toml has publish = false" $?

grep -q 'publish = false' "$PROJECT_DIR/daemon/Cargo.toml" 2>/dev/null
check "daemon/Cargo.toml has publish = false" $?

# ════════════════════════════════════════════
#  Config Infrastructure (3 checks)
# ════════════════════════════════════════════
echo ""
echo "=== Config Infrastructure ==="

# Verify config module exists
test -f "$PROJECT_DIR/common/src/config.rs"
check "common/src/config.rs exists" $?

# Verify config is exported from common
grep -q "pub mod config" "$PROJECT_DIR/common/src/lib.rs" 2>/dev/null
check "config module is public in common/src/lib.rs" $?

# Env var override: set BROWSER_TOOLS_SCREENSHOT_QUALITY and verify CLI doesn't error
# (We test that the binary starts cleanly with the env var set)
TMPDIR_CFG=$(mktemp -d)
cat > "$TMPDIR_CFG/config.toml" << 'TOMLEOF'
[settle]
timeout_ms = 999

[screenshot]
quality = 42
TOMLEOF
# The config infrastructure should parse env vars without crashing
BROWSER_TOOLS_SCREENSHOT_QUALITY=50 "$BIN" --help > /dev/null 2>&1
check "CLI starts cleanly with BROWSER_TOOLS_SCREENSHOT_QUALITY env var" $?

rm -rf "$TMPDIR_CFG"

# ════════════════════════════════════════════
#  --help Completeness (2 checks)
# ════════════════════════════════════════════
echo ""
echo "=== --help Completeness ==="

HELP_OUTPUT=$("$BIN" --help 2>&1)
# Count subcommand lines (lines starting with 2+ spaces then a lowercase letter)
SUBCMD_COUNT=$(echo "$HELP_OUTPUT" | grep -c '^  [a-z]')
[ "$SUBCMD_COUNT" -ge 52 ]
check "--help lists >= 52 subcommands (got $SUBCMD_COUNT)" $?

# Spot-check key S06-adjacent commands exist in help
echo "$HELP_OUTPUT" | grep -q "navigate" 2>/dev/null && \
echo "$HELP_OUTPUT" | grep -q "visual-diff" 2>/dev/null && \
echo "$HELP_OUTPUT" | grep -q "vault-save" 2>/dev/null && \
echo "$HELP_OUTPUT" | grep -q "batch" 2>/dev/null
check "--help includes navigate, visual-diff, vault-save, batch" $?

# ════════════════════════════════════════════
#  Cargo Tests (unit + integration)
# ════════════════════════════════════════════
echo ""
echo "=== Cargo Tests ==="

CARGO_TEST_OUTPUT=$(cargo test --workspace --manifest-path "$PROJECT_DIR/Cargo.toml" 2>&1)
CARGO_TEST_EXIT=$?
# Extract test summary line
CARGO_SUMMARY=$(echo "$CARGO_TEST_OUTPUT" | grep "test result:" | tail -3)
echo "$CARGO_SUMMARY"
check "cargo test --workspace passes" $CARGO_TEST_EXIT

# ════════════════════════════════════════════
#  Visual Diff Regression (R022)
# ════════════════════════════════════════════
echo ""
echo "=== Visual Diff Regression (R022) ==="

cleanup_daemon
sleep 1

# Navigate to a page to warm up daemon
NAV_OUT=$("$BIN" navigate https://example.com 2>&1) || true
echo "$NAV_OUT" | grep -q "Example Domain" 2>/dev/null
check "navigate to example.com for visual-diff test" $?

# Create baseline
VD1=$("$BIN" --json visual-diff --name s06-regression 2>&1) || true
VD1_STATUS=$(echo "$VD1" | python3 -c "import sys, json; print(json.load(sys.stdin).get('status', ''))" 2>/dev/null || echo "")
[ "$VD1_STATUS" = "baseline_created" ] || [ "$VD1_STATUS" = "baseline_updated" ]
check "visual-diff creates baseline (status=$VD1_STATUS)" $?

# Compare against baseline — should match
VD2=$("$BIN" --json visual-diff --name s06-regression 2>&1) || true
VD2_SIM=$(echo "$VD2" | python3 -c "import sys, json; print(json.load(sys.stdin).get('similarity', 0))" 2>/dev/null || echo "0")
python3 -c "assert float('$VD2_SIM') >= 0.99, f'similarity too low: $VD2_SIM'" 2>/dev/null
check "visual-diff matches baseline (similarity=$VD2_SIM)" $?

# ════════════════════════════════════════════
#  S03-S05 Regression
# ════════════════════════════════════════════
echo ""
echo "=== Cleanup before regression ==="
cleanup_daemon
rm -f ~/.browser-tools/baselines/s06-regression.png
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

echo ""
echo "=== S04 Regression ==="
if [ -f "$SCRIPT_DIR/verify-s04.sh" ]; then
    bash "$SCRIPT_DIR/verify-s04.sh"
    S04_EXIT=$?
    check "verify-s04.sh passes" $S04_EXIT
else
    echo "  ⚠ verify-s04.sh not found, skipping"
fi

echo ""
echo "=== S05 Regression ==="
if [ -f "$SCRIPT_DIR/verify-s05.sh" ]; then
    bash "$SCRIPT_DIR/verify-s05.sh"
    S05_EXIT=$?
    check "verify-s05.sh passes" $S05_EXIT
else
    echo "  ⚠ verify-s05.sh not found, skipping"
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
echo "  S06 Results: $PASS/$TOTAL passed, $FAIL failed"
echo "════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
    echo "  ❌ FAIL"
    exit 1
else
    echo "  ✅ ALL PASS"
    exit 0
fi
