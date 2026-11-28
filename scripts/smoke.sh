#!/usr/bin/env bash
# The smoke test (docs/08-desktop-and-release.md, "The layers", row 5):
# starts the release program under tauri-driver against a generated `trip`
# album and checks, in the real window, what the page tests cannot. It is
# opt-in: check.sh does not run it. Windows only for now (Linux is paused).
#
# Usage: ./scripts/smoke.sh [--no-build]
#   --no-build      use the release exe already in the target directory
#   MSEDGEDRIVER    msedgedriver.exe matching the installed WebView2 runtime
#                   (default: msedgedriver on PATH)
#   TAURI_DRIVER    tauri-driver (default: on PATH, or ~/.cargo/bin)
#   SMOKE_KEEP=1    keep the temp folder (album, data folder, log) afterwards
#
# One-time setup: `cargo install tauri-driver --locked`, and download
# msedgedriver for the WebView2 version shown in Edge WebView2's registry
# key (https://developer.microsoft.com/microsoft-edge/tools/webdriver/).
#
# Nothing touches the network, the registry, or the real app data: the
# program runs with OWLBUM_DATA_DIR in a temp folder (msedgedriver gives
# WebView2 a fresh profile there too), without update checks, What's new or
# a browser (OWLBUM_NO_BROWSER), and the folder dialog answers the fixture
# album (OWLBUM_PICK_FOLDER).
set -euo pipefail
source "$(dirname "$0")/env.sh"

build=true
[ "${1:-}" = "--no-build" ] && build=false

case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*) ;;
  *) echo "The smoke test runs on Windows only for now." >&2; exit 1 ;;
esac

driver=${TAURI_DRIVER:-$(command -v tauri-driver || echo "$HOME/.cargo/bin/tauri-driver.exe")}
native=${MSEDGEDRIVER:-$(command -v msedgedriver || true)}
if [ ! -x "$driver" ]; then
  echo "tauri-driver not found: cargo install tauri-driver --locked" >&2
  exit 1
fi
if [ -z "$native" ] || [ ! -x "$native" ]; then
  echo "msedgedriver not found: set MSEDGEDRIVER to the one matching WebView2." >&2
  exit 1
fi

if $build; then
  npm --prefix ui run tauri -- build --no-bundle
fi
exe="$CARGO_TARGET_DIR/release/owlbum.exe"
[ -f "$exe" ] || { echo "No release build at $exe" >&2; exit 1; }

tmp=$(mktemp -d)
driver_pid=
cleanup() {
  # The driver and the msedgedriver it started, as a tree.
  if [ -n "$driver_pid" ]; then
    taskkill //F //T //PID "$(cat "/proc/$driver_pid/winpid")" >/dev/null 2>&1 || true
  fi
  if [ -n "${SMOKE_KEEP:-}" ]; then
    echo "Kept $tmp"
  else
    # The webview may still hold its folder for a moment after quitting.
    for _ in 1 2 3 4 5; do rm -rf "$tmp" 2>/dev/null && break; sleep 1; done
  fi
}
trap cleanup EXIT

# A copy, so another build replacing the exe cannot change it mid-run.
cp "$exe" "$tmp/owlbum.exe"
# Worktrees share the target directory, so the exe may be another branch's
# build. One without the test variables would show the real folder dialog
# and open the real browser: refuse it.
if ! grep -q OWLBUM_NO_BROWSER "$tmp/owlbum.exe"; then
  echo "$exe does not know OWLBUM_NO_BROWSER (an older build?); build again." >&2
  exit 1
fi
cargo run -q -p owlbum-fixtures -- trip "$tmp/album"

export OWLBUM_DATA_DIR OWLBUM_PICK_FOLDER
OWLBUM_DATA_DIR=$(cygpath -w "$tmp/data")
OWLBUM_PICK_FOLDER=$(cygpath -w "$tmp/album")
export OWLBUM_NO_BROWSER=1 OWLBUM_NO_UPDATE_CHECK=1 OWLBUM_NO_WHATS_NEW=1

# msedgedriver gives WebView2 a fresh profile in a scoped_dir under TEMP and
# leaves it behind when the program quits first; keep those in the temp
# folder too.
mkdir -p "$tmp/temp"
TMP=$(cygpath -w "$tmp/temp") TEMP=$(cygpath -w "$tmp/temp") \
  "$driver" --port 4444 --native-port 4445 --native-driver "$native" >"$tmp/driver.log" 2>&1 &
driver_pid=$!

node e2e/smoke.mjs "$(cygpath -w "$tmp/owlbum.exe")" "$OWLBUM_PICK_FOLDER" "$OWLBUM_DATA_DIR"
