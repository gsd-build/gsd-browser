#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");

const PLATFORM_MAP = {
  "darwin-arm64": "browser-tools-darwin-arm64",
  "darwin-x64": "browser-tools-darwin-x64",
  "linux-arm64": "browser-tools-linux-arm64",
  "linux-x64": "browser-tools-linux-x64",
  "win32-x64": "browser-tools-win-x64.exe",
};

function main() {
  const platform = process.platform;
  const arch = process.arch;
  const key = `${platform}-${arch}`;
  const binaryName = PLATFORM_MAP[key];

  if (!binaryName) {
    console.error(
      `browser-tools: unsupported platform ${platform}-${arch}.\n` +
        `Supported: ${Object.keys(PLATFORM_MAP).join(", ")}`
    );
    process.exit(1);
  }

  const binDir = path.join(__dirname, "..", "bin");
  const sourcePath = path.join(binDir, binaryName);
  const isWindows = platform === "win32";
  const targetName = isWindows ? "browser-tools.exe" : "browser-tools";
  const targetPath = path.join(binDir, targetName);

  // Check if the platform-specific binary exists
  if (!fs.existsSync(sourcePath)) {
    console.error(
      `browser-tools: binary not found at ${sourcePath}.\n` +
        `This package may not include pre-built binaries for ${key}.\n` +
        `You can build from source: cargo install browser-tools`
    );
    process.exit(1);
  }

  // Remove existing target if present
  try {
    fs.unlinkSync(targetPath);
  } catch (_) {
    // Ignore — file may not exist
  }

  if (isWindows) {
    // On Windows, copy instead of symlink to avoid permission issues
    fs.copyFileSync(sourcePath, targetPath);
  } else {
    // On Unix, create a relative symlink
    fs.symlinkSync(binaryName, targetPath);
    // Ensure the binary is executable
    try {
      fs.chmodSync(sourcePath, 0o755);
    } catch (_) {
      // Ignore — may already be executable
    }
  }

  console.log(`browser-tools: linked ${binaryName} → ${targetName}`);
}

main();
