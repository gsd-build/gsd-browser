#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");
const https = require("https");
const { execSync } = require("child_process");

const REPO = "gsd-build/gsd-browser";

const PLATFORM_MAP = {
  "darwin-arm64": "gsd-browser-darwin-arm64",
  "darwin-x64": "gsd-browser-darwin-x64",
  "linux-arm64": "gsd-browser-linux-arm64",
  "linux-x64": "gsd-browser-linux-x64",
};

function fetchJSON(url) {
  return new Promise((resolve, reject) => {
    https.get(url, { headers: { "User-Agent": "gsd-browser-npm" } }, (res) => {
      if (res.statusCode === 302 || res.statusCode === 301) {
        return fetchJSON(res.headers.location).then(resolve, reject);
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`HTTP ${res.statusCode} from ${url}`));
      }
      let data = "";
      res.on("data", (chunk) => (data += chunk));
      res.on("end", () => {
        try { resolve(JSON.parse(data)); }
        catch (e) { reject(e); }
      });
    }).on("error", reject);
  });
}

function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    https.get(url, { headers: { "User-Agent": "gsd-browser-npm" } }, (res) => {
      if (res.statusCode === 302 || res.statusCode === 301) {
        return downloadFile(res.headers.location, dest).then(resolve, reject);
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`HTTP ${res.statusCode} downloading ${url}`));
      }
      const file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on("finish", () => { file.close(); resolve(); });
    }).on("error", reject);
  });
}

async function main() {
  const platform = process.platform;
  const arch = process.arch;
  const key = `${platform}-${arch}`;
  const binaryName = PLATFORM_MAP[key];

  if (!binaryName) {
    console.error(
      `gsd-browser: unsupported platform ${key}.\n` +
      `Supported: ${Object.keys(PLATFORM_MAP).join(", ")}`
    );
    process.exit(1);
  }

  const binDir = path.join(__dirname, "..", "bin");
  fs.mkdirSync(binDir, { recursive: true });

  const isWindows = platform === "win32";
  const targetName = isWindows ? "gsd-browser.exe" : "gsd-browser";
  const targetPath = path.join(binDir, targetName);

  // Check if binary already exists (e.g., bundled in package)
  if (fs.existsSync(targetPath)) {
    console.log(`gsd-browser: binary already present at ${targetPath}`);
    return;
  }

  // Determine version from package.json
  const pkg = require("../package.json");
  let version = pkg.version;

  // Download from GitHub releases
  let url = `https://github.com/${REPO}/releases/download/v${version}/${binaryName}`;

  // If version is 0.1.0 or similar pre-release, try latest
  try {
    console.log(`gsd-browser: downloading ${binaryName} v${version}...`);
    await downloadFile(url, targetPath);
  } catch (e) {
    // Fall back to latest release
    console.log(`gsd-browser: v${version} not found, trying latest release...`);
    try {
      const release = await fetchJSON(`https://api.github.com/repos/${REPO}/releases/latest`);
      const asset = release.assets.find((a) => a.name === binaryName);
      if (!asset) {
        throw new Error(`No asset ${binaryName} in latest release`);
      }
      await downloadFile(asset.browser_download_url, targetPath);
      version = release.tag_name;
    } catch (e2) {
      console.error(
        `gsd-browser: failed to download binary.\n` +
        `  ${e2.message}\n` +
        `  You can build from a repo checkout:\n` +
        `    git clone https://github.com/${REPO}.git\n` +
        `    cd gsd-browser\n` +
        `    cargo install --path cli\n` +
        `  Or use the installer: curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash`
      );
      process.exit(1);
    }
  }

  if (!isWindows) {
    fs.chmodSync(targetPath, 0o755);
  }

  console.log(`gsd-browser: installed ${binaryName} (${version}) to ${targetPath}`);
}

main().catch((e) => {
  console.error(`gsd-browser: postinstall failed: ${e.message}`);
  process.exit(1);
});
