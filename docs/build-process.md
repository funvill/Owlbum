# Build process and how to resume

The v1 build ran unattended with subagents. It ran one GitHub issue at a time from 2026-11-25 to 2026-11-26 (milestones 0, 1 and 2), paused at the owner's request, and resumed on 2026-11-27 on a second Windows machine, where the rest was built in batches (below). The progress record is the issues on `funvill/Owlbum` (issue numbers equal [roadmap](roadmap.md) numbers up to #69), and the latest status is the newest comment on #69 and the Status section of the roadmap.

In this file `<checkout>` is the main checkout (for example `C:/dev/Owlbum`); worktrees live beside it in `owlbum-wt/`.

## How the build runs

- **Batches.** Related issues are built together: one branch and one pull request per batch of related issues (for example "A file actions, #38 to #42"), in a worktree `owlbum-wt/batch-<letter>`. At most two batch agents run at a time, and two batches never touch the same files if it can be helped. The orchestrator gives each agent the brief below, the batch's issues and its branch name.
- **Checks.** Each agent runs `check.sh --quick` plus the focused tests for what it changed, then merges by a fast-forward push to `main` (never `gh pr merge`). The orchestrator runs the full gate on `main` every few merges, and CI (`.github/workflows/checks.yml`) runs it on Windows for every push to `main` and every pull request.
- **Test builds.** After a batch or two lands, the orchestrator builds a test exe with `./scripts/build-test.sh` from an up-to-date `main` in the main checkout (set `OWLBUM_GIT` to the push wrapper). It raises the last number of the version (0.1.4 to 0.1.5), turns the `Unreleased` section of `CHANGELOG.md` into that version's section, commits and pushes that (no tag), and copies the exe, with ONNX Runtime beside it, to `~/Documents/OwlbumDemo` (`OWLBUM_TEST_DIR`). The window title shows the version, so the owner can say which build a finding is about, and `CHANGELOG.md` says what each build changed. That is why every agent writes each change a user would notice into `Unreleased`.
- **Owner feedback.** The owner tries each test build and comments on #69. Each finding becomes an issue (or a fix at once when it is tiny) and goes ahead of the remaining batches.

## Owner feedback

Feedback on a test build is handled as [AGENTS.md](../AGENTS.md) says: one `Feedback on <version>` issue first, research on every item, linked issues for the large items, closed when all of it is addressed.

## Resuming

1. Clone the repository, install Rust (the version in `rust-toolchain.toml` installs itself), Node 24 (`ui/.nvmrc`), Git for Windows, and the WebView2 runtime. Run `./scripts/check.sh` once from Git Bash; it must end with "All checks passed".
2. Recreate the push wrappers outside the repository, in `.git/agent/bin/` (not tracked):
   - `ghf`: `#!/bin/sh` then `GH_TOKEN=$(gh auth token --user funvill) exec gh "$@"`
   - `gitf`: the same idea for `git push` (the token of the account that may push to `funvill/Owlbum`).
   Agent commits are authored by `SWS-Chipkin <sws-dev@chipkin.com>`; the pushing account is separate.
3. Copy the brief below to `.git/agent/BRIEF.md`, replacing `<checkout>` with the new machine's path, and append the lessons below as its last section.
4. Read the open issues, the newest comment on #69 and the Status section of [roadmap.md](roadmap.md).
5. The owner's standing orders (2026-11-26 and 2026-11-27): batches of related issues, at most two agents at a time; agents run `check.sh --quick` plus focused tests, and the orchestrator runs the full gate on `main` every few merges; no over-verifying; Windows only, Linux paused until the owner says otherwise; never publish a release, push a version tag, enable Pages or create signing keys.

## Lessons from the build

- All worktrees share one Cargo target directory. Touch the workspace sources before a focused `cargo test` (the brief has the command), or Cargo may test another worktree's build. `check.sh` does this itself and holds a lock so two gates never share the directory at once.
- A worktree that adds a page package does not install it in the main checkout: run `npm --prefix ui ci` there before a release build.
- Do not junction `ui/node_modules` into a worktree: `check.sh` runs `npm ci` when the lock file looks newer, and that goes through the junction into the main checkout. Run `npm --prefix ui ci` in the worktree (about 190 MB) and delete the worktree afterwards.
- Measure speed on a release build. Debug builds were many times slower on real photos until 16df7c2 optimised the image crates in debug builds.
- The `rust-lld` linker was tried and dropped: with a shared target directory it made every checkout rebuild the dependencies.
- CI broke twice after agent merges on 2026-11-27, from checks the quick gate does not run. So, before merging:
  - Run the repository-wide guard test too: `cargo test -p owlbum-core --lib registry::tests`. It fails on any file outside the media registry that holds two or more media extensions, test data included; such a file goes in `GUARD_ALLOW` with a reason.
  - When a change alters a shape other code relies on (a command's reply, a struct's fields, a settings key), grep `src-tauri/tests` and `ui/src` for its other users and run those tests as well.
  - After touching any CSS, run `npx vitest run src/styles` (the style rules: tokens only, never `outline: none`).
- Temporary folders must be empty after every test, or `check.sh` fails on leftovers:
  - A shell test fixture in `src-tauri/tests/` closes its album in `Drop` (`album_close`) before its temporary folder goes.
  - A fixture struct that holds an album and its temporary folder lists the album first, since fields drop in declaration order.
  - To check a focused run yourself: point `TMP` and `TEMP` at an empty folder for the run and look that it is still empty afterwards.
- Page files are not kept prettier-formatted. Never run prettier over a whole file; edit by hand to match the code around you (eslint and tsc are the checks).
- Do not write an issue number as `#` and digits in a .ts, .tsx or .css comment: the style rules read it as a hex colour. Write "issue 146".
- A test that depends on order or timing must hold the work with something that cannot be undone underneath it (a scan flag, not a pause that opening clears), and is run three times before merging.
- The Tauri mock runtime answers an empty string for a window's title, so the title is tested as a unit (`window_title`), and real-window behaviour (accelerators, title, CSP, the map's look) needs the owner's test pass.

## The agent brief, as used

The brief as it stood at the final pass, with the paths written as `<checkout>`. Its lessons section is the list above.

### Standing brief for Owlbum build agents

> **Pushing and GitHub: ALWAYS use the wrappers** `<checkout>/.git/agent/bin/gitf push ...` and `<checkout>/.git/agent/bin/ghf ...`. Plain `git push`/`gh` run as an account with read-only access and get 403. A 403 means you forgot the wrapper, not that you are blocked: never stop to ask about accounts.

You are implementing one GitHub issue of Owlbum (repo `funvill/Owlbum`, local `<checkout>`). The owner has approved the design and asked for unattended work: no questions to the user. If you hit a roadblock, record it on the issue and make the best reduced decision you can.

## Read first

1. `docs/README.md`, `docs/01-architecture.md`, `docs/02-data.md` (canonical when specs disagree).
2. The issue body (`ghf issue view N --repo funvill/Owlbum --comments`) and every spec section it links.
3. Existing code near what you touch. Follow its patterns.

## Tools

- GitHub: always use `<checkout>/.git/agent/bin/ghf` instead of `gh` (it uses the `funvill` login's token per command). Never run `gh auth switch`.
- Pushing: `<checkout>/.git/agent/bin/gitf push ...` instead of `git push`.
- Shell is Git Bash on Windows. Linux checks run in WSL `Ubuntu-22.04` (Rust and Tauri deps installed there; sudo works). Run Linux checks from a clone inside the WSL filesystem, never on `/mnt/c`. Several agents run at once, so each issue uses its own WSL worktree of the base clone `~/owlbum` (never check out branches in `~/owlbum` itself): `wsl -d Ubuntu-22.04 -- bash -lc 'cd ~/owlbum && git fetch -q origin && (git worktree add -f ~/owlbum-wt/issue-N origin/<branch> 2>/dev/null || true) && cd ~/owlbum-wt/issue-N && git checkout -q --detach origin/<branch> && CARGO_TARGET_DIR=~/owlbum-target ./scripts/check.sh'` (re-run the fetch and checkout after each push). Remove it when done: `git -C ~/owlbum worktree remove --force ~/owlbum-wt/issue-N`. All WSL builds share `~/owlbum-target`.
- Disk on C: is tight (~25 GB). Use the shared target directory `<checkout>/target`; do not create extra target dirs. `scripts/check.sh` and `scripts/bindings.sh` set `CARGO_TARGET_DIR` to it by themselves (from `git rev-parse --git-common-dir`). For any other cargo or tauri command in a worktree, first run `export CARGO_TARGET_DIR=<checkout>/target` (a relative `target-dir` in `.cargo/config.toml` cannot reach it from both the main checkout and `owlbum-wt/*`). Cargo locks the shared directory, so a build may wait for another agent's build. Do not download large files into the repo.
- Commands and events: declare a command with `command!` in `src-tauri/src/commands/<prefix>.rs` and add it to that file's `area!` list; an event is a payload type in `owlbum-core` with an `Event` impl, listed in `events` of the area that sends it. Then run `./scripts/bindings.sh` (rewrites `ui/src/api/bindings/`, which is committed) and add the fake handler in `ui/src/api/fake/<prefix>.ts` (typecheck fails until you do). On a rebase conflict inside `ui/src/api/bindings/`, take either side and rerun `./scripts/bindings.sh`. Shell tests go in `src-tauri/tests/` using Tauri's mock runtime (see `src-tauri/tests/commands.rs`).

## Identity (mandatory)

- Every commit: `git -c user.name="SWS-Chipkin" -c user.email="sws-dev@chipkin.com" commit ...`. Never any other author or committer. Same for merges, rebases (`git -c user.name=... -c user.email=... rebase`), cherry-picks, tags.
- Commit message: short subject, body wrapped at ~72 columns, include `Closes #N` (the final commit), and end with:

  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`

- Never commit secrets, `.env`, tokens or large binaries.

## Workflow for issue N

1. Branch from up-to-date `main`: `issue-N-short-name`. Work in the worktree you were given, or `git worktree add ../owlbum-wt/issue-N -b issue-N-... origin/main` if none.
2. Comment on the issue that work started (one line).
3. Tests first: write failing Rust tests (unit/integration in the crate) and page tests (Vitest + Testing Library against the faked backend) for the behaviour, then implement until they pass.
4. Checks, on Windows from Git Bash (see the speed rules just below: quick gate plus focused tests, not the full gate). **The owner has paused all Linux work (2026-11-26): do not run the gate in WSL, do not test on Linux, and do not spend time on Linux-only problems.** Keep code portable (no needless Windows-only APIs) so Linux can be picked up later; note any known Linux gap on the issue. Ignore the WSL instructions elsewhere in this brief until this note is removed.
   **Speed rules (owner's order, 2026-11-26): do NOT run the full gate.** Before merging run `timeout 900 ./scripts/check.sh --quick` (format, clippy, bindings, page types and lint; about 2 minutes) plus the focused tests for what you changed (`cargo test -p <crate> <module filter>`, `npx vitest run <paths>`). The orchestrator runs the full gate on main every few merges and sends any breakage back. Run each check once; re-run only after you change code, not after a clean rebase. No load loops, repeated runs, benchmarks or extra verification passes unless the issue itself is about performance. Do not over-verify. Aim to finish an issue within about 60–90 minutes of work; if it is much bigger, ship a solid core, file a follow-up issue for the rest, and merge.
   **Before any focused `cargo test`/`cargo build` in a worktree**, mark this checkout's sources newest, or Cargo may quietly run another worktree's build from the shared target: `find crates src-tauri -type f \( -name '*.rs' -o -name '*.sql' -o -name '*.json' -o -name 'Cargo.toml' -o -name 'build.rs' \) -not -path '*/target/*' -not -path '*/gen/*' -print0 | xargs -0 touch` (the same step `check.sh` runs).
   Never wait on an unbounded test run: wrap long runs in `timeout` (e.g. `timeout 1200 ./scripts/check.sh`, `timeout 300 npx vitest run ...`). A run that hits the limit is a hang to debug, not a slow test to wait for.
5. Keep the design docs true: if the build forced a change, edit the doc in the same branch. Add every change a user would notice to the `Unreleased` section of `CHANGELOG.md` (Keep a Changelog: Added, Changed, Fixed, Removed), in plain words. On a rebase conflict there, keep both sides' lines.
6. Push the branch and open a PR with `ghf pr create --repo funvill/Owlbum --base main`. PR body: what changed, how it was tested, decisions; no hard-wrapping (one paragraph per line); `Closes #N`; end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
7. Merge yourself, never with `gh pr merge` (that would record a human identity). Instead:
   ```
   git fetch origin
   git -c user.name="SWS-Chipkin" -c user.email="sws-dev@chipkin.com" rebase origin/main   # on your branch; re-run the gate if main moved
   gitf push --force-with-lease origin <branch>
   git checkout main && git pull --ff-only   (in the main checkout: <checkout>, or `git push origin <branch>:main` directly)
   gitf push origin <branch>:main            # fast-forward only; GitHub marks the PR merged
   ```
   If the fast-forward push is rejected because main moved, rebase again and retry.
8. Post decisions made while building as a comment on the issue (what, why, alternatives). Confirm the issue closed (close it with `ghf issue close N --repo funvill/Owlbum --comment ...` if `Closes` did not).
9. Delete the remote branch after merge (`gitf push origin --delete <branch>`) and remove your worktree.

## Roadblocks

- Something cannot work as specified: comment on the issue with what you tried and what you shipped instead, correct the doc, continue.
- A feature cannot be made reliable on Linux: ship it on Windows only, disable its control on Linux (via `app_capabilities`), file a new issue with labels `after-v1,windows-only` describing the gap, link it from the original issue.
- Blocked on another unmerged issue: say so in your final report; do not stub across a boundary silently.
- Never: publish a release, push a version tag, enable GitHub Pages, create signing keys, spend money, or delete user data.

## Final report to the orchestrator

Under 15 lines: PR URL, merged commit SHA on main, gate result on Windows and Linux (actual commands run), decisions posted, follow-up issues filed, anything left undone.
