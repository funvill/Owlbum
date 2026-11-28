#!/usr/bin/env bash
# Builds the Windows installer on this computer, as the release workflow
# does but without the updater files and their signing key
# (docs/08-desktop-and-release.md, "Packaging").
#
# 1. Fetches ONNX Runtime, checked by its hash (scripts/fetch-ort.mjs).
# 2. Builds the third-party notices, failing on any missing licence text
#    (scripts/notices.mjs --strict; needs `cargo install cargo-about
#    --locked --features cli`).
# 3. Builds the page, the program and the NSIS installer with the bundle
#    resources (src-tauri/tauri.bundle.conf.json).
#
# Usage: ./scripts/build-installer.sh
# The installer is at $CARGO_TARGET_DIR/release/bundle/nsis/.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"

node scripts/fetch-ort.mjs
node scripts/notices.mjs --strict
npm --prefix ui run tauri -- build --bundles nsis --config src-tauri/tauri.bundle.conf.json
ls "$CARGO_TARGET_DIR"/release/bundle/nsis/*.exe
