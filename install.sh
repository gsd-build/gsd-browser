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
  ok "Binary installed: $target"
}

download_chromium() {
  if [ -z "$CHROME_PLATFORM" ]; then
    warn "Chromium not available for $PLATFORM via Chrome for Testing"
    warn "Install Chrome/Chromium manually and set GSD_BROWSER_BROWSER__PATH"
    return
  fi

  # Check if already installed
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
