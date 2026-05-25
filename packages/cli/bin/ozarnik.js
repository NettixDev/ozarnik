#!/usr/bin/env node
// Unified entry point for the ОЗАРНИК CLI.
// Detects host platform and spawns the platform-specific OZARNIK binary that was installed
// via the matching optional dependency.

import { spawn } from "node:child_process";
import { existsSync, realpathSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const require = createRequire(import.meta.url);

const PLATFORM_PACKAGE_BY_TARGET = {
  "x86_64-unknown-linux-musl": "@ozarnik/cli-linux-x64",
  "aarch64-unknown-linux-musl": "@ozarnik/cli-linux-arm64",
  "x86_64-pc-windows-msvc": "@ozarnik/cli-win32-x64",
  "aarch64-pc-windows-msvc": "@ozarnik/cli-win32-arm64",
};

const { platform, arch } = process;

let targetTriple = null;
switch (platform) {
  case "linux":
  case "android":
    if (arch === "x64") targetTriple = "x86_64-unknown-linux-musl";
    else if (arch === "arm64") targetTriple = "aarch64-unknown-linux-musl";
    break;
  case "win32":
    if (arch === "x64") targetTriple = "x86_64-pc-windows-msvc";
    else if (arch === "arm64") targetTriple = "aarch64-pc-windows-msvc";
    break;
}

if (!targetTriple) {
  throw new Error(`Unsupported platform: ${platform} (${arch})`);
}

const platformPackage = PLATFORM_PACKAGE_BY_TARGET[targetTriple];
const binaryName = process.platform === "win32" ? "OZARNIK.exe" : "OZARNIK";

function resolveBinary() {
  try {
    const pkgJson = require.resolve(`${platformPackage}/package.json`);
    const pkgRoot = path.dirname(pkgJson);
    const candidate = path.join(pkgRoot, "bin", binaryName);
    if (existsSync(candidate)) return candidate;
  } catch {}
  // Fallback: bundled vendor/ directory next to this launcher (for local installs).
  const localCandidate = path.join(__dirname, "..", "vendor", targetTriple, "bin", binaryName);
  if (existsSync(localCandidate)) return localCandidate;
  return null;
}

const binaryPath = resolveBinary();
if (!binaryPath) {
  throw new Error(
    `Missing optional dependency ${platformPackage}. Reinstall: npm install -g @ozarnik/cli@latest`,
  );
}

const env = { ...process.env };
env.OZARNIK_MANAGED_PACKAGE_ROOT = realpathSync(path.join(__dirname, ".."));

const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  env,
});

child.on("error", (err) => {
  console.error(err);
  process.exit(1);
});

["SIGINT", "SIGTERM", "SIGHUP"].forEach((sig) => {
  process.on(sig, () => {
    if (!child.killed) {
      try { child.kill(sig); } catch {}
    }
  });
});

const result = await new Promise((resolve) => {
  child.on("exit", (code, signal) => {
    if (signal) resolve({ type: "signal", signal });
    else resolve({ type: "code", exitCode: code ?? 1 });
  });
});

if (result.type === "signal") {
  process.kill(process.pid, result.signal);
} else {
  process.exit(result.exitCode);
}
