#!/usr/bin/env bash
set -euo pipefail

# gsd-browser installer
# Usage: curl -fsSL https://raw.githubusercontent.com/gsd-build/gsd-browser/main/install.sh | bash

VERSION="${GSD_BROWSER_VERSION:-latest}"
REPO="gsd-build/gsd-browser"
INSTALL_DIR="${GSD_BROWSER_DIR:-$HOME/.gsd-browser}"
BIN_DIR="$INSTALL_DIR/bin"
CHROMIUM_DIR="$INSTALL_DIR/chromium"

# Colors
cyan="\033[36m"
green="\033[32m"
yellow="\033[33m"
red="\033[31m"
dim="\033[2m"
bold="\033[1m"
reset="\033[0m"

banner() {
  echo ""
  printf "${cyan}${bold}"
  echo "   ██████╗ ███████╗██████╗       ██████╗ ██████╗  ██████╗ ██╗    ██╗███████╗███████╗██████╗ "
  echo "  ██╔════╝ ██╔════╝██╔══██╗      ██╔══██╗██╔══██╗██╔═══██╗██║    ██║██╔════╝██╔════╝██╔══██╗"
  echo "  ██║  ███╗███████╗██║  ██║█████╗██████╔╝██████╔╝██║   ██║██║ █╗ ██║███████╗█████╗  ██████╔╝"
  echo "  ██║   ██║╚════██║██║  ██║╚════╝██╔══██╗██╔══██╗██║   ██║██║███╗██║╚════██║██╔══╝  ██╔══██╗"
  echo "  ╚██████╔╝███████║██████╔╝      ██████╔╝██║  ██║╚██████╔╝╚███╔███╔╝███████║███████╗██║  ██║"
  echo "   ╚═════╝ ╚══════╝╚═════╝       ╚═════╝ ╚═╝  ╚═╝ ╚═════╝  ╚══╝╚══╝ ╚══════╝╚══════╝╚═╝  ╚═╝"
  printf "${reset}\n"
  printf "  ${dim}Browser automation for AI agents${reset}\n\n"
}

info()  { printf "  ${cyan}>${reset} %s\n" "$1"; }
ok()    { printf "  ${green}✓${reset} %s\n" "$1"; }
warn()  { printf "  ${yellow}!${reset} %s\n" "$1"; }
fail()  { printf "  ${red}✗${reset} %s\n" "$1"; exit 1; }

detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os="darwin" ;;
    Linux)  os="linux"  ;;
    *) fail "Unsupported OS: $os (Windows users: use WSL)" ;;
  esac

  case "$arch" in
    x86_64|amd64)  arch="x64"   ;;
    arm64|aarch64) arch="arm64" ;;
    *) fail "Unsupported architecture: $arch" ;;
  esac

  PLATFORM="${os}-${arch}"

  # Chrome for Testing platform names (different from ours)
  case "$PLATFORM" in
    darwin-arm64) CHROME_PLATFORM="mac-arm64" ;;
    darwin-x64)   CHROME_PLATFORM="mac-x64"   ;;
    linux-x64)    CHROME_PLATFORM="linux64"    ;;
    linux-arm64)  CHROME_PLATFORM=""           ;; # Not available
    *) CHROME_PLATFORM="" ;;
  esac
}

resolve_version() {
  if [ "$VERSION" = "latest" ]; then
    info "Fetching latest release..."
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v?([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
      fail "Could not determine latest version. Set GSD_BROWSER_VERSION manually."
    fi
  fi
  ok "Version: $VERSION"
}

download_binary() {
  local url filename
  filename="gsd-browser-${PLATFORM}"
  url="https://github.com/$REPO/releases/download/v${VERSION}/${filename}"

  info "Downloading gsd-browser for $PLATFORM..."
  mkdir -p "$BIN_DIR"

  local target="$BIN_DIR/gsd-browser"

  if ! curl -fsSL -o "$target" "$url"; then
    fail "Download failed: $url"
  fi

  chmod +x "$target"

  # On macOS, strip quarantine/provenance attributes added by curl, then verify
  # the Developer ID signature. Fall back to ad-hoc signing only if the binary
  # has no valid signature (manual builds, stripped signatures).
  if [ "$(uname -s)" = "Darwin" ]; then
    xattr -d com.apple.quarantine "$target" 2>/dev/null || true
    xattr -d com.apple.provenance "$target" 2>/dev/null || true

    if command -v codesign >/dev/null 2>&1; then
      if codesign --verify --strict "$target" 2>/dev/null; then
        ok "Binary signature verified (Developer ID)"
      else
        codesign --sign - --force "$target" 2>/dev/null && {
          ok "Ad-hoc signed for macOS Gatekeeper"
        } || {
          warn "Ad-hoc signing failed — binary may be blocked by Gatekeeper"
          warn "Fix manually: codesign --sign - --force $target"
        }
      fi
    fi
  fi

  ok "Binary installed: $target"
}

detect_existing_chrome() {
  # Check standard Chrome/Chromium locations — mirrors find_chrome() in common/src/chrome.rs
  local candidates=()

  case "$(uname -s)" in
    Darwin)
      candidates=(
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
      )
      ;;
    Linux)
      candidates=(
        "/usr/bin/google-chrome"
        "/usr/bin/google-chrome-stable"
        "/usr/bin/chromium-browser"
        "/usr/bin/chromium"
        "/snap/bin/chromium"
      )
      ;;
  esac

  for path in "${candidates[@]}"; do
    if [ -x "$path" ]; then
      echo "$path"
      return 0
    fi
  done

  # Check PATH
  for name in google-chrome google-chrome-stable chromium-browser chromium; do
    if command -v "$name" >/dev/null 2>&1; then
      command -v "$name"
      return 0
    fi
  done

  return 1
}

download_chromium() {
  if [ -z "$CHROME_PLATFORM" ]; then
    warn "Chromium not available for $PLATFORM via Chrome for Testing"
    warn "Install Chrome/Chromium manually and set GSD_BROWSER_BROWSER_PATH"
    return
  fi

  # Check if a system Chrome/Chromium is already available
  local system_chrome
  if system_chrome=$(detect_existing_chrome); then
    ok "Found Chrome at: $system_chrome (skipping download)"
    SKIP_CHROMIUM_CONFIG=1
    return
  fi

  # Check if we previously downloaded Chrome for Testing
  if [ -f "$CHROMIUM_DIR/version" ]; then
    local existing
    existing=$(cat "$CHROMIUM_DIR/version")
    info "Chromium $existing already installed"
    read -rp "  Reinstall? [y/N] " yn < /dev/tty || yn="n"
    case "$yn" in
      [Yy]*) ;;
      *) ok "Keeping existing Chromium"; return ;;
    esac
  fi

  info "Fetching Chrome for Testing metadata..."
  local json chrome_url chrome_version
  json=$(curl -fsSL "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json")

  chrome_version=$(echo "$json" | grep -o '"Stable"[^}]*"version":"[^"]*"' | head -1 | grep -o '"version":"[^"]*"' | cut -d'"' -f4)
  chrome_url=$(echo "$json" | python3 -c "
import sys, json
data = json.load(sys.stdin)
for entry in data['channels']['Stable']['downloads']['chrome']:
    if entry['platform'] == '$CHROME_PLATFORM':
        print(entry['url'])
        break
" 2>/dev/null || echo "")

  if [ -z "$chrome_url" ]; then
    warn "Could not find Chromium download for $CHROME_PLATFORM"
    return
  fi

  info "Downloading Chromium $chrome_version for $CHROME_PLATFORM..."
  mkdir -p "$CHROMIUM_DIR"

  local tmpzip="$CHROMIUM_DIR/chrome.zip"
  if ! curl -fsSL -o "$tmpzip" "$chrome_url"; then
    warn "Chromium download failed — you can install Chrome manually"
    rm -f "$tmpzip"
    return
  fi

  info "Extracting Chromium..."
  unzip -qo "$tmpzip" -d "$CHROMIUM_DIR"
  rm -f "$tmpzip"

  # Find the chrome binary inside the extracted directory
  local chrome_bin=""
  case "$CHROME_PLATFORM" in
    mac-arm64|mac-x64)
      chrome_bin=$(find "$CHROMIUM_DIR" -name "Google Chrome for Testing" -type f 2>/dev/null | head -1)
      if [ -z "$chrome_bin" ]; then
        chrome_bin=$(find "$CHROMIUM_DIR" -name "Chromium" -type f 2>/dev/null | head -1)
      fi
      ;;
    linux64)
      chrome_bin=$(find "$CHROMIUM_DIR" -name "chrome" -type f 2>/dev/null | head -1)
      ;;
  esac

  if [ -n "$chrome_bin" ]; then
    echo "$chrome_version" > "$CHROMIUM_DIR/version"
    echo "$chrome_bin" > "$CHROMIUM_DIR/binary_path"
    ok "Chromium $chrome_version installed"
  else
    warn "Chromium extracted but binary not found — check $CHROMIUM_DIR"
  fi
}

setup_path() {
  local target="/usr/local/bin/gsd-browser"
  local source="$BIN_DIR/gsd-browser"

  # Try symlink to /usr/local/bin
  if [ -w "/usr/local/bin" ] || [ -w "$(dirname /usr/local/bin)" ]; then
    ln -sf "$source" "$target" 2>/dev/null && {
      ok "Linked to $target"
      return
    }
  fi

  # Try with sudo
  if command -v sudo >/dev/null 2>&1; then
    info "Linking to /usr/local/bin (may need password)..."
    sudo ln -sf "$source" "$target" 2>/dev/null && {
      ok "Linked to $target"
      return
    }
  fi

  # Fallback: tell user to add to PATH
  warn "Could not link to /usr/local/bin"
  echo ""
  printf "  Add this to your shell profile:\n"
  printf "  ${bold}export PATH=\"$BIN_DIR:\$PATH\"${reset}\n"
  echo ""
}

write_config() {
  # If system Chrome was detected, no config needed — find_chrome() will discover it
  if [ "${SKIP_CHROMIUM_CONFIG:-}" = "1" ]; then
    return
  fi

  # If we downloaded Chromium, write a config pointing to it
  if [ -f "$CHROMIUM_DIR/binary_path" ]; then
    local chrome_path
    chrome_path=$(cat "$CHROMIUM_DIR/binary_path")
    local config_dir="$HOME/.gsd-browser"
    mkdir -p "$config_dir"
    cat > "$config_dir/config.toml" << TOML
[browser]
path = "$chrome_path"
TOML
    ok "Config written: $config_dir/config.toml"
  fi
}

verify() {
  info "Verifying installation..."

  if ! command -v gsd-browser >/dev/null 2>&1; then
    # Try the direct path
    if [ -x "$BIN_DIR/gsd-browser" ]; then
      local result
      result=$("$BIN_DIR/gsd-browser" daemon health 2>&1) && {
        ok "Daemon healthy (using direct path)"
        "$BIN_DIR/gsd-browser" daemon stop >/dev/null 2>&1 || true
        return
      }
    fi
    warn "gsd-browser not in PATH yet — restart your shell or run: export PATH=\"$BIN_DIR:\$PATH\""
    return
  fi

  local result
  result=$(gsd-browser daemon health 2>&1) && {
    ok "Daemon healthy"
    gsd-browser daemon stop >/dev/null 2>&1 || true
  } || {
    warn "Daemon test failed — this is normal if Chrome is not in the default path"
    warn "Set browser.path in ~/.gsd-browser/config.toml"
  }
}

SKILL_REPO_BASE="https://raw.githubusercontent.com/$REPO/main/gsd-browser-skill"
SKILL_FILES=(
  "SKILL.md"
  "references/command-reference.md"
  "references/snapshot-and-refs.md"
  "references/semantic-intents.md"
  "references/configuration.md"
  "references/error-recovery.md"
  "workflows/scrape-and-extract.md"
  "workflows/setup-and-configure.md"
  "workflows/navigate-and-interact.md"
  "workflows/test-and-assert.md"
  "workflows/debug-and-diagnose.md"
)

install_skill_to() {
  local dest="$1"
  mkdir -p "$dest" "$dest/references" "$dest/workflows"

  local failed=0
  for file in "${SKILL_FILES[@]}"; do
    if ! curl -fsSL -o "$dest/$file" "$SKILL_REPO_BASE/$file" 2>/dev/null; then
      failed=1
      break
    fi
  done

  if [ "$failed" -eq 1 ]; then
    warn "Failed to download skill files — check network or repo access"
    rm -rf "$dest"
    return 1
  fi

  ok "Skill installed: $dest"
  return 0
}

install_skill() {
  echo ""
  printf "  ${cyan}${bold}AI Agent Skill Installation${reset}\n"
  printf "  ${dim}Install the gsd-browser skill so your AI coding agent knows how to use it${reset}\n"
  echo ""

  # Detect available AI CLIs
  local available=()
  local labels=()

  if command -v claude >/dev/null 2>&1; then
    available+=("claude")
    labels+=("Claude Code")
  fi
  if command -v codex >/dev/null 2>&1; then
    available+=("codex")
    labels+=("OpenAI Codex CLI")
  fi
  if command -v gemini >/dev/null 2>&1; then
    available+=("gemini")
    labels+=("Google Gemini CLI")
  fi

  if [ ${#available[@]} -eq 0 ]; then
    info "No AI coding agents detected (claude, codex, gemini)"
    info "Install one, then re-run: gsd-browser skill install"
    return
  fi

  printf "  Detected AI agents:\n"
  local idx=1
  for label in "${labels[@]}"; do
    printf "    ${bold}%d)${reset} %s\n" "$idx" "$label"
    idx=$((idx + 1))
  done
  printf "    ${bold}a)${reset} All detected agents\n"
  printf "    ${bold}s)${reset} Skip\n"
  echo ""

  local choice
  read -rp "  Install skill for which agent(s)? [a]: " choice < /dev/tty || choice="a"
  choice="${choice:-a}"

  if [ "$choice" = "s" ] || [ "$choice" = "S" ]; then
    info "Skipping skill installation"
    return
  fi

  # Build list of selected tools
  local selected=()
  if [ "$choice" = "a" ] || [ "$choice" = "A" ]; then
    selected=("${available[@]}")
  else
    # Parse comma-separated or single number
    IFS=',' read -ra nums <<< "$choice"
    for num in "${nums[@]}"; do
      num=$(echo "$num" | tr -d ' ')
      if [[ "$num" =~ ^[0-9]+$ ]] && [ "$num" -ge 1 ] && [ "$num" -le ${#available[@]} ]; then
        selected+=("${available[$((num - 1))]}")
      fi
    done
  fi

  if [ ${#selected[@]} -eq 0 ]; then
    warn "No valid selection — skipping skill installation"
    return
  fi

  # Ask scope
  echo ""
  printf "  Install scope:\n"
  printf "    ${bold}g)${reset} Global (available in all projects)\n"
  printf "    ${bold}l)${reset} Local  (current directory only)\n"
  echo ""
  local scope
  read -rp "  Scope? [g]: " scope < /dev/tty || scope="g"
  scope="${scope:-g}"

  for tool in "${selected[@]}"; do
    local dest=""
    case "$tool" in
      claude)
        if [ "$scope" = "l" ] || [ "$scope" = "L" ]; then
          dest=".claude/skills/gsd-browser"
        else
          dest="$HOME/.claude/skills/gsd-browser"
        fi
        ;;
      codex)
        if [ "$scope" = "l" ] || [ "$scope" = "L" ]; then
          dest=".codex/skills/gsd-browser"
        else
          dest="$HOME/.codex/skills/gsd-browser"
        fi
        ;;
      gemini)
        if [ "$scope" = "l" ] || [ "$scope" = "L" ]; then
          dest=".gemini/skills/gsd-browser"
        else
          dest="$HOME/.gemini/skills/gsd-browser"
        fi
        ;;
    esac

    if [ -n "$dest" ]; then
      info "Installing skill for $tool → $dest"
      install_skill_to "$dest"
    fi
  done
}

main() {
  banner
  detect_platform
  ok "Platform: $PLATFORM"
  resolve_version
  download_binary
  download_chromium
  setup_path
  write_config
  verify
  install_skill

  echo ""
  printf "  ${green}${bold}Installation complete!${reset}\n"
  echo ""
  printf "  ${dim}Quick start:${reset}\n"
  printf "    gsd-browser navigate https://example.com\n"
  printf "    gsd-browser screenshot --output page.png\n"
  printf "    gsd-browser snapshot\n"
  echo ""
}

main
