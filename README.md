# Owlbum

A free, open-source desktop program for looking through folders of photos: star the best ones, hide the bad ones, find things by what is in them, make simple edits, and drag photos into other programs. It runs on Windows and Linux.

Owlbum is being built. The specification and the roadmap are in [`docs/`](docs/README.md).

## Building

You need Rust (the version in `rust-toolchain.toml` is installed for you by rustup), Node 24 (`ui/.nvmrc`), and on Linux the [Tauri build packages](https://v2.tauri.app/start/prerequisites/) for WebKitGTK 4.1.

```sh
npm --prefix ui ci
npm --prefix ui run app:dev     # run the development build (com.funvill.owlbum.dev)
./scripts/check.sh              # everything that must pass before a change is merged
./scripts/bindings.sh           # rewrite ui/src/api/bindings after changing a command
```

On Windows, run the scripts from Git Bash.

## Layout

| Folder | What |
|---|---|
| `crates/owlbum-core` | All logic, testable without a window. No Tauri dependency. |
| `crates/owlbum-fixtures` | Generates the synthetic test albums. |
| `src-tauri` | The desktop shell (crate `owlbum`): commands, events, protocol. |
| `ui` | The page: React, TypeScript, Vite, zustand, Vitest. |
| `scripts` | The check and generator scripts. |
| `docs` | The design documents. |

## Licence

MIT. See [LICENSE](LICENSE).
