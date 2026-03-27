#!/usr/bin/env node

// Postinstall script: resolve the correct platform-specific binary
// and symlink it to bin/omnilens.

const { existsSync, mkdirSync, copyFileSync, chmodSync } = require("fs");
const { join, dirname } = require("path");

const PLATFORMS = {
  "linux-x64": "omnilens-linux-x64",
  "linux-arm64": "omnilens-linux-arm64",
  "darwin-x64": "omnilens-darwin-x64",
  "darwin-arm64": "omnilens-darwin-arm64",
  "win32-x64": "omnilens-win32-x64",
};

const platform = `${process.platform}-${process.arch}`;
const pkgName = PLATFORMS[platform];

if (!pkgName) {
  console.error(
    `omnilens: unsupported platform ${platform}. Supported: ${Object.keys(PLATFORMS).join(", ")}`
  );
  console.error(
    "You can build from source instead: cargo install omnilens"
  );
  process.exit(1);
}

try {
  const pkgPath = dirname(require.resolve(`${pkgName}/package.json`));
  const ext = process.platform === "win32" ? ".exe" : "";
  const binaryName = `omnilens${ext}`;
  const src = join(pkgPath, binaryName);
  const destDir = join(__dirname, "bin");
  const dest = join(destDir, binaryName);

  if (!existsSync(src)) {
    console.error(`omnilens: binary not found at ${src}`);
    console.error(
      "Try reinstalling: npm install -g omnilens"
    );
    process.exit(1);
  }

  mkdirSync(destDir, { recursive: true });
  copyFileSync(src, dest);

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
  console.error(`omnilens: failed to install binary for ${platform}`);
  console.error(err.message);
  console.error(
    "\nFallback: install from source with `cargo install omnilens`"
  );
  process.exit(1);
}
