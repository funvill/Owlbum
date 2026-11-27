# Architecture

## At a glance

```text
┌───────────────────────────── Owlbum (one process, Tauri 2) ─────────────────────────────┐
│                                                                                          │
│  Webview (WebView2 on Windows, WebKitGTK 4.1 on Linux)                                   │
│   React + TypeScript page: grid, preview, viewer, details, filters, map, settings       │
│   It holds view state only: what is selected, what is scrolled into view, what is open.  │
│        │ invoke (commands)        ▲ events             ▲ owlbum:// (images)             │
│        ▼                          │                    │                                 │
│  Shell crate `owlbum` (src-tauri)                                                        │
│   commands, events, the owlbum:// protocol, menus, dialogs, single instance,             │
│   drag-out, trash, clipboard, reveal, updater, logs                                      │
│        │ plain Rust calls                                                                │
│        ▼                                                                                 │
│  Core crate `owlbum-core` (no Tauri dependency)                                          │
│   album database · scanner · media registry · thumbnails · query engine ·                │
│   photo score and stacks · edit model and renderer · tagging queue and models            │
│        │                                         │                                       │
└────────┼─────────────────────────────────────────┼───────────────────────────────────────┘
         ▼                                         ▼
  Album folder                              App data folder (a cache)
   photo files (read only)                   thumbs/  models/  app.sqlite  logs/
   .owlbum/owlbum.sqlite (the truth)
```

There is one operating-system process plus the webview's own helper processes. Long work (scanning, thumbnailing, tagging, rendering edits) runs on Rust worker threads, never on the thread that answers the page.

## Repository layout

```text
Owlbum/
  Cargo.toml              workspace; the one version number
  rust-toolchain.toml     the pinned Rust version, so rustfmt and clippy agree everywhere
  .cargo/config.toml      Cargo settings (build output in target/)
  crates/
    owlbum-core/          all logic; testable without a window; one module per feature area
    owlbum-fixtures/      generates the synthetic test album (no personal photos in the repo)
  src-tauri/              the shell crate `owlbum`: Tauri builder, commands, protocol, plugins
    src/commands/         one module per command prefix, and the registry that collects them
    src/bindings.rs       writes the page's TypeScript types from the registry
    capabilities/         what the window is allowed to call
    whats-new.json        release notes bundled with the program
  ui/                     the React page (Vite, TypeScript, Vitest)
    src/api/              the only module that calls Tauri; everything else imports from here
    src/api/bindings/     generated from Rust and committed: command, event and payload types
    src/api/fake/         the in-memory backend used by page tests, one file per command prefix
    src/features/         one folder per feature area, each with its own zustand store
    src/text/             every interface string shown by the page, one file per area
    src/styles/           tokens.css (every colour, size, duration and layer) and base.css
    src/controls/         the base controls: buttons, inputs, menus, dialogs, toasts, tooltips
    src/icons/            the icon set, inline SVG
    src/keys/keymap.json  the one table of keyboard shortcuts
  e2e/                    the thin WebdriverIO smoke test, run by hand before a release
  scripts/                check.sh (the gate, on Windows under Git Bash and on Linux), bindings.sh, and the notices generator
  site/                   the marketing page (static HTML), deployed to GitHub Pages by a workflow
  docs/                   these documents; docs/archive holds the old program's
  .github/workflows/      checks.yml (the gate on pushes and pull requests), release.yml (installers on a tag), pages.yml (site)
  notices/                hand-written third-party notices for things that are not packages
  LICENSE                 MIT
```

**Interface text has two homes.** Text the page shows lives in `ui/src/text/`. Text that Rust produces (error messages in `AppError`, menu labels, native dialog wording) lives in one `text` module in `owlbum-core`. No sentence for the user is written anywhere else.

## The two Rust crates

**`owlbum-core`** holds everything that can be tested without a window. It has no dependency on Tauri. Its public surface is a small set of types: `Album` (an open album: database, scanner, query engine), `AppData` (cache, settings, models), and plain functions for pure logic (score, stacks, the edit model, query parsing). Every feature's rules are implemented and tested here first.

**`owlbum`** (in `src-tauri/`) is thin. Each Tauri command validates its arguments, calls one core function, and maps the result or error to a serialisable reply. It also owns what only a desktop shell can do: windows, menus, native dialogs, drag-out, trash, clipboard, reveal in file manager, the updater and single-instance handling.

The rule for where code goes: if it could be tested with `cargo test` and a temp folder, it belongs in `owlbum-core`.

**The album index.** An open album keeps a compact in-memory copy of the columns that filters, sorts, scores and stacks need (one small record per photo). It is built when the album opens and patched by the writer thread on every change, including each batch a scan commits. Column filters, facets, sorting and stacking run over this index; full-text search and people and label lookups run in SQL. "Rust owns the data" means both. [Organising](05-organising.md) specifies the index.

**Undo** also lives in Rust: each changing command records an undo step on the open album. The page only asks for `album_undo`.

## The page

The page is React 19 and TypeScript, built with Vite. State is held in one small store (zustand). It shows the photos three ways: grid, timeline and map. The map uses Leaflet with marker clustering.

The page never holds the whole album. It holds:

- the current query (filter, sort, search text, view, stack setting);
- the result's size and a window of rows around what is on screen;
- the selection, as photo ids;
- the detail record of the selected photo;
- interface state (panel widths, open sections, dialogs).

`ui/src/api/` is the only module that imports `@tauri-apps/api`. Page tests replace it with a fake. No component calls `invoke` directly.

## How the page and Rust talk

### Commands

A command is a request with a reply (`invoke`). Names are `snake_case`, grouped by a prefix. Each part's "Commands" section defines its commands in full; this table says which prefix is for what and which part owns it.

| Prefix | Purpose | Owner | Examples |
|---|---|---|---|
| `album_` | Open, close, status, scan, undo, tidy, people list | [Browsing](04-browsing.md) (`album_open`, `album_status`), [Media and scanning](03-media-and-scanning.md), [Organising](05-organising.md) | `album_open`, `album_close`, `album_status`, `album_rescan`, `album_undo`, `album_tidy_list` |
| `query_` | Ask for rows | [Organising](05-organising.md) | `query_run`, `query_window`, `query_locate`, `query_facets`, `query_parse` |
| `photo_` | Read or change one or more photos | [Browsing](04-browsing.md), [Organising](05-organising.md) | `photo_detail`, `photo_set_info`, `photo_nudge`, `photo_histogram` |
| `edit_` | The edit model, rendering and saving | [Editing and file actions](06-editing-and-file-actions.md) | `edit_plan`, `edit_set`, `edit_turn`, `edit_save_to_file`, `edit_save_as` |
| `file_` | File actions | [Editing and file actions](06-editing-and-file-actions.md) | `file_trash`, `file_reveal`, `file_copy_image`, `file_copy_files`, `file_copy_paths`, `file_prepare_drag`, `file_start_drag` |
| `tagging_` | Models and the queue | [Tagging](07-tagging.md) | `tagging_models`, `tagging_download`, `tagging_pause`, `tagging_regenerate` |
| `search_` | Search by meaning | [Tagging](07-tagging.md) | `search_meaning` |
| `app_` | Start-up, settings, recent albums, session, updates, cache | [Desktop, release and tests](08-desktop-and-release.md), [Browsing](04-browsing.md) | `app_startup`, `app_settings_get`, `app_settings_set`, `app_check_updates` |

**Errors.** Every command returns `Result<T, AppError>`. `AppError` serialises as `{ "code": "<stable_code>", "message": "<sentence for the user>", "details"?: { … } }`. The page shows `message` and branches only on `code`; `details` carries structured facts a dialog needs (for example the other computer's name and last-seen time for a stale lock). An `internal` error may carry `details.cause`, the underlying library's own error text, for the log; the page never shows it. Codes are an enum in `owlbum-core`; adding one is a code change, never a string typed at the call site. Codes used by more than one part:

| Code | Meaning |
|---|---|
| `photo_not_found` | The id is not a photo of the open album. |
| `query_expired` | The token was dropped; run the query again. |
| `view_only` | The album cannot be changed right now. |
| `no_album` | No album is open. |
| `bad_query` | The query could not be understood. |
| `scan_running` | The action must wait for the scan. |
| `album_unreachable` | The album folder cannot be reached (an unplugged drive). |
| `not_a_folder`, `folder_unreadable` | The path given to `album_open` cannot be used. |
| `setting_out_of_range` | `app_settings_set` was given a value outside the setting's range. |
| `internal` | Something failed that the user cannot act on (an unexpected failure inside Owlbum or Tauri). The message says what was being done. |

**One source for types.** Commands, events and every type they carry are declared once, in Rust. A command is written with the shell's `command!` macro in the module for its prefix (`src-tauri/src/commands/<prefix>.rs`) and listed in that module's `area!` call, which also lists the events the area sends; an event is a payload type in `owlbum-core` that implements `Event` (its `NAME` is the event name). The shell routes each `invoke` to the module that lists the command, so adding a command never edits a shared list. `scripts/bindings.sh` writes the TypeScript types to `ui/src/api/bindings/` with ts-rs; the page's `invoke` and `listen` and the fake backend are typed from them, and a test fails while they are stale. Arguments are passed by their Rust names in `snake_case`. 64-bit integers are TypeScript `number`s.

**Photo identity.** The page refers to a photo by its integer `id` from the album database, never by a path. Rust resolves ids to paths. A command given an unknown id answers `photo_not_found`. Because the page cannot name a path, it cannot ask Rust to touch a file outside the open album. The only command that takes a path is `album_open`. Rust does send paths to the page for display (the album root, a photo's full path in the details panel); the page never sends one back.

### The query model

Opening a result is two steps, so that 100,000 photos never cross to the page at once.

1. `query_run(query, keep?) → QueryResult`. Rust runs the filter, sort, search and stacking, and keeps the ordered list of ids in memory under a `token`. The result carries the token, the album revision, the counts the interface shows ("Showing X of Y"), the parsed chips, timeline groups when asked for, and where the current photo landed (`keep` names the photo and selection to follow).
2. `query_window(token, offset, limit) → Row[]`. The page asks for the rows it is about to show. A `Row` is small: id, relative path, thumbnail key, aspect ratio, star, hidden, score, stack info, edit plan (or null), kind and duration.

`query_locate(token, id)` finds where a photo sits in a result, for keeping the selection in view. It answers an index, or says the photo is folded inside a stack, or null. `search_meaning(token)` appends photos found by meaning to the end of a token's list and answers `{ count, photos, meaning_start }`. A re-run of the query makes a new token without that segment until `search_meaning` is called again; in between, a current photo that was inside the segment counts as gone.

**One definition.** [Organising](05-organising.md) owns the exact shapes of `Query`, `QueryResult` and `Row`. Other parts add fields there and refer to it; they do not redefine them.

When the album changes, Rust emits `album_changed` with a revision number and a reason, and the page re-runs its query when the reason can affect what it shows. Rust keeps the last 4 tokens and drops older ones; a dropped token answers `query_expired` and the page re-runs.

### Events

An event is pushed from Rust to the page. There is no polling anywhere.

| Event | Payload | When |
|---|---|---|
| `scan_progress` | `{ phase, found, done, failed, finished, cancelled }` | While a scan runs; at most 10 per second. `phase` is `walking`, `reading` or `idle`. |
| `album_changed` | `{ revision, reason, rekeyed? }` | Rows were added, removed or changed. |
| `tagging_progress` | `{ total, done, failed, paused, running, not_working, problem }` | While tagging runs, and on any state change. |
| `download_progress` | `{ model, received, total, error, finished }` | While a model downloads. |
| `edit_save_progress` | `{ done, total, current }` | While Save all runs. `current` is the id of the photo being saved. |
| `open_folder` | `{ path }` | A second launch, the File menu, or a folder dropped on the window asked for a folder. A folder given on the command line at start, or one that arrived before the page called `app_startup`, is returned by `app_startup` instead, so it cannot arrive before the page listens. |
| `folder_drop` | `{ phase }` | Something is dragged over the window (`over`), the drag left or was dropped (`left`), or a drop held nothing that could be opened (`refused`). The page draws the drop outline from it; a drop that opens a folder sends `left`, then `open_folder`. |
| `app_menu` | `{ item }` | A menu item that the page carries out was chosen. |
| `app_closing` | `{}` | The window is about to close; the page finishes what it is doing and answers with `app_close_ready`. |
| `app_fatal` | `{ title, message, advice, details }` | Something failed that the page should show before the program stops. |
| `update_state` | `{ state, version?, percent? }` | The updater's state changed. `state` is `unavailable`, `idle`, `checking`, `available`, `downloading`, `ready` or `installing`. |

`album_changed.reason` is one of:

| Reason | Cause | The page re-runs its query |
|---|---|---|
| `scan` | A scan added, changed or removed rows. Sent in batches while a scan runs. | Yes. |
| `watch` | The folder watcher saw files change. | Yes. |
| `info` | Typed info changed (star, hide, notes, people, date, place, score nudge). | Yes. |
| `edit` | A photo's edit changed. | Only to refresh rows on screen. |
| `saved` | An edit was saved into a file or as a copy. | Yes. |
| `trashed` | Photos were moved to the Recycle Bin or Trash. | Yes. |
| `tidy` | Missing rows were removed. | No. |
| `facts` | Album-wide facts (bursts, quality) were recomputed. | Yes. |
| `tags` | Tagging wrote results. Coalesced to one every 5 seconds. | Only when the query uses search text or a keyword or object filter. |
| `status` | The album's state changed: the lock was taken over, a write the database could not take made the album view-only, or the folder stopped being watched. | No; the page reads `album_status`. |

`rekeyed` lists photo ids whose file content changed, so their thumbnails and tags are stale.

### The `owlbum://` protocol

Images reach the page through a custom URI scheme registered with the asynchronous handler, so decoding never blocks the main thread.

| URL | Returns | Caching |
|---|---|---|
| `owlbum://thumb/<thumb_key>/s` | 512 px JPEG | Immutable; the key changes when the file changes. |
| `owlbum://thumb/<thumb_key>/m` | 1600 px JPEG | Immutable. |
| `owlbum://full/<photo_id>?v=<thumb_key>` | Full-size upright JPEG for zooming, cached on disk | One hour. |
| `owlbum://original/<photo_id>?v=<thumb_key>` | The file's own bytes, for animated GIF and WebP | One hour. |
| `owlbum://video/<photo_id>` | The video file, with range requests | None. |

On Windows the webview reaches a custom scheme as `http://owlbum.localhost/…`; the page builds addresses through one helper in `ui/src/api/` so no component cares.

The handler serves only thumbnails of the open album and photos of the open album. A thumbnail asked for before the scanner has made it is made on demand. There is no address for an edited picture: the page draws edits itself by applying the edit plan to a thumbnail (see [Editing and file actions](06-editing-and-file-actions.md)). Video delivery on Linux has a known Tauri problem with custom schemes; [Media and scanning](03-media-and-scanning.md) covers the fallback.

## Threads

| Work | Where it runs |
|---|---|
| Commands | Tauri's async runtime. A command that touches the database or a file runs on the blocking pool. |
| Scanning and thumbnails | A scan pool sized to the logical processors minus one, at least one. |
| Database writes | One writer thread per open album; everything that writes sends it a message. Reads use a small pool of read-only connections. |
| Tagging | One dedicated thread. Model runs use half the logical processors. A search's text is embedded on the command's own blocking thread, so a search never waits behind a photo being described. |
| Model downloads and hashing | One download thread. |
| Edit rendering | The blocking pool. |

One writer per album means writes never collide inside the program, and "database is locked" can only come from another computer.

## Network rules

Owlbum makes four kinds of network request. Map tiles and model downloads are off until the user asks for them; the update check is on by default and can be turned off; feedback opens the user's own browser.

| Use | Made by | Host | Turned on by |
|---|---|---|---|
| Map tiles | The webview | `tile.openstreetmap.org` | Choosing "Load maps" on the Map tab. |
| Model downloads | Rust | `huggingface.co`, `*.huggingface.co`, `*.hf.co`; HTTPS only, default port, no credentials | Choosing Download in Settings. |
| Update check | Rust (the updater) | `github.com` and its release-asset hosts | On by default, once a day; can be turned off in Help. The first start says so. |
| Feedback | The user's own browser | `github.com` | Choosing Help > Send feedback. |

The webview is restricted by policy:

- A Content-Security-Policy with `default-src 'self'`; images from `owlbum:`, `data:`, `blob:` and the tile host only; media from `owlbum:` only; bundled fonts only; inline styles allowed (tile positions and the map need them); and connections to Tauri's IPC only. The exact string, including the Windows forms of the custom scheme, is in [Desktop, release and tests](08-desktop-and-release.md).
- Navigation away from the app's own page is refused, and new windows are refused.
- The page has no general HTTP capability; it has no way to ask Rust to fetch an arbitrary URL.

This is a policy, not an interception layer. Tauri offers no way to inspect and cancel every webview request, so Owlbum does not claim a proof that nothing else is contacted. A page test asserts that the built page contains no remote URL other than the tile host.

Two more network facts, stated in About and on the site: the Windows installer may download Microsoft's WebView2 runtime if the computer lacks it, and tile requests carry the user agent `Owlbum/<version>` with the platform and the project's address.

## Capabilities

Tauri capabilities grant the one window only the app's own commands and events. **No plugin is granted to the page.** Every plugin (dialog, drag, clipboard, opener, updater) is called from Rust behind an Owlbum command that takes photo ids. In particular a drag is started from Rust, never by the drag plugin's own page command, which takes paths.

## Names and folders

- The installed program's identifier is `com.funvill.owlbum`. A development build uses `com.funvill.owlbum.dev`, so its single-instance lock and data never collide with an installed copy.
- Owlbum computes its own app data path (see [Data](02-data.md#the-app-data-folder)) and does not use Tauri's identifier-named folders. The webview's own data goes in `<app data>/webview`.

## Dependencies chosen

These are the starting choices. A prototype may replace one; the replacement is recorded on the feature's issue.

| Need | Choice | Notes |
|---|---|---|
| Database | `rusqlite` with bundled SQLite, FTS5 enabled | |
| JPEG decode and resize | `zune-jpeg` (through `image`, which uses it as its JPEG decoder), `jpeg-decoder` for a large JPEG's DCT-scaled preview (#114), `fast_image_resize`; `image` for other formats | Thumbnails and the analysis image are resized with `fast_image_resize`'s Lanczos3, about 4 times faster than `image`'s on a 24-megapixel decode (#10). |
| JPEG encode | `jpeg-encoder` | Pure Rust, writes 4:2:0 (which `image`'s encoder cannot); `mozjpeg` was not taken because it needs a C and assembler toolchain on every build machine (#11). |
| EXIF and video metadata | EXIF: Owlbum's own reader (`media::exif`); video containers: Owlbum's own walk of MP4, MOV and AVI headers, and `nom-exif` for dates, places, cameras and Matroska | Read only. Why EXIF is read by hand is in [Media and scanning](03-media-and-scanning.md#metadata-reading). |
| JPEG XL | `jxl-oxide` | |
| Camera RAW | `rawler` 0.8 (LGPL-2.1) | Reads the embedded previews and develops the sensor data (#49). Listed in `notices/rawler.txt` until the notices generator lists crates; `about.toml` must accept LGPL-2.1 for it. |
| Model runtime | `ort` (ONNX Runtime, loaded from Microsoft's library by full path), `tokenizers` | Driven directly, without `usls` (#52). |
| Model downloads | `ureq` (rustls with the webpki roots, redirects handled by hand), `sha2` | `ureq` is the client `ort` already builds with; blocking suits the one download thread. |
| Colour profiles | `moxcms` | Converting embedded profiles to sRGB (`media::colour`). Chosen by measurement in #7 over `lcms2` and `qcms` ([Media and scanning](03-media-and-scanning.md#open-questions), question 8). `image` already depends on it. |
| EXIF writing | none: `owlbum_core::edit::metadata` | Only when an edit is saved into a file or copied: keep the block, reset orientation, set the new size, drop the embedded thumbnail. A small pure-Rust TIFF rewriter, because `little_exif` could not drop the thumbnail (#150). Lossless WebP with its profile, EXIF and XMP is written with `image-webp`, the version `image` already uses. |
| Video poster frames | The `windows` crate (the shell's thumbnails; Media Foundation is a follow-up); `gstreamer` linked to the system libraries on Linux, not built yet | System codecs only. |
| Country outlines | Natural Earth 5.1.2 Admin 0 countries at 1:10m, simplified to about 1 km and bundled (public domain) | For the Country row and country search. `scripts/countries.mjs` downloads the pinned file, checks its SHA-256, simplifies each outline (Douglas-Peucker, 0.01°) and writes `crates/owlbum-core/data/countries.bin` (258 countries, 746 KB), read by `owlbum_core::info::country`. 1:10m simplified was chosen over the low-resolution scales by measurement (#25): of points within 11 km of a border or coast, 1:110m put 28% in the wrong country or none and 1:50m 11%, against 2% here; 1:110m also lacks 81 small countries and territories. |
| Local dates and times | `chrono` (clock only) | Backup file names and the time shown for a stale lock. |
| Logging | `tracing`, with Owlbum's own small subscriber writing the file (`owlbum_core::appdata::log`) | `tracing-subscriber` and `tracing-appender` were not needed: the line format, the one rotation at open and the temp-folder fallback are a few dozen lines. |
| Licence notices | `cargo-about` plus a small script for npm packages | |
| Folder watching | `notify` | |
| Trash | `trash` | Called through `owlbum_core::files::no_bin_reason` first: on Windows only a local fixed drive is sent to the Recycle Bin (#39). |
| Drag-out | `drag`, the library under `tauri-plugin-drag`, called from the shell | The plugin's own command takes paths, so it is not used. Windows only for now; the Linux prototype waits for Linux work to resume (#42). |
| Clipboard | `arboard` for text and one image; `clipboard-win` for a list of files on Windows | `arboard` is what `tauri-plugin-clipboard-manager` uses; called directly, since no clipboard command is granted to the page. The shell keeps one `arboard::Clipboard` for the program's life, as its Linux backend needs (#40, #41). |
| Reveal in file manager | `tauri-plugin-opener` | Its `reveal_item_in_dir` function only; the plugin is not registered. |
| Updates | `tauri-plugin-updater` | Needs a signing key, created at final review. |
| Single instance | `tauri-plugin-single-instance` | |
| Native dialogs | `tauri-plugin-dialog` | |
| Page | React, TypeScript, Vite, zustand, Leaflet, `leaflet.markercluster`, Vitest, Testing Library, ESLint | |
| Smoke test | WebdriverIO with `tauri-driver` | Run by hand before a release. |

## What is deliberately absent

- No local HTTP server and no open port.
- No telemetry and no crash reporter in version 1.
- No plug-ins loaded at run time. "Format plug-in" means a module compiled into the program.
- No second program sharing data with Owlbum.
