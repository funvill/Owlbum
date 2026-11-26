# Sourced by the other scripts. Moves to the repository root and points
# Cargo at one shared build directory for every worktree of this clone.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if [ -z "${CARGO_TARGET_DIR:-}" ]; then
  common_dir=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)
  if [ -n "$common_dir" ]; then
    # <main checkout>/.git -> <main checkout>/target
    CARGO_TARGET_DIR="$(dirname "$common_dir")/target"
    export CARGO_TARGET_DIR
  fi
fi
