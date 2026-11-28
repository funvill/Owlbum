#!/usr/bin/env bash
# Rewrites the page's TypeScript bindings (ui/src/api/bindings) and the page's
# capability file (src-tauri/capabilities/main.json) from the Rust command
# registry. Run it after changing a command, an event or a type they
# use, and commit the result. scripts/check.sh fails while they are stale.
set -euo pipefail
source "$(dirname "$0")/env.sh"

OWLBUM_WRITE_BINDINGS=1 cargo test -p owlbum --test bindings
echo "Bindings written to ui/src/api/bindings and src-tauri/capabilities/main.json."
