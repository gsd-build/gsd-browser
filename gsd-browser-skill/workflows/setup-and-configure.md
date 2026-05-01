<required_reading>
**Read these reference files NOW:**
1. references/configuration.md
2. references/command-reference.md (Daemon Management section)
</required_reading>

<process>

**Step 1: Install gsd-browser**

```bash
# One-liner (macOS / Linux)
curl -fsSL https://raw.githubusercontent.com/gsd-build/gsd-browser/main/install.sh | bash

# Or from a repo checkout
git clone https://github.com/gsd-build/gsd-browser.git
cd gsd-browser
cargo install --path cli

# Verify
gsd-browser daemon start
gsd-browser daemon health
gsd-browser daemon stop
```

The installer downloads the binary and reuses a system Chrome/Chromium when present. Otherwise it downloads Chromium automatically when Chrome for Testing is available for the platform.

**Step 2: Configure browser path (if needed)**

If Chrome/Chromium is not in the default location:

```bash
# Via config file
mkdir -p ~/.gsd-browser
cat > ~/.gsd-browser/config.toml << 'TOML'
[browser]
path = "/path/to/chrome"
TOML

# Or via environment variable
export GSD_BROWSER_BROWSER_PATH="/path/to/chrome"

# Or via CLI flag (per-command)
gsd-browser --browser-path "/path/to/chrome" navigate https://example.com
```

**Step 3: Project-level configuration**

Create `gsd-browser.toml` in your project root:

```toml
[browser]
path = "/usr/bin/chromium"
headless = true

[daemon]
port = 9222
host = "127.0.0.1"

[screenshot]
quality = 90
format = "png"
full_page = false

[settle]
timeout_ms = 500
poll_ms = 40
quiet_window_ms = 100

[logs]
max_buffer_size = 1000

[artifacts]
dir = "./browser-artifacts"

[timeline]
max_entries = 500
```

**Step 4: Set up the encrypted auth vault**

The vault key must be set **before the daemon starts**:

```bash
export GSD_BROWSER_VAULT_KEY="your-encryption-key"
gsd-browser daemon stop    # Stop existing daemon if running
gsd-browser vault-save --profile github \
  --url https://github.com/login \
  --username user --password "secret"
```

**Step 5: Parallel sessions**

Run multiple independent browser instances:

```bash
gsd-browser --session site1 navigate https://site-a.com
gsd-browser --session site2 navigate https://site-b.com

# Each session has its own daemon, socket, and Chrome instance
gsd-browser --session site1 snapshot
gsd-browser --session site2 snapshot

# Clean up both
gsd-browser --session site1 daemon stop
gsd-browser --session site2 daemon stop
```

**Step 6: Daemon management**

The daemon auto-starts on browser commands. `daemon health` reports state and does not start a session. Manual management is rarely needed:

```bash
gsd-browser daemon health     # Check status of the current session
gsd-browser daemon stop       # Stop daemon and Chrome
gsd-browser daemon start      # Explicit start (rarely needed)
```

**Step 7: Live viewer setup**

Open the narrated shared-screen viewer for a session:

```bash
gsd-browser --session demo navigate https://example.com
gsd-browser --session demo view
```

Use `--print-only` when another tool or person will open the URL:

```bash
gsd-browser --session demo view --print-only
```

The viewer runs on localhost and attaches to the session daemon. It shows live browser frames, action history, refs overlay, and pause/step/resume/abort controls.

**Step 8: CI/CD usage**

For CI pipelines, ensure headless mode and configure paths:

```bash
export GSD_BROWSER_BROWSER_PATH=$(which chromium-browser)
gsd-browser navigate https://staging.example.com
gsd-browser assert --checks '[{"kind": "text_visible", "text": "App loaded"}]'
gsd-browser daemon stop
```

</process>

<success_criteria>
Setup is complete when:
- `gsd-browser daemon start` succeeds
- `gsd-browser daemon health` reports `healthy` after startup
- Browser path is configured (if not in default location)
- Vault key is set (if using auth vault)
- Project config exists (if project-specific settings are needed)
</success_criteria>
