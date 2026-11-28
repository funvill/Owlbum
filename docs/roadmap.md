# Roadmap

Every version 1 feature is one GitHub issue. Issues are grouped into milestones, and milestones are built in order. Inside a milestone, issues are built in the order listed unless one is blocked.

Each issue links to the section of the design document that specifies it. An issue is done when its behaviour matches the spec, it has Rust tests and page tests written first, and the tests pass on Windows and on Linux (while Linux is paused by the owner, on Windows only). A decision made while building (a prototype that failed, a spec that was wrong) is recorded as a comment on the issue, and the design document is corrected in the same pull request.

Titles below are the issue titles. The numbers are a working order, not GitHub issue numbers.

## Status (2026-11-27)

Every issue in milestones 0 to 6 is closed except #69, the final pass. The owner's feedback on test builds 0.1.1 to 0.1.3 was built too (#165 to #169, #182 to #185), and test builds up to 0.1.5 are recorded in `CHANGELOG.md`. Version 1 is built and tested on Windows only: Linux work is paused by the owner.

The final pass (#69) checked every design document against the program on Windows and corrected the documents. What it found not built is marked in the document where the spec says it, with its issue:

| Issue | Gap | Label |
|---|---|---|
| #199 | Quit: exit 5 seconds after a stalled album close | v1 |
| #200 | Edit: block edit mode while the photo is being saved | after-v1 |
| #201 | Narrow window: the thumbnail-size slider's popover | after-v1 |
| #202 | Log the ONNX Runtime version and show it in About | after-v1 |
| #203 | Keep the webview's data in `<app data>/webview` | after-v1 |
| #204 | Smoke test with `tauri-driver` | after-v1 |
| #205 | Check the photo score thresholds on the `trip` fixture | after-v1 |

Still open for version 1:

- **#69, the rest of the final pass:** the owner's test pass in a real window (the checklist comment on #69 lists what only the running app can show), and the whole Linux half.
- **#199** (above).
- **Linux** (paused by the owner): the Linux gate and CI job, Linux packaging and the release job (#181), and the Linux halves of copy and drag (#147) and video poster frames (#156).

## Milestone 0: Foundation

Nothing visible to a user yet. After this milestone the program opens a folder, scans it into the album database and shows raw rows.

| # | Issue | Spec |
|---|---|---|
| 1 | Project scaffold: Cargo workspace, Tauri shell, React page, lint, test runners, licence | [Architecture](01-architecture.md) |
| 2 | Fixture generator: the synthetic `trip` and `large-N` albums | [Data](02-data.md#test-fixtures) |
| 3 | Album database: schema, migrations, album root discovery, writer thread | [Data](02-data.md#the-album-database) |
| 4 | Album safety: lock file, view-only, daily backup, damage check, conflict-copy warning | [Data](02-data.md#opening-locking-and-view-only) |
| 5 | App data: `app.sqlite`, settings, recent albums, logs | [Data](02-data.md#the-app-data-folder) |
| 6 | Media format registry and still-image decoders (JPEG, PNG, WebP, GIF, TIFF, BMP, JPEG XL) | [Media and scanning](03-media-and-scanning.md#the-media-format-registry) |
| 7 | Colour profiles: every decoder converts an embedded profile to sRGB (library chosen by measurement) | [Media and scanning](03-media-and-scanning.md#media-handlers), [Architecture](01-architecture.md#dependencies-chosen) |
| 8 | Scanner: walker, change detection, file keys, moves and missing files, progress, cancel | [Media and scanning](03-media-and-scanning.md#the-scanner), [Data](02-data.md#identity-renames-moves-and-missing-files) |
| 9 | Walker rules for what is left out: cloud-only (online-only) files are never read, links and system folders are skipped, nested album roots, and the scan report commands | [Media and scanning](03-media-and-scanning.md#the-walker) |
| 10 | Metadata and image analysis | [Media and scanning](03-media-and-scanning.md#metadata-reading) |
| 11 | Thumbnails, the size-limited cache, and the `owlbum://` protocol | [Media and scanning](03-media-and-scanning.md#thumbnails-and-the-zoom-copy), [Data](02-data.md#thumbnail-keys-and-the-cache) |
| 12 | The album index: the in-memory copy that filters, sorts, scores and stacks read | [Organising](05-organising.md#the-album-index), [Architecture](01-architecture.md#the-two-rust-crates) |
| 13 | Query engine: `query_run` with `keep`, `query_window`, `query_locate`, tokens, the `album_changed` event | [Organising](05-organising.md#the-query), [Architecture](01-architecture.md#the-query-model) |
| 14 | Folder watching and Rescan | [Media and scanning](03-media-and-scanning.md#folder-watching) |

## Milestone 1: Browse

After this milestone Owlbum is a usable photo viewer.

| # | Issue | Spec |
|---|---|---|
| 15 | Design tokens, base controls and the icon set | [Visual design](09-visual-design.md) |
| 16 | Window shell: top bar (album root and scope), start screen, recent albums, opening a folder, album banners | [Browsing](04-browsing.md#whole-window) |
| 17 | The key table (`ui/src/keys/keymap.json`) and key matching, read by the page's handlers and the shell's menu | [Desktop, release and tests](08-desktop-and-release.md#the-key-table) |
| 18 | The grid: windowed virtual scrolling, tiles, badges, thumbnail size | [Browsing](04-browsing.md) |
| 19 | Selection model and navigation keys, Ctrl+A and Ctrl+D | [Browsing](04-browsing.md#selection) |
| 20 | Preview pane: zoom, pan, mini map, full-size swap | [Browsing](04-browsing.md) |
| 21 | Instant stepping: the preload window and decoded-image cache | [Browsing](04-browsing.md) |
| 22 | Full-screen viewer, with zoom and edits shown | [Browsing](04-browsing.md) |
| 23 | Details panel: photo facts, camera, histogram, sections | [Browsing](04-browsing.md) |
| 24 | Typed info: description, notes, people (with suggestions), date, place | [Browsing](04-browsing.md#how-typed-fields-save), [Organising](05-organising.md#people-suggestions) |
| 25 | Country outlines: the Country row and the country search, from bundled Natural Earth data | [Browsing](04-browsing.md#how-typed-fields-save), [Architecture](01-architecture.md#dependencies-chosen) |
| 26 | Large-album check: 100,000 photos open, scroll, filter and step within the targets | [Browsing](04-browsing.md#tests), [Organising](05-organising.md#performance-targets) |
| 106 | Show photos as soon as possible when opening a new album: rows from the facts pass at once, the page's thumbnails ahead of the scan, the grid keeping its place (absorbs #91) | [Media and scanning](03-media-and-scanning.md#pixels-pass), [Browsing](04-browsing.md#when-the-result-changes) |

## Milestone 2: Organise

| # | Issue | Spec |
|---|---|---|
| 27 | Star and hide | [Organising](05-organising.md) |
| 28 | Photo score, breakdown and nudges | [Organising](05-organising.md) |
| 29 | Sorting | [Organising](05-organising.md) |
| 30 | Bursts and stacks | [Organising](05-organising.md) |
| 31 | Filters: panel, chips, counts, "Showing X of Y" | [Organising](05-organising.md) |
| 32 | Query words and text search | [Organising](05-organising.md) |
| 33 | Undo | [Organising](05-organising.md) |
| 34 | Tidy: remove info for missing photos | [Organising](05-organising.md) |
| 35 | Timeline view | [Browsing](04-browsing.md#timeline-view) |
| 36 | Map view | [Browsing](04-browsing.md#map-view) |
| 37 | Reopen where I was | [Browsing](04-browsing.md#reopening-where-i-was) |

## Milestone 3: Edit and act

| # | Issue | Spec |
|---|---|---|
| 38 | Photo menu, Show in folder, Copy path | [Editing and file actions](06-editing-and-file-actions.md) |
| 39 | Move to Recycle Bin or Trash, and the Delete key | [Editing and file actions](06-editing-and-file-actions.md) |
| 40 | Copy image and copy files (Ctrl+C) | [Editing and file actions](06-editing-and-file-actions.md#copy-image-one-photo) |
| 41 | Copy files: clipboard platform code for a list of files (Windows drop list; Linux formats, prototype on X11 and Wayland first), and `app_capabilities` | [Editing and file actions](06-editing-and-file-actions.md#copy-several-photos) |
| 42 | Drag photos out (prototype on Linux first) | [Editing and file actions](06-editing-and-file-actions.md) |
| 43 | Edit model and renderer: rotate, flip, straighten, crop | [Editing and file actions](06-editing-and-file-actions.md) |
| 44 | Edit mode: crop and straighten interface, edits shown everywhere | [Editing and file actions](06-editing-and-file-actions.md) |
| 45 | Rotate and flip outside edit mode | [Editing and file actions](06-editing-and-file-actions.md) |
| 46 | Light and colour, Auto, and before/after | [Editing and file actions](06-editing-and-file-actions.md) |
| 47 | Saving edits: the `edit_target` setting, Save, Save as, Save all, Revert, the save record and its recovery, the close question in `file` mode | [Editing and file actions](06-editing-and-file-actions.md#saving) |

## Milestone 4: More formats

| # | Issue | Spec |
|---|---|---|
| 48 | Animated GIF and WebP: frame 1 for thumbnails, the badge, and playing the original in the preview and viewer | [Media and scanning](03-media-and-scanning.md#animated-pictures) |
| 49 | Camera RAW (prototype first: DNG and fast previews) | [Media and scanning](03-media-and-scanning.md#the-raw-handler) |
| 50 | Video: thumbnails, metadata and playback through system codecs (prototype first) | [Media and scanning](03-media-and-scanning.md#the-video-handler), [Browsing](04-browsing.md#video) |
| 51 | Files-not-shown notice and dialog: unsupported files (HEIC, AVIF) and files that could not be read | [Browsing](04-browsing.md#notices), [Media and scanning](03-media-and-scanning.md#files-recognised-but-not-supported-yet) |

## Milestone 5: What's in it

| # | Issue | Spec |
|---|---|---|
| 52 | Tagging prototype: Florence-2 and CLIP on ONNX Runtime in Rust, on both systems | [Tagging](07-tagging.md) |
| 53 | Model catalogue, download, verification and removal | [Tagging](07-tagging.md) |
| 54 | Tagging runtime and queue, with pause, retry and progress | [Tagging](07-tagging.md) |
| 55 | "What's in it" panel, tag chips as filters, Regenerate | [Tagging](07-tagging.md) |
| 56 | Search by meaning: `search_meaning(token)` and the meaning segment | [Tagging](07-tagging.md#search-by-meaning), [Organising](05-organising.md#how-it-combines) |
| 57 | Settings: photo tagging | [Tagging](07-tagging.md) |

## Milestone 6: Ship

| # | Issue | Spec |
|---|---|---|
| 58 | Menus, single instance, command line, window geometry | [Desktop, release and tests](08-desktop-and-release.md) |
| 59 | Settings dialog | [Desktop, release and tests](08-desktop-and-release.md) |
| 60 | Keyboard shortcut overlay, drawn from the key table | [Desktop, release and tests](08-desktop-and-release.md#the-overlay) |
| 61 | About, feedback, failure dialogs and logs | [Desktop, release and tests](08-desktop-and-release.md) |
| 62 | Update check and install | [Desktop, release and tests](08-desktop-and-release.md) |
| 63 | What's new after an update | [Desktop, release and tests](08-desktop-and-release.md) |
| 64 | Network policy: Content-Security-Policy, navigation blocking, capabilities | [Desktop, release and tests](08-desktop-and-release.md#network-policy), [Architecture](01-architecture.md#network-rules) |
| 65 | Accessibility baseline: roles and names, the focus ring, F6 regions, and the automated check | [Visual design](09-visual-design.md#accessibility-baseline) |
| 66 | Packaging: installers, icons, licence and third-party notices | [Desktop, release and tests](08-desktop-and-release.md) |
| 67 | Release workflow: installers and a draft release on a version tag | [Desktop, release and tests](08-desktop-and-release.md) |
| 68 | Marketing site and the Pages workflow | [Desktop, release and tests](08-desktop-and-release.md) |
| 69 | Final pass: every spec checked against the program on Windows and Linux; documents corrected | all |

## After version 1

These are filed as issues with the label `after-v1` and are not built now.

| Issue | Why it waits |
|---|---|
| HEIC and HEIF photos | Needs a decoder Owlbum will not bundle; to be done through system codecs. |
| AVIF photos | Decoding needs a C library that complicates the Windows build. |
| Telemetry | Deferred by decision. The issue records the open choice between asking first and opt-out, and the candidate service. |
| macOS build | A later platform. |
| Windows code signing | Costs money each year; the installer is unsigned for now. |
| Light theme | The tokens allow it; version 1 ships dark only. |
| Translations | Version 1 is English only; the text module allows it. |
| Any feature shipped on Windows only | One issue per feature that could not be made reliable on Linux. |

## What needs a person

These are not done by the unattended build and wait for the owner:

- Creating the updater signing key, adding it and its password to the repository's secrets, and putting the public key in `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` (see [Desktop, release and tests](08-desktop-and-release.md)). Until then no build can update itself and the release workflow stops.
- Pushing the first version tag and publishing the draft release the workflow makes.
- The test pass in a real window: the checklist comment on #69.
- Resuming Linux work.

Done by the owner: GitHub Pages was switched on on 2026-11-28 (the site is published by the hand-run `pages.yml`).
