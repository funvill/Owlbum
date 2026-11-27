#!/usr/bin/env bash
# Builds a test exe of main with a new version number.
#
# 1. Raises the last number of the workspace version (0.1.1 -> 0.1.2).
# 2. Turns CHANGELOG.md's "Unreleased" section into that version's section
#    and opens a new, empty "Unreleased" above it.
# 3. Commits both (no tag: tags are for real releases, made by a person) and
#    pushes main, so the next build on any computer takes the next number.
# 4. Builds the release exe (no installer) and copies it to the test folder.
#
# Usage: ./scripts/build-test.sh
#   OWLBUM_TEST_DIR  where the exe goes (default ~/Documents/OwlbumDemo)
#   OWLBUM_GIT       the git used to push (default git; the build agents
#                    use their push wrapper)
#   OWLBUM_NO_PUSH=1 commit without pushing
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"

git_push=${OWLBUM_GIT:-git}
test_dir=${OWLBUM_TEST_DIR:-$HOME/Documents/OwlbumDemo}

if [ "$(git rev-parse --abbrev-ref HEAD)" != main ]; then
  echo "Build test exes from main." >&2
  exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "The working tree has changes; commit or stash them first." >&2
  exit 1
fi
git pull -q --ff-only

old=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
IFS=. read -r major minor patch <<<"$old"
new="$major.$minor.$((patch + 1))"
base=$(git rev-parse --short HEAD)
today=$(date +%Y-%m-%d)

node - "$new" "$base" "$today" <<'EOF'
const fs = require("fs");
const [version, base, today] = process.argv.slice(2);
const file = "CHANGELOG.md";
let text = fs.readFileSync(file, "utf8").replace(/\r\n/g, "\n");
const head = "## [Unreleased]\n";
const start = text.indexOf(head);
if (start < 0) throw new Error("CHANGELOG.md has no '## [Unreleased]' section");
const after = text.indexOf("\n## [", start + head.length);
const body = text.slice(start + head.length, after < 0 ? undefined : after);
if (!/^- /m.test(body)) {
  console.error("CHANGELOG.md's Unreleased section lists no changes; add them first.");
  process.exit(1);
}
text = text.replace(head, `${head}\n## [${version}] - ${today}\n`);
text = text.replace(
  /^\[Unreleased\]: .*$/m,
  `[Unreleased]: https://github.com/funvill/Owlbum/compare/${base}...HEAD\n` +
    `[${version}]: https://github.com/funvill/Owlbum/tree/${base}`,
);
fs.writeFileSync(file, text);
EOF

sed -i "0,/^version = \"$old\"$/s//version = \"$new\"/" Cargo.toml
cargo update --workspace --offline -q
git add Cargo.toml Cargo.lock CHANGELOG.md
git -c user.name="SWS-Chipkin" -c user.email="sws-dev@chipkin.com" commit -q -F - <<EOF
Build $new

Raise the version for a test build and close its changelog section.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
if [ -z "${OWLBUM_NO_PUSH:-}" ]; then
  "$git_push" push -q origin main
fi

npm --prefix ui ci --silent
npm --prefix ui run tauri -- build --no-bundle
mkdir -p "$test_dir"
cp "$CARGO_TARGET_DIR/release/owlbum.exe" "$test_dir/owlbum-test.exe"
echo "Owlbum $new is at $test_dir/owlbum-test.exe"
