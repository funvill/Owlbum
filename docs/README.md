# Owlbum: design documents

Owlbum is a free, open-source desktop program for looking through folders of photos: star the best ones, hide the bad ones, find things by what is in them, make simple edits, and drag photos into other programs. It runs on Windows and Linux.

These documents are the specification for version 1. They say what Owlbum does and how it is built. Where a document and the code disagree after version 1 ships, the code wins and the document gets fixed.

Owlbum is a from-scratch rebuild of an earlier program, Photo Tool. The old program's documents are kept in [`archive/`](archive/README.md) as a record of behaviour that worked. Owlbum shares no code, file formats or data with it, and does not import its files.

## Reading order

| Part | File | What it covers |
|---|---|---|
| 1 | [Architecture](01-architecture.md) | The processes, the Rust crates, the page, how they talk, the network rules, and the repository layout. |
| 2 | [Data](02-data.md) | The album database (`.owlbum/owlbum.sqlite`), its schema and safety rules, and the app data folder (cache, settings, models). |
| 3 | [Media and scanning](03-media-and-scanning.md) | The format registry, decoders, camera RAW, video, the scanner, thumbnails, and folder watching. |
| 4 | [Browsing](04-browsing.md) | The window, start screen, grid, selection, preview, full-screen viewer, details panel, timeline, map, and reopening where you were. |
| 5 | [Organising](05-organising.md) | Star, hide, photo score, undo, sorting, stacks, filters, query words, text search, and tidying. |
| 6 | [Editing and file actions](06-editing-and-file-actions.md) | The edit model (crop, rotate, flip, straighten, light and colour), saving edits, the photo menu, trash, copy, reveal, and dragging photos out. |
| 7 | [Tagging](07-tagging.md) | "What's in it": models, downloads, the tagging queue, descriptions, objects, keywords, and search by meaning. |
| 8 | [Desktop, release and tests](08-desktop-and-release.md) | Menus, settings, keyboard shortcut overlay, updates, About, What's new, logs, packaging, the release workflow, the marketing site, and the test rules. |
| 9 | [Visual design](09-visual-design.md) | Theme tokens, typography, spacing, layering, window sizes, components, accessibility, icons, the logo, and design suggestions awaiting a decision. |
| | [Build process](build-process.md) | How the v1 build was run, where it paused, and how to resume it on this or another computer. |
| | [Roadmap](roadmap.md) | Every version 1 feature as a GitHub issue, grouped into ordered milestones, plus the issues deferred until after version 1. |

## Principles

These shape every part. A feature spec that conflicts with one of them is wrong.

1. **Photos stay on the computer.** Owlbum uses the network for three things only: map tiles and model downloads, each after the user asks, and a daily update check that can be turned off. Sending feedback opens the user's own browser. Version 1 has no telemetry.
2. **Photo files are only read.** The one exception is "Save edit to file", which the user asks for explicitly. Deleting always means the Recycle Bin or Trash, never erasing.
3. **What Owlbum knows about an album lives in the album.** One file, `.owlbum/owlbum.sqlite`, at the root of the folder that was opened. Copy the folder to another computer and the stars, notes, tags and scan results come with it.
4. **App data is a cache.** Thumbnails, downloaded models and settings live in the per-user app data folder. Deleting it loses no information about any photo.
5. **Rust owns the data; the page is a view.** Filtering, sorting, searching and scoring run in Rust against the database. The page asks for the rows it is showing. The target is an album of 100,000 photos that stays quick.
6. **No patent-encumbered decoders are bundled.** Video, HEIC and AVIF go through whatever the operating system provides (on Windows, the Microsoft Store's free extensions).
7. **Failures are messages, never blank windows or silent loss.** A file that cannot be read is counted and listed. A database that cannot be written makes the album view-only and says so.

## Decisions already made

| Topic | Decision |
|---|---|
| Name and licence | Owlbum, MIT. LGPL components (the RAW decoder) are allowed and listed in the third-party notices. |
| Repository | `github.com/funvill/Owlbum`, public. Work happens in public: plans are GitHub issues, and decisions made during the build are recorded as comments on the issue they affect. |
| Platforms | Windows x64 and Linux x64 in version 1. macOS is a later target with no build yet. Mobile is out of scope. |
| Frameworks | Tauri 2, Rust, and a React + TypeScript page. No local HTTP server. |
| Language | English only. All interface text lives in one module so translation is possible later. |
| Formats in version 1 | JPEG, PNG, WebP, GIF, TIFF, BMP, JPEG XL, camera RAW, and video through system codecs. HEIC and AVIF through Windows' codecs where the Store extensions are installed (#70, #71); not on Linux. |
| Deferred to after version 1 | Telemetry, macOS, Windows code signing. Each has an issue. |
| Linux gaps | A feature that cannot be made reliable on Linux may ship on Windows only, with its control disabled on Linux and an issue open. |
| Old program | No import of Photo Tool's `photoinfo.json` or `phototool-tags*.json`. |
| Tests | Rust unit and integration tests, and page tests against a faked backend, run locally on Windows and on Linux (WSL), and on GitHub Actions on Windows for every push to `main` and pull request. |
| Releases | One GitHub Actions workflow builds installers on a version tag and creates a draft release. Publishing is done by a person. |

## Glossary

| Term | Meaning |
|---|---|
| Album | The folder the user opens, with all its subfolders. An album has exactly one database. |
| Album root | The folder that holds `.owlbum/`. Opening a subfolder of an existing album uses the album root's database and shows only that subfolder. |
| Album database | `.owlbum/owlbum.sqlite` in the album root. The only place Owlbum keeps information about photos. |
| App data | The per-user folder for the cache, settings, models and logs. Rebuildable. |
| Photo | Any file Owlbum shows: a still image, a RAW file or a video. |
| Typed info | What a person entered or did: star, hidden, people, date, place, description, notes, score nudge, and the photo's edit. |
| Scan facts | What Owlbum read or measured from the file: size, dimensions, camera details, date taken, GPS, quality measures. |
| Tags | What the models produced: description, objects, keywords, and vectors for search by meaning. |
| File key | A fingerprint of a file's bytes (its size plus a hash of its first and last 16 KiB). It follows a photo through a rename or a move. |
| Thumbnail | A JPEG copy Owlbum makes for display: small (512 px) and medium (1600 px), longest side. |
| Stack | A burst of near-identical shots shown as one tile with a count. |
| Photo score | A number built from focus, exposure, colour, composition, people and size, plus 50 for a star and minus 50 when hidden. Up and Down nudge it. |
| Part (tagging) | One of the two tagging jobs: `describe` (descriptions and objects) or `keywords` (keywords and search by meaning). |
| View-only | The state of an album whose database cannot be written (a read-only folder, or another computer holds the lock). Browsing works; star, notes, edits and tagging are disabled. |
