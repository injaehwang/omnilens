#!/usr/bin/env node

// Downloads the omnilens binary from GitHub Releases.
// No platform-specific npm packages needed.

const { existsSync, mkdirSync, chmodSync, createWriteStream, unlinkSync } = require("fs");
const { join } = require("path");
const { execSync } = require("child_process");

const VERSION = "v1.0.2";
const REPO = "injaehwang/omnilens";

const PLATFORMS = {
  "linux-x64": { file: "omnilens-linux-x64.tar.gz", binary: "omnilens" },
  "linux-arm64": { file: "omnilens-linux-arm64.tar.gz", binary: "omnilens" },
  "darwin-x64": { file: "omnilens-darwin-x64.tar.gz", binary: "omnilens" },
  "darwin-arm64": { file: "omnilens-darwin-arm64.tar.gz", binary: "omnilens" },
  "win32-x64": { file: "omnilens-win32-x64.zip", binary: "omnilens.exe" },
};

const platform = `${process.platform}-${process.arch}`;
const info = PLATFORMS[platform];

if (!info) {
  console.error(`omnilens: unsupported platform ${platform}`);
  console.error("Build from source: cargo install --git https://github.com/" + REPO);
  process.exit(1);
}

const url = `https://github.com/${REPO}/releases/download/${VERSION}/${info.file}`;
const destDir = join(__dirname, "bin");
const dest = join(destDir, info.binary);

// Skip if already installed.
if (existsSync(dest)) {
  console.log("omnilens: binary already installed");
  process.exit(0);
}

mkdirSync(destDir, { recursive: true });

try {
  const tmpFile = join(destDir, info.file);

  // Download.
  console.log(`omnilens: downloading ${info.file}...`);
  execSync(`curl -fsSL "${url}" -o "${tmpFile}"`, { stdio: "pipe" });

  // Extract.
  if (info.file.endsWith(".tar.gz")) {
    execSync(`tar xzf "${tmpFile}" -C "${destDir}"`, { stdio: "pipe" });
  } else {
    // Windows zip — use PowerShell Expand-Archive.
    execSync(
      `powershell -Command "Expand-Archive -Force -Path '${tmpFile}' -DestinationPath '${destDir}'"`,
      { stdio: "pipe" }
    );
  }

  // Cleanup archive.
  try { unlinkSync(tmpFile); } catch {}

  // Make executable.
  if (process.platform !== "win32") {
    chmodSync(dest, 0o755);
  }

  console.log(`omnilens: installed ${platform} binary`);
  console.log(``);
  console.log(`  1. cd your-project`);
  console.log(`  2. omnilens`);
  console.log(`  3. Tell your AI: "Review the omnilens snapshot"`);
  console.log(``);
} catch (err) {
  console.error(`omnilens: failed to download binary`);
  console.error(err.message);
  console.error(`\nFallback: cargo install --git https://github.com/${REPO}`);
  process.exit(1);
}
