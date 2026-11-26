#!/usr/bin/env bash
# The gate every change must pass, on Windows (Git Bash) and on Linux.
# Stops at the first failure. See docs/08-desktop-and-release.md, "Tests".
set -euo pipefail
source "$(dirname "$0")/env.sh"

step() { printf '\n==> %s\n' "$*"; }

step "Text files are UTF-8"
node scripts/check-utf8.mjs

step "Page dependencies"
if [ ! -d ui/node_modules ] || [ ui/package-lock.json -nt ui/node_modules/.package-lock.json ]; then
  npm --prefix ui ci
else
  echo "up to date"
fi

step "Rust format"
cargo fmt --all --check

step "Rust lint"
cargo clippy --workspace --all-targets -- -D warnings

step "TypeScript bindings are current"
cargo test -p owlbum --test bindings

step "Rust tests"
cargo test --workspace

step "Page types"
npm --prefix ui run typecheck

step "Page lint"
npm --prefix ui run lint

step "Page tests"
npm --prefix ui test

step "Page build"
npm --prefix ui run build

printf '\nAll checks passed.\n'
