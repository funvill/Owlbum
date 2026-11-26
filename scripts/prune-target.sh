#!/usr/bin/env bash
# Keeps the shared target directory from filling the disk. Every worktree
# shares one target dir, and each source change leaves a new build-script
# output for the workspace crates, so old ones pile up (hundreds of copies).
# Called by check.sh before building. Deleting these is safe: Cargo rebuilds
# whatever it needs.
set -euo pipefail

target="${CARGO_TARGET_DIR:-target}"
for profile in debug release; do
  build="$target/$profile/build"
  [ -d "$build" ] || continue
  for crate in owlbum owlbum-core owlbum-fixtures owlbum-lib; do
    dir="$build/$crate"
    [ -d "$dir" ] || continue
    # Keep the 20 newest outputs.
    (cd "$dir" 2>/dev/null && ls -t | tail -n +21 | xargs -r rm -rf) || true
  done
  # The incremental cache grows without bound across worktrees; clear it
  # when it passes 4 GB.
  inc="$target/$profile/incremental"
  if [ -d "$inc" ]; then
    size_kb=$(du -sk "$inc" 2>/dev/null | cut -f1 || true)
    size_kb=${size_kb:-0}
    if [ "$size_kb" -gt 4000000 ]; then
      echo "Clearing $inc ($((size_kb / 1024)) MB)"
      rm -rf "$inc"
    fi
  fi
done
