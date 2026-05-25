#!/usr/bin/env bash
# ОЗАРНИК build + publish script for a fresh Ubuntu 22.04/24.04 VDS.
#
# Usage:
#   1. SSH into your VDS as root (or a sudoer).
#   2. Save your npm token: export NPM_TOKEN=npm_xxxxxxxxxxxx
#   3. (Optional) Set VERSION; defaults to 0.1.0.
#   4. curl -fsSL https://raw.githubusercontent.com/NettixDev/ozarnik/main/scripts/vds-publish.sh | bash
#      or: bash vds-publish.sh
#
# Builds 4 platform binaries:
#   - linux-x64    (native cargo)
#   - linux-arm64  (cross)
#   - win32-x64    (cargo-xwin)
#   - win32-arm64  (cargo-xwin)
# Then stages them into packages/cli-*/bin/ and publishes 5 npm packages.

set -euo pipefail

VERSION="${VERSION:-0.1.0}"
REPO_URL="${REPO_URL:-https://github.com/NettixDev/ozarnik.git}"
WORKDIR="${WORKDIR:-$HOME/ozarnik-build}"

if [ -z "${NPM_TOKEN:-}" ]; then
  echo "ERROR: set NPM_TOKEN env var to your npm publish token before running." >&2
  echo "  export NPM_TOKEN=npm_xxxxxxxxxx" >&2
  exit 1
fi

echo "==> Updating apt"
sudo apt-get update -y
sudo apt-get install -y curl build-essential pkg-config libssl-dev git ca-certificates clang lld

echo "==> Installing Rust toolchain"
if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"

echo "==> Installing Rust targets"
rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl x86_64-pc-windows-msvc aarch64-pc-windows-msvc

echo "==> Installing musl tools for static linux binaries"
sudo apt-get install -y musl-tools

echo "==> Installing cross (for linux arm64)"
cargo install cross --locked || true

echo "==> Installing cargo-xwin (for Windows targets without MSVC)"
cargo install cargo-xwin --locked || true

echo "==> Installing Node.js 20"
if ! command -v node >/dev/null 2>&1; then
  curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
  sudo apt-get install -y nodejs
fi

echo "==> Cloning repo to $WORKDIR"
if [ -d "$WORKDIR/.git" ]; then
  cd "$WORKDIR" && git pull --ff-only
else
  git clone --depth 1 "$REPO_URL" "$WORKDIR"
  cd "$WORKDIR"
fi

cd "$WORKDIR/codex-rs"

echo "==> Building linux-x64 (native musl)"
cargo build --release --target x86_64-unknown-linux-musl -p codex-cli

echo "==> Building linux-arm64 (cross)"
cross build --release --target aarch64-unknown-linux-musl -p codex-cli

echo "==> Building win32-x64 (cargo-xwin)"
cargo xwin build --release --target x86_64-pc-windows-msvc -p codex-cli

echo "==> Building win32-arm64 (cargo-xwin)"
cargo xwin build --release --target aarch64-pc-windows-msvc -p codex-cli

cd "$WORKDIR"

echo "==> Staging binaries into packages/"
declare -A BIN_MAP=(
  [linux-x64]=x86_64-unknown-linux-musl/release/OZARNIK
  [linux-arm64]=aarch64-unknown-linux-musl/release/OZARNIK
  [win32-x64]=x86_64-pc-windows-msvc/release/OZARNIK.exe
  [win32-arm64]=aarch64-pc-windows-msvc/release/OZARNIK.exe
)

for pkg in "${!BIN_MAP[@]}"; do
  src="codex-rs/target/${BIN_MAP[$pkg]}"
  dest_dir="packages/cli-${pkg}/bin"
  if [[ "$pkg" == win32-* ]]; then
    dest="$dest_dir/OZARNIK.exe"
  else
    dest="$dest_dir/OZARNIK"
  fi
  mkdir -p "$dest_dir"
  cp "$src" "$dest"
  [[ "$pkg" != win32-* ]] && chmod +x "$dest"
  echo "  staged $pkg -> $dest"
done

echo "==> Syncing version $VERSION across package.json files"
for f in packages/cli-*/package.json packages/cli/package.json; do
  node -e "const fs=require('fs');const p=JSON.parse(fs.readFileSync('$f'));p.version='$VERSION';if(p.optionalDependencies){for(const k of Object.keys(p.optionalDependencies)){p.optionalDependencies[k]='$VERSION';}}fs.writeFileSync('$f',JSON.stringify(p,null,2)+'\n');"
done

echo "==> Writing npm auth"
echo "//registry.npmjs.org/:_authToken=$NPM_TOKEN" > "$HOME/.npmrc"

echo "==> Publishing platform packages"
for d in packages/cli-*; do
  echo "  publishing $d"
  (cd "$d" && npm publish --access public)
done

echo "==> Publishing main @ozarnik/cli package"
(cd packages/cli && npm publish --access public)

echo "==> Done! Install with: npm install -g @ozarnik/cli"
