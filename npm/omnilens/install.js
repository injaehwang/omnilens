#!/usr/bin/env node

// Downloads the omnilens binary from GitHub Releases.

const { existsSync, mkdirSync, chmodSync, unlinkSync, renameSync } = require("fs");
const { join } = require("path");
const { execSync } = require("child_process");

const VERSION = "v1.1.1";
const REPO = "injaehwang/omnilens";

const PLATFORMS = {
  "linux-x64": { file: "omnilens-linux-x64.tar.gz", src: "omnilens", dest: "omnilens-bin" },
  "linux-arm64": { file: "omnilens-linux-arm64.tar.gz", src: "omnilens", dest: "omnilens-bin" },
  "darwin-x64": { file: "omnilens-darwin-x64.tar.gz", src: "omnilens", dest: "omnilens-bin" },
  "darwin-arm64": { file: "omnilens-darwin-arm64.tar.gz", src: "omnilens", dest: "omnilens-bin" },
  "win32-x64": { file: "omnilens-win32-x64.zip", src: "omnilens.exe", dest: "omnilens-bin.exe" },
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
const destPath = join(destDir, info.dest);

// Skip if already installed.
if (existsSync(destPath)) {
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
    execSync(
      `powershell -Command "Expand-Archive -Force -Path '${tmpFile}' -DestinationPath '${destDir}'"`,
      { stdio: "pipe" }
    );
  }

  // Rename extracted binary to omnilens-bin (so bin/omnilens node wrapper can find it).
  const extractedPath = join(destDir, info.src);
  if (existsSync(extractedPath)) {
    renameSync(extractedPath, destPath);
  }

  // Cleanup archive.
  try { unlinkSync(tmpFile); } catch {}

  // Make executable.
  if (process.platform !== "win32") {
    chmodSync(destPath, 0o755);
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
