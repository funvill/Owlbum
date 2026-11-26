#!/usr/bin/env bash
# The gate every change must pass, on Windows (Git Bash) and on Linux.
# Stops at the first failure. See docs/08-desktop-and-release.md, "Tests".
set -euo pipefail

# `--quick` is the merge check for a single issue: format, lint, bindings and
# page types/lint, plus whatever focused tests the author ran. The full run
# (all Rust and page tests, page build) happens on main every few merges.
quick=false
[ "${1:-}" = "--quick" ] && quick=true
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

step "Wait for other gate runs that share this target directory"
# Worktrees share one target directory. Two gates at once would rebuild and
# replace each other's test programs while they run, so only one gate uses a
# target directory at a time. The lock is a directory (mkdir is atomic on
# every system); a lock older than 45 minutes is from a run that died.
if [ -n "${CARGO_TARGET_DIR:-}" ]; then
  mkdir -p "$CARGO_TARGET_DIR"
  gate_lock="$CARGO_TARGET_DIR/.gate-lock"
  waited=0
  until mkdir "$gate_lock" 2>/dev/null; do
    if [ -n "$(find "$gate_lock" -maxdepth 0 -mmin +45 2>/dev/null)" ]; then
      echo "Removing a stale gate lock"
      rm -rf "$gate_lock"
      continue
    fi
    [ "$waited" -eq 0 ] && echo "Another gate is running ($(cat "$gate_lock/owner" 2>/dev/null)); waiting"
    waited=$((waited + 5))
    sleep 5
  done
  echo "$PWD (pid $$)" > "$gate_lock/owner"
  trap 'rm -rf "$gate_lock"' EXIT
  echo "lock taken${waited:+ after ${waited}s}"
fi

step "Prune the shared target directory"
bash scripts/prune-target.sh

step "Mark this checkout's sources as newest"
# Every worktree shares one target directory, and Cargo decides whether a
# workspace crate is up to date by comparing file times with its last build,
# not by which checkout it came from. A worktree whose files are older than
# another worktree's last build would silently test that other code. Touching
# the workspace sources makes this checkout's build the newest, so Cargo
# rebuilds the workspace crates from these files. Dependencies are unaffected.
find crates src-tauri -type f \( -name '*.rs' -o -name '*.sql' -o -name '*.json' -o -name 'Cargo.toml' -o -name 'build.rs' \) \
  -not -path '*/target/*' -not -path '*/gen/*' -print0 | xargs -0 touch

step "Rust format"
cargo fmt --all --check

step "Rust lint"
cargo clippy --workspace --all-targets -- -D warnings

step "TypeScript bindings are current"
cargo test -p owlbum --test bindings

if [ "$quick" = false ]; then
step "Rust tests"
# The tests run with a temporary folder of their own, which must be empty
# when they finish: a test that leaves an album or data folder behind
# (usually a TempDir dropped while a database inside it is still open, so
# Windows refuses the delete and tempfile ignores the error) fails the gate
# instead of quietly filling the drive. Only this run's folder is removed.
cargo test --workspace --no-run
test_tmp=$(mktemp -d)
trap 'rm -rf "$test_tmp"; [ -n "${gate_lock:-}" ] && rm -rf "$gate_lock"' EXIT
test_tmp_native=$test_tmp
if command -v cygpath >/dev/null 2>&1; then
  test_tmp_native=$(cygpath -w "$test_tmp")
fi
TMP=$test_tmp_native TEMP=$test_tmp_native TMPDIR=$test_tmp_native \
  cargo test --workspace
leftovers=$(ls -A "$test_tmp")
if [ -n "$leftovers" ]; then
  echo "The Rust tests left $(echo "$leftovers" | wc -l) item(s) in their temporary folder:" >&2
  for item in $leftovers; do
    echo "  $item: $(ls -A "$test_tmp/$item" 2>/dev/null | tr '\n' ' ')" >&2
  done
  exit 1
fi
echo "no temporary folders left behind"
fi

step "Page types"
npm --prefix ui run typecheck

step "Page lint"
npm --prefix ui run lint

if [ "$quick" = false ]; then
step "Page tests"
npm --prefix ui test

step "Page build"
npm --prefix ui run build
fi

if [ "$quick" = true ]; then
  printf '\nQuick checks passed (full test run not done).\n'
else
  printf '\nAll checks passed.\n'
fi
