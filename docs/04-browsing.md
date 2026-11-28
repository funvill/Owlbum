# Browsing

This part covers everything a person does to look through an album: the window, the start screen, opening a folder, the grid, the timeline, the map, selection, the preview pane, the full-screen viewer, the details panel, and reopening where they were.

It builds on [Architecture](01-architecture.md) and [Data](02-data.md). Three rules from those parts shape every section here:

- The page never holds the whole album. It holds the size of the current result and a window of rows around what is on screen.
- A photo is named by its integer `id`. The page never sees or sends a photo's path, except to show it as text.
- Every preference is stored through `app_settings_get` and `app_settings_set`. Nothing is kept in `localStorage`.

Other parts own behaviour that appears on these screens. This part says where their controls sit and refers to them:

| Topic | Owner |
|---|---|
| Star, hide, photo score, undo, sort, stacks, filters, search, Tidy | [Organising](05-organising.md) |
| Edit mode, saving edits, the photo menu, trash, copy, drag-out | [Editing and file actions](06-editing-and-file-actions.md) |
| The "What's in it" section and the tagging bar | [Tagging](07-tagging.md) |
| Menus, Settings, the shortcut overlay, About, What's new | [Desktop, release and tests](08-desktop-and-release.md) |
| Colours, fonts, spacing tokens, icons | [Visual design](09-visual-design.md) |
| Decoding, thumbnails, video delivery, the scanner | [Media and scanning](03-media-and-scanning.md) |

## Words used here

| Word | Meaning |
|---|---|
| Result | The ordered list of photos that the current query produced. Rust holds it under a `token`. The page knows its `count`. |
| Row | The small record the page holds for one place in the result. `Row`, `Query` and `QueryResult` are defined once, in [Organising](05-organising.md#the-query); this part refers to them and does not redefine them. |
| Index | A photo's place in the result, from 0. |
| Current photo | The one photo the preview and the details panel show. Held as `{ id, index }`. |
| Selection | Every selected photo, as a set of ids. It always contains the current photo when there is one. |
| Anchor | Where a Shift range starts. Held as `{ id, index }`. |
| Tile | One square in the grid or timeline. A tile shows one photo, or the face of a stack. |

## Whole window

The window is one column. Parts appear top to bottom in this order:

| # | Part | Shown when |
|---|---|---|
| 1 | Top bar | Always. |
| 2 | Album banners | An album is open and has something to say (view-only with its reason, a damaged database, a sync-conflict copy). |
| 3 | Reading bar | A scan is running. |
| 4 | Tagging bar | Tagging is running, paused part-way, or stopped with a problem. |
| 5 | Notices | The scan left files out ("not supported yet", "could not be read"). |
| 6 | Filter bar and filtered row | An album is open and holds at least one photo. Defined in [Organising](05-organising.md). |
| 7 | Body | Always. One of the states in the table below. |
| 8 | Overlays | Viewer, dialogs, photo menu, toasts. |

```text
+--------------------------------------------------------------------------------------------+
| TOP BAR (48px)        window title: Owlbum 0.1.3 - D:\Photos\2024\Trip                     |
| Owlbum [Search photos…      ] Show whole album [View only]  [grid ===o===] [Tidy…]         |
|  Try star:yes …                                   [Close folder] [folder Open folder] [gear] |
+--------------------------------------------------------------------------------------------+
| BANNER (only when needed)  View only: this album is open on STUDIO-PC. Stars, notes, …     |
+--------------------------------------------------------------------------------------------+
| READING BAR (only while scanning)  [=======-------] Reading photos 120 / 400 · about 2 …   |
+--------------------------------------------------------------------------------------------+
| TAGGING BAR (see 07)               [===----] Tagging 120 of 2,140              [Pause]     |
+--------------------------------------------------------------------------------------------+
| NOTICES (only when needed)  3 files not supported yet (HEIC, AVIF) · 2 files could not … [x]|
+--------------------------------------------------------------------------------------------+
| FILTER BAR (see 05)                               [x] Stack similar [Filters v] 2,140 photos |
+--------------------------------------------------------------------------------------------+
| FILTERED ROW (see 05) [Starred x] [lake x]            Showing 312 of 2,140 · [Clear filter] |
+--------------------------------------------------------------------------------------------+
| BODY (fills the rest): start screen | message | review screen                              |
|                                                                                            |
+--------------------------------------------------------------------------------------------+
                                                                 toasts: fixed bottom centre
```

### Body states

The body is chosen from two numbers: whether a scan is running, and `album_photos`, the number of photos the album (or scope) holds so far, hidden ones included. The rule that matters: "No photos match the filter" is shown only when the album is known to hold photos.

`album_photos` starts from `AlbumStatus.photos` in the `album_open` answer. After that, every `query_run` answer carries the newest value. During a scan the query is re-run on `album_changed` about once a second, so the number, and the tiles, keep up with the scan. The review screen therefore appears as soon as the first photos have been read, long before the scan finishes. An album that was scanned before shows its photos at once, from the database.

| State | Body shows |
|---|---|
| No album open | The start screen. |
| An album is opening (the `album_open` call has not answered) | "Opening <folder name>…", muted, centred. |
| Album open, `album_photos = 0`, scan running | "Looking for photos…", muted, centred. The filter bar is not shown. |
| Album open, `album_photos = 0`, scan finished | "No photos in this folder." The notices still show, so a folder of only HEIC files explains itself. |
| Album open, `album_photos > 0` | The review screen, whether or not the scan is still running. |

Inside the review screen, the thumbnail column has its own empty state: when `album_photos > 0` and the result's `count` is 0, it shows "No photos match the filter" (muted) and a **Clear filter** link. It never shows that text while `album_photos` is 0.

### Counting photos

| Text | Number |
|---|---|
| "N photos" in the filter bar | `total`: photos in the album (or the scope) that are not missing and could be read, **leaving out hidden photos unless the filter shows hidden ones**. |
| "Showing X of Y" | X is the number of photos that pass the filter, with stacks unfolded. Y is `total`, counted the same way. |

A stack counts as all its members, never as one. "1 photo" is singular. Numbers are written with thousands separators ("2,140").

### Layers

The stacking order is fixed. Each row covers every row above it in this table. The tokens and their values are defined in [Visual design](09-visual-design.md#layering); no component writes a number.

| Layer | Token |
|---|---|
| Review screen content, including the whole map stage | `--z-base` |
| Sticky timeline heading, month rail, score toast (see 05) | `--z-raised` |
| Top bar, filter bar, banners, reading and tagging bars | `--z-bar` |
| Pop-ups anchored to a control (score breakdown, country matches), Filters panel (see 05) | `--z-popover` |
| Full-screen viewer | `--z-viewer` |
| Dialog backdrop and dialogs | `--z-dialog` |
| Photo menu | `--z-menu` |
| Toasts | `--z-toast` |
| Tooltips | `--z-tooltip` |

The map stage has `isolation: isolate` and sits at `--z-base`. The map library's own layers and controls (which use values up to 1000) are therefore ordered only among themselves. The viewer, dialogs, the photo menu and toasts always cover the map. A page test checks this.

## Start screen

Shown while no album is open.

```text
+--------------------------------------------------------------------------------------------+
| Owlbum                                                                            [gear]   |  48px top bar: name and Settings only
+--------------------------------------------------------------------------------------------+
|        (padding 48px top; content column 720px wide, centred; gap `--space-5`; scrolls)           |
|        Open a folder of photos                                       (h1)                  |
|        Look through your photos, star the best ones and drag them into other programs.     |
|        Nothing leaves this computer.                                 (max-width 560px)     |
|        [folder Open folder]                                                                |
|                                                                                            |
|        Recent folders                                                (h2)                  |
|        +--------------------------------------------------------------------------+        |
|        | [folder] D:\Photos\Trip 2024                                        [x]  |        |  one button per folder, full width
|        +--------------------------------------------------------------------------+        |
|        | [folder] E:\Old drive\Album (greyed)   Not found — is the drive connected? [x] |   |  disabled, opacity 0.6
|        +--------------------------------------------------------------------------+        |
+--------------------------------------------------------------------------------------------+
```

- **Open folder** is the primary button. It opens the native folder dialog (see [Opening a folder](#opening-a-folder)).
- **Recent folders** appears only when the list is not empty. It holds up to 12 folders, newest first. Each entry's text is the full path, cut with an ellipsis in the middle when it does not fit, with the full path as its tooltip.
- An entry whose folder was not found is disabled, shows "Not found — is the drive connected?", and has that as its tooltip.
- Each entry has a small **x** button, labelled "Remove from this list". It removes the entry from the recent list and touches nothing else.
- There is no "Include subfolders" choice. An album is always a folder with all its subfolders.
- The whole start screen is a drop target for a folder (see below).

## Opening a folder

Every route ends in the same call, `album_open(path)`.

| Route | How it reaches the page |
|---|---|
| **Open folder** button (start screen and top bar) | The page calls `app_pick_folder`. Rust shows the native dialog titled "Open a folder of photos" and answers with the chosen path, or null if it was cancelled. |
| File > Open folder… (Ctrl+O) | Rust shows the same dialog and emits `open_folder`. |
| A recent folder | The page calls `album_open` with the entry's `root`. |
| A folder on the command line | `app_startup` answers with `launch_folder`. |
| A second launch with a folder | The running window comes to the front and Rust emits `open_folder`. A second launch with no folder only brings the window forward. |
| A folder dragged onto the window | Rust receives the drop and emits `open_folder`. |

Rules:

- **A path to a file** (command line, second launch or drop) opens the folder that holds the file.
- **A drop of several items** opens the first folder among them. A drop that holds nothing openable shows the toast "Drop a folder to open it."
- **While a drag is over the window**, the body shows a dashed outline in `--accent` and the text "Drop to open this folder".
- **Opening while an album is open** closes the open album first. No question is asked, because every edit is already in the album: an open edit mode ends as Done, and a running save finishes its current photo first (see [Editing and file actions](06-editing-and-file-actions.md#closing-and-quitting)).
- **Opening the folder that is already open** does nothing except bring the window forward. The page checks this on each route (the dialog, a recent entry, a drop, `open_folder`) against `AlbumStatus.folder`; `album_open` itself always opens again, which is how **Try again**, **Open for changes** and **Show whole album** work.
- **A new album** (no `.owlbum` found) is created without asking. The scan starts at once.

`album_open` can answer with an error that needs a decision. The page shows the error's `message` in a dialog, fills the names in from the error's `details` object, and calls again with an option.

| Error code | Dialog | Buttons |
|---|---|---|
| `album_lock_stale` (`details: { host, last_seen }`) | "This album looks open on <host>, last seen <time>." | Exactly the two buttons [Data](02-data.md#opening-locking-and-view-only) names. **Open anyway** replaces the lock and calls `album_open(path, { take_lock: true })`. **Open view-only** calls `album_open(path, { view_only: true })`; it has the focus, and Esc and a click on the backdrop choose it, because it is the safe answer. |
| `album_damaged` (`details: { backup_date }`, null when there is no backup) | "This album's database is damaged. Owlbum can restore the backup from <date>. The damaged file is kept beside it." When there is no backup: "This album's database is damaged and there is no backup. You can still look through the photos." | **Restore backup** (primary; absent when there is none) calls `album_open(path, { restore_backup: true })`. **Open view-only**. **Cancel**. |
| `not_a_folder`, `folder_unreadable` | Toast: "Couldn't open <folder name>: <message>" | None. The start screen stays. |
| Any other code | Toast: "Couldn't open <folder name>: <message>" | None. |

**Close folder** (top bar, and File > Close folder, Ctrl+W) calls `album_close` and shows the start screen.

### Opening a picture

Owner's feedback on 0.1.3 (#184): Owlbum can be the program that opens pictures (see [Desktop and release](08-desktop-and-release.md#opening-pictures)). A launch that names a picture file, from the file manager or the command line, opens the folder that holds it (or the album that holds that folder, scoped to it, as any open does) with that picture current, the only one selected and in view, so the preview shows it. A second launch hands the picture to the running window the same way; when its folder is already open only the current photo changes.

- The page asks `photo_find(path)` for the picture's id. Found, the album opens in the grid with that photo current: the saved session's view, filters and search are not used for this open, so nothing can hide it.
- Not found yet (a folder opened for the first time, whose scan has not reached it): the album opens as usual and the page asks again each time the status is read during the scan; when the picture is found and the grid holds it, it becomes current. If the scan ends without it, the page stops asking; a picture found that the grid does not hold yet is tried up to 5 more times, a second apart.
- The picture reaches the page as `launch_photo` in the `app_startup` answer, or as the `photo` of an `open_folder` event.
- A picture inside a closed stack cannot be shown as a tile; the album opens without moving to it.

## Top bar

The bar is 48px high. It has a left group that takes the free space and a right group.

**Left group**

- **Owlbum**, in bold.
- **The search box** with its hint line, while an album with photos is open (the same condition as the filter bar). It sits at the left, in line with Open folder and Close folder at the right, and is a `search` landmark labelled "Photo search". Defined in [Organising](05-organising.md#the-search-box). The album's folder is not shown in the bar: it is in the window title (owner's feedback, #182; see [Desktop](08-desktop-and-release.md#the-window)).
- **Show whole album**, a small link shown only in a scoped open. It opens the album root.
- **View only**, a badge shown while the album is view-only. Its tooltip is the banner's sentence.

**Right group**

- **Save all (N)** (album open, `edit_target` is `file` and a photo has an edit): defined in [Editing and file actions](06-editing-and-file-actions.md).
- **Thumbnail size** (album open): a grid icon and a slider, 120px wide, minimum 96, maximum 320, step 8. Label and tooltip "Thumbnail size". See [Thumbnail size](#thumbnail-size).
- **Tidy…** (album open): defined in [Organising](05-organising.md). Disabled in a view-only album.
- **Close folder** (album open).
- **Open folder** (album open; on the start screen the button is in the body instead).
- **Settings** (always): a gear icon button, label and tooltip "Settings". Defined in [Desktop](08-desktop-and-release.md).

The bar never wraps. The window cannot be narrower than 960px, and below 1280px the bar gives up room in the order [Visual design](09-visual-design.md#the-bars) sets: the search box shrinks, then below 1100px the buttons become icon buttons, the slider moves into a popover and the search box's hint row is hidden. (The slider's popover is not built yet: the slider stays in the bar; see #69 final pass.)

## Album banners

A banner is a full-width strip under the top bar. Its look (fill, icon, text size and padding) is the Banner component in [Visual design](09-visual-design.md#banners): the warning tone for view-only and for the conflict copy, the error tone for a damaged database. Several banners stack, one per line. A view-only banner cannot be dismissed. The conflict banner has an **x** that hides it until the album is next opened. A scoped album is not a banner (see below).

`AlbumStatus.view_only` is null or one of the five reasons [Data](02-data.md#opening-locking-and-view-only) lists: `read_only`, `locked`, `chosen`, `newer`, `damaged`.

| State (from `AlbumStatus`) | Exact text | Extra control |
|---|---|---|
| `view_only = "locked"` | "View only: this album is open on <host>. Stars, notes, edits and tagging are off until it is closed there." | **Try again** re-opens the album. |
| `view_only = "read_only"` | "View only: this folder can't be written to. Stars, notes, edits and tagging are off." | None. |
| `view_only = "read_only"` and `in_memory` is true (no database and the folder cannot be written) | "View only: this folder can't be written to, so nothing Owlbum learns about these photos is kept." | None. |
| `view_only = "newer"` | "This album was last used by a newer Owlbum. It is open view-only." | **Check for updates** (see 08). |
| `view_only = "damaged"` | "View only: this album's database is damaged." | **Restore backup…** opens the `album_damaged` dialog above. |
| `view_only = "chosen"` (the user picked Open view-only at a stale lock) | "View only: this album may be open on <host>." | **Open for changes** calls `album_open(path, { take_lock: true })`. |
| `view_only = "chosen"` and `lock_host` is null (Open view-only with no old lock) | "View only: you opened this album view-only." | **Open for changes** calls `album_open(path)`. |
| `conflict_file` is set | "Another computer changed this album at the same time. Owlbum is using its own copy and has not merged "<file name>"." | **x** |

**What view-only disables.** Browsing, filtering, the viewer, the map, copy and drag-out work. These controls are disabled, each with a tooltip that says why, which is the first sentence of the banner in force (for example "View only: this folder can't be written to."): the star button on tiles, every typed field in the details panel, people chips' remove buttons, "Use the camera's date", Edit, Tidy, and the tagging controls. The keys S, F, H, E, `[`, `]`, Up, Down, Delete and Ctrl+Z do nothing and show the toast "This album is view-only." at most once every 4 seconds. (The keys do nothing, but the toast is not built yet: see #69 final pass.)

**Scoped open.** A scoped open has no banner. It is shown by the scope folder in the window title and the **Show whole album** link in the top bar. Counts, the result, the map and the recent list all refer to the scope only.

## Reading bar

Shown from `album_open` until a `scan_progress` event arrives with `finished: true`. It is a `role=progressbar` labelled "Reading photos": a bar up to 420px wide and 6px high, filled with `--accent` in proportion `done / found`, then a label.

| Condition | Label |
|---|---|
| `phase` is `walking`, or `found = 0` | "Looking for photos…" |
| `phase` is `reading` | "Reading photos {done} / {found}", then " · {estimate}" when there is one |

`found` can grow while the walk is still running, so the bar may step back. That is expected.

**Estimate.** Unchanged files need no reading and are counted in `done` at once, so they must not count as work done. Let `start` be the value of `done` on the first event whose `phase` is `reading`, and `read = done − start`. There is no estimate until `read` is at least 5 and 2 seconds of reading have passed. After that it is `elapsed reading time / read × (found − done)` seconds, written as:

| Remaining | Text |
|---|---|
| Under 60 s | "less than a minute left" |
| 90 minutes or more | "about H hour(s) M minute(s) left" |
| Otherwise | "about N minute(s) left", minutes rounded up |

The plural "s" is written only when the number is not 1. A watched folder that picks up a few new files shows the bar only if the scan lasts longer than 1 second.

## Tagging bar

The tagging bar sits under the reading bar and looks the same: a bar up to 420px wide and 6px high, a label, and one button at the right. It is drawn from the `tagging_progress` event. What tagging does, its queue and the meaning of each field are defined in [Tagging](07-tagging.md); the full table of states and their exact texts is in [Tagging](07-tagging.md#the-tagging-bar) and wins over this outline.

| State | Shows |
|---|---|
| `problem` is set and tagging is not running | "Tagging stopped: {problem}" in `--error`, and a **Try again** button. |
| Running, or paused with photos still to do | The bar filled to `(done + failed) / total`, "Tagging {done + failed} of {total}" with " · paused" added when paused, and a **Pause** or **Resume** button. |
| Finished with some photos failed | "{failed} photos could not be tagged." and **Try failed photos again**. |
| Anything else, including `total = 0` or no album open | Nothing. |

## Notices

After a scan, `AlbumStatus` says how many files were left out. Each kind is one sentence in the notice strip, which is the information banner of [Visual design](09-visual-design.md#banners). The strip has one **x** that hides it until the numbers change or the album is opened again.

| Kind | Exact text | Meaning |
|---|---|---|
| Unsupported | "N files not supported yet (HEIC, AVIF)" | Files of a format Owlbum recognises but cannot show in this version. They have no row in the database. The brackets list the distinct labels found, in the order of the table of recognised-but-unsupported formats in [Media and scanning](03-media-and-scanning.md#files-recognised-but-not-supported-yet). One file reads "1 file not supported yet (HEIC)". |
| Unreadable | "N files could not be read" | Files of a supported format whose row has a `scan_error`. One file reads "1 file could not be read". |

Each sentence is a link button. Either one opens the same wide dialog **Files not shown**: a scrolling list (max-height 50vh) of each file's path relative to the album root and its reason (the format's reason, such as "HEIC photos are not supported yet.", or the sentence of the `scan_error`), the unsupported files first and then the files that could not be read, each in walker order; a **Copy list** button that puts the list on the clipboard as plain text; and **Close**, which has the focus. The list is read with the commands Media and scanning defines, a page of 500 at a time until all of it is there: `album_unsupported(offset, limit)` for the first kind and `album_scan_errors(offset, limit)` for the second. It is read again when the numbers change while the dialog is open. **Copy list** calls `album_copy_not_shown()`, which writes the whole list in Rust: one line per file, its path, a tab and its reason, in the dialog's order. A toast says "Copied the list", or the error's sentence ("Couldn't copy the list: …"). Files left out never appear in a result, in the map or in any count.

The strip shows whenever `AlbumStatus` has a count above zero, during a scan too; it is drawn from the status the page already reads, so the unreadable count grows as the scan reads files and the unsupported count follows each walk.

## Review screen

A row of five parts: the thumbnail column, a 6px splitter, the preview pane, a 6px splitter, the details column. Only the thumbnail view and the details body scroll. The preview is clipped to its pane.

```text
+-------------------------------+--+------------------------------------------+--+---------------------------+
| THUMBNAIL COLUMN              |  | PREVIEW PANE (takes the rest, min 300px, |  | DETAILS COLUMN            |
| width: default 420,           |S |   background --stage)                    |S | width: default 340,       |
|   min 200, max = room left    |P |                                          |P |   min 260, max 560        |
| +---------------------------+ |L |  +------------------------------------+  |L | +-----------------------+ |
| | [Grid|Timeline|Map]       | |I |  | STAGE: photo fitted, never cropped; |  |I | | IMG_0042.jpg      [x] | |
| |  (padding 8px 12px)       | |T |  | wheel zooms, drag pans; mini map    |  |T | +-----------------------+ |
| +---------------------------+ |T |  | top-right when zoomed               |  |T | | Photo score       74  | |
| | GRID (scrolls, pad 12px,  | |E |  |               "+1" toast (05)       |  |E | | > Photo               | |
| |  gap 8px, square tiles)   | |R |  +------------------------------------+  |R | | > What's in it        | |
| | [tile][tile][tile]        | |  |  | CAPTION (padding 8px 12px):         |  |  | | > Camera              | |
| | [tile][tile][tile]        | |6 |  | 2024/Trip/IMG_0042.jpg [Starred]    |  |6 | | > Histogram           | |
| | [ .. ][ .. ][ .. ]        | |px|  | [Hidden] [2 of 5 in stack]          |  |px| | > Place               | |
| |   (placeholders for rows  | |  |  | [Edited]  (badge: see 06)           |  |  | | > People              | |
| |    not fetched yet)       | |  |  |        ... edit actions (see 06)    |  |  | | > Date and time       | |
| |                           | |  |  |                                     |  |  | | > Description & notes | |
| +---------------------------+ |  |  +------------------------------------+  |  | +-----------------------+ |
+-------------------------------+--+------------------------------------------+--+---------------------------+
```

### Column widths

All values are in px. `room` is the measured width of the row.

| Name | Rule |
|---|---|
| `free` | `room − 300 − 12` (the preview's minimum and the two splitters). Treated as unlimited until `room` has been measured. A hidden column, or thumbnails at the bottom, take no room and no splitter: `free` loses only 6 per splitter shown, and the other column's maximum no longer leaves room for it. |
| `gridMax` | `max(200, free − 260)` |
| Thumbnail column width | `clamp(panel.grid_width, 200, gridMax)` |
| `detailsMax` | `max(260, min(560, free − thumbnail column width))` |
| Details width | `clamp(panel.details_width, 260, detailsMax)` |
| Preview | The rest, never under 300. |

So as the row narrows the preview gives up room first, down to 300; then the details column, down to 260; then the thumbnail column, down to 200: the order [Visual design](09-visual-design.md#the-three-columns) sets. The stored widths are not changed by clamping. A narrow window shows narrower columns and a wider window gives the stored widths back. `gridMax` and `detailsMax` are also the splitters' maximums, so dragging the thumbnail splitter can narrow the shown details column down to 260 (its stored width is kept), while the details splitter stops where the thumbnail column is.

### Splitters

Each splitter is a `role=separator` with `aria-orientation=vertical`, 6px wide, coloured `--line-soft`, and `--accent` on hover, focus or drag. Labels: "Resize thumbnail column" and "Resize details panel".

- **Dragging.** Pointer-down with the primary button captures the pointer. Each move sets the width to `clamp(start + dir × dx, min, max)`, rounded. `dir` is +1 for the thumbnail splitter and −1 for the details splitter.
- **Keys.** With focus on a splitter, Left and Right move it by 16px. Home and End set the minimum and maximum.
- **Saving.** On pointer-up, on lost capture, or 500 ms after the last key, the width is saved with `app_settings_set` under `panel.grid_width` or `panel.details_width`.
- **Double-click** puts the default width back (420 or 340).

### Hiding and moving the panels

Owner's feedback on 0.1.3 (#185). The thumbnails and the details panel can each be hidden and shown again, and the thumbnails can sit in a strip along the bottom of the window instead of the column at the left. Each choice is a setting (`panel.grid_shown`, `panel.details_shown`, `panel.grid_place`, `panel.strip_height`, see [Data](02-data.md)), so it is remembered for every album.

| Way | Thumbnails | Details panel |
|---|---|---|
| Buttons | **Thumbnails**, a toggle button at the preview's top right (`panel-left` or `panel-bottom` icon), pressed while they show | **Details panel**, the toggle beside it (`details` icon) |
| Keys (browse) | `T` (`browse.toggle_grid`) | `I` (`browse.toggle_details`), as in the viewer |
| View menu | `&Thumbnails` (checkbox) | `&Details panel` (checkbox) |

- **Where the thumbnails sit.** View > `Thumbnails at the &bottom` (a checkbox), or Settings > View > Thumbnails (**At the left** or **Along the bottom, under the preview**), sets `panel.grid_place`. Moving them also shows them.
- **The bottom strip.** As wide as the window, under the preview and the details column, with the view switch on top. It is the same grid turned on its side: its rows run down the strip (one or two tiles, as its height allows) and it scrolls sideways; the mouse wheel scrolls it sideways too. Its height is `panel.strip_height` (default 200, at least 120, and at most what leaves the preview 200 px above it), set by a row splitter over it ("Resize thumbnail strip": drag, Up and Down by 16 px, Home and End, double-click for 200), saved like the column splitters. In the timeline each heading reads upwards in its own narrow column; the month rail is not shown in the strip. The map, when it is the view, fills the strip.
- **Hidden thumbnails** stay in the page, out of sight, so the review keys, the current photo, the selection, the rows the preview needs and the store keep working: Left and Right still step through the result. When they show again the current photo is brought into view. A hidden column has no splitter, and the preview takes its room (the column widths below count only the columns in the row).
- **A hidden details panel** is not drawn; the preview still reads the photo's details it needs.
- F6 skips a hidden region.

### View switch

A segmented control at the top of the thumbnail column, labelled "Show the photos as", with three tabs: **Grid**, **Timeline**, **Map** (a `tablist`, as [Visual design](09-visual-design.md#accessibility-baseline) sets for this switch). At its right sits the **Sort** control (see [Organising](05-organising.md#sorting)) while Grid is the view, and the Day / Month switch while Timeline is (owner's feedback, #182). In a narrow column the bar wraps onto a second line. The view replaces only the thumbnails; the preview and details stay. Choosing a view saves the setting `view`, re-runs the query with the new view, and keeps the current photo and the selection (see [When the result changes](#when-the-result-changes)).

## How the page holds a result

This applies to the grid and the timeline. The map differs and says so in its own section.

### Running a query

The page calls `query_run(query, keep?)` whenever the query changes: view, timeline unit, sort, filter, search text, the stack setting, or an opened or folded stack. It also re-runs after an `album_changed` event whose reason calls for it. Rust answers with the `QueryResult` that [Organising](05-organising.md#query_run) defines; this part uses its `token`, `count`, `total`, `album_photos`, `revision`, `groups`, `unit` and `kept`.

- **Which events.** The table of reasons in [Architecture](01-architecture.md#events) says when the page re-runs: always for `scan`, `watch`, `info`, `saved`, `trashed` and `facts`; for `tags` only when the query has search text or a keyword or object filter; never for `tidy` or `status`. For `edit` it only fetches the rows on screen again.
- **Coalescing.** Changes within one animation frame make one call. Re-runs caused by `album_changed` are limited to one per second while a scan or tagging is running, and one per 100 ms otherwise. A re-run is skipped when the event's `revision` is not newer than the revision of the result the page already holds.
- **Late answers.** Each call carries a counter. An answer to anything but the newest call is thrown away.
- **No blank flash.** Until the first rows of a new result arrive, the page keeps drawing the rows it already has at their old places. The total height changes at once.
- **Expired tokens.** Any `query_` command that answers `query_expired` makes the page re-run the query once and repeat the call. For the grid's blocks, the re-run is spaced like a re-run after `album_changed`, and the new result's blocks are then asked for again, which is the repeat; a backend that kept expiring could otherwise keep the page busy for ever.

### Blocks of rows

Rows are fetched in blocks with `query_window(token, offset, limit)`.

| Constant | Value | Meaning |
|---|---|---|
| `BLOCK` | 200 | Rows per block. Block `b` covers indexes `200b` to `200b + 199`. |
| `BLOCKS_KEPT` | 30 | Blocks held per result (6,000 rows). Beyond that, the block furthest from the viewport is dropped. A block that holds the current photo or the anchor is never dropped. |
| `FETCHES` | 3 | Block requests in flight at once. |
| `OVERSCAN_ROWS` | 4 | Rows of tiles rendered above and below the viewport. |
| `AHEAD` | 2 viewports | Blocks are fetched this far past the viewport in the direction of the last scroll. |
| `BEHIND` | 1 viewport | And this far the other way. |

- **What is wanted.** On every frame in which the scroll position or size changed, the page works out the wanted index range: the rendered range, extended by `AHEAD` and `BEHIND`. Blocks touching that range, and the blocks covering indexes `current − 5` to `current + 20` (the preload window), are wanted.
- **Order.** Blocks touching the viewport are asked for first, then outward in the scroll direction.
- **Dropping requests.** A block that is waiting to be asked for and is no longer wanted is taken off the queue. This is what keeps a drag of the scrollbar across 100,000 photos from fetching everything it passes.
- **Placeholders.** A tile whose row has not arrived is a placeholder: the tile's square in `--panel-2` with the usual outline and nothing else. It takes no clicks. When its row arrives the picture is shown at once, without a fade: nothing in the grid animates when rows load (see [Visual design](09-visual-design.md#motion)).
- **A new token** empties the block store, except that old rows stay on screen as described above until their replacements arrive.

### When the result changes

Every `query_run` after the first passes `keep`, so Rust can say where things went.

`keep = { token, current: { id, index } | null, anchor_id, selection: id[], view: { id, index } | null }` and the answer's `kept = { current: { id, index } | null, anchor: { id, index } | null, selection: id[], view: { id, index } | null }`. Both shapes (`Keep` and `Kept`) are defined in [Organising](05-organising.md#the-query).

Rust settles the current photo in this order. [Organising](05-organising.md#where-the-selection-goes-afterwards) states the same four rules for star and hide:

1. If the current photo is in the new result, it stays, with its new index.
2. Otherwise, if stack rules name a replacement (the stack's new face, or the stack is opened so a folded member shows), that photo is used. See [Organising](05-organising.md#stacks).
3. Otherwise the photo "now in its place" is used: let `k` be the number of photos that were before it in the old result and are still in the new one; the new current photo is the one at index `k`, or the last photo when `k` is past the end. If the old token is no longer held, `k` is the old index.
4. If the new result is empty, there is no current photo.

`kept.selection` is the old selection limited to photos still in the result, plus the new current photo. If the anchor is gone, the anchor becomes the current photo.

`keep.view` is the tile the grid keeps its place by while rows arrive (#106): on a re-run after `album_changed`, the current photo when its row is in view, else the first tile in view; null after a change the user made, and null at the very top of the grid when the current photo is out of view. `kept.view` is where that photo is now, or the photo now in its place by rule 3 when it left the result.

**Scrolling after a change.**

| What changed | Scroll |
|---|---|
| The user changed the sort, filter, search, view, timeline unit or stack setting | If there is a current photo, scroll so it is in view (see below). Otherwise go to the top. |
| `album_changed` (a scan, a star, tags) | The grid scrolls by as many rows as `keep.view` moved (from its index in `keep.view` to its index in `kept.view`), so the photos in view stay where they are on screen while a scan adds or reorders rows above them; the current photo never jumps away. At the very top with the current photo out of view nothing is kept and the scroll stays at 0, so a first scan shows the photos that sort first as they arrive. A re-run with no tile to keep (none in view yet, or its row not fetched) keeps the scroll position in pixels. |
| The user opened or folded a stack | The pixel position is kept, then the current photo is brought into view if it is not. |

**First selection.** When an album opens with no session to restore, its first photo becomes current once its row arrives (#119), unless the user picked one first. A restored session's photo wins over the first photo (see [Restoring a session](#restoring-a-session)). With nothing current, the preview shows the hint "Select a photo to see it here."

## The grid

The grid is a scroll box with 12px padding (`--space-4`), an 8px gap between tiles (`--space-3`) and background `--stage`. Inside is one box as tall as the whole content. Only tiles in view, plus the overscan, exist as elements. Each row of tiles is a `role=row` box placed with `transform: translateY(y)`; each tile inside it is placed with `transform: translateX(x)` and has `contain: strict`. The rows are what [Visual design](09-visual-design.md#accessibility-baseline) asks of the grid's roles.

### Layout maths

```text
width      = max(0, clientWidth − 24)                       // content width inside the 12px padding
cols       = max(1, floor((width + gap) / (tile + gap)))    // gap = 8, tile = thumb_size
side       = max(1, (width − (cols − 1) × gap) / cols)      // tiles stretch to fill the row
stride     = side + gap
rows       = ceil(count / cols)
total      = rows > 0 ? rows × stride − gap : 0
top        = max(0, scrollTop − 12)
firstRow   = clamp(floor(top / stride) − 4, 0, rows)
endRow     = clamp(ceil((top + viewport) / stride) + 4, 0, rows)
firstIndex = firstRow × cols
lastIndex  = min(count, endRow × cols) − 1
tile i     → x = (i mod cols) × stride, y = floor(i / cols) × stride
```

- **Measuring.** The box is watched for size changes. A scroll is handled once per animation frame.
- **A tile's side** is never smaller than the chosen thumbnail size unless the column is narrower than one tile.
- **Very tall content.** Browsers stop laying out elements beyond a height limit. When `total` is more than `MAX_SCROLL = 8,000,000` px, the inner box is made exactly `MAX_SCROLL` tall (plus the padding) and positions are scaled: the real scroll position is `scrollTop × (total + 24 − viewport) / (MAX_SCROLL + 24 − viewport)`, and `top` is worked out from it as above, so the last row sits exactly at the bottom at the end of the bar. Wheel and key scrolling are then applied by the page in real pixels (the wheel's delta is divided by the same ratio before it is added to `scrollTop`), so one wheel notch still moves about one row. Dragging the scrollbar moves in proportion. Below the limit none of this applies.

### Keeping the current photo in view

Whenever the current photo changes, or `cols` changes, and the viewer is closed, the grid scrolls just enough to show the current photo's row:

- its top is `12 + floor(index / cols) × stride`;
- if `top − 12 < scrollTop`, set `scrollTop = top − 12`;
- else if `top + side + 12 > scrollTop + clientHeight`, scroll so that edge sits at the bottom of the box;
- otherwise do nothing.

The index comes from the current photo's `{ id, index }`. When only an id is known (after a restore, or a pick on the map), the page calls `query_locate(token, id)` first. It answers `{ index }`, or `{ folded_in }` with the id of the closed stack that holds the photo, or null. For `folded_in` the page adds that stack to its open list and re-runs the query, so the photo shows (see [Organising](05-organising.md#stacks)). An answer of null means the photo is not in the result, and the current photo is settled as in [When the result changes](#when-the-result-changes).

### Search by meaning

When a result holds photos found only by meaning, they start on a new row under a note "Also matching by meaning". The photos are a meaning segment appended to the end of the token's list by `search_meaning(token)`, which the page calls after `query_run` has answered; its reply gives `meaning_start` and the new `count` and `photos`. Which photos are in the segment is defined in [Organising](05-organising.md#how-it-combines) and [Tagging](07-tagging.md#search-by-meaning). Until that reply arrives, and whenever it reports no matches, `meaning_start` is null and there is no note. The grid's part is this: with `meaning_start = m`, `pad = ((cols − m mod cols) mod cols) + cols`; a photo at index `i ≥ m` sits in slot `i + pad`; the note fills the row before slot `m + pad`. The note row is a grid row holding a row header with `role="note"`: muted `--text-sm` text at the row's foot over a 1px `--line-soft` line. Only the grid view asks for the segment; the timeline and the map never show one.

### Thumbnail size

| Rule | Value |
|---|---|
| Range | 96 to 320, default 160, slider step 8. |
| Clamping | `min(320, max(96, round(x)))`; anything that is not a finite number becomes 160. |
| Storage | The setting `thumb_size`, saved 300 ms after the slider last moved. |
| Scroll while resizing | The photo at the top-left of the viewport stays in the top row. |
| Which thumbnail a tile loads | Small (`owlbum://thumb/<key>/s`, 512px) unless `side × devicePixelRatio > 512`, then medium (`/m`, 1600px). |

### Tiles

A tile is a square with radius `--radius-lg`, hidden overflow, background `--panel-2` and a 1px `--line-soft` outline. Text cannot be selected in it. Its tooltip is the photo's path relative to the album root (the row's `rel_path`).

**Picture.** The thumbnail, drawn to cover the square, decoded off the main thread. A photo with an edit is drawn edited: the page applies the row's `edit_plan` to the same thumbnail, as [Editing and file actions](06-editing-and-file-actions.md#how-the-page-draws-a-plan) defines. There is no separate address for an edited picture. A thumbnail that fails to load leaves the `--panel-2` square with the `warning` icon in `--muted`.

**Marks.** Their places, sizes and colours are the badges of [Visual design](09-visual-design.md#badges-on-tiles); every badge sits 4px from the tile's edge on `--badge-bg` with `--badge-text`.

| Mark | Position | Shown when | Look and behaviour |
|---|---|---|---|
| Star | bottom-left | The photo is starred, or the pointer is over the tile, or the tile is selected or has focus inside it | A 24px button. Filled star in `--accent` when starred, outline star in `--badge-text` when not. Label "Star" or "Unstar". A click stars or unstars this photo only (see 05). Fades in over `--motion-fast`. |
| Hidden | top-left | The photo is hidden | The `eye-off` icon, label "Hidden". The picture is dimmed to 45% opacity. |
| Edit | top-left, after Hidden | The row's `edit_plan` is not null | An 8px dot. With `edit_target = album`: `--marker-edited`, label and tooltip "Edited. The edit is kept in the album; the file is unchanged." With `edit_target = file`: `--marker-unsaved`, label and tooltip "Edited, not saved into the file yet". See [Editing and file actions](06-editing-and-file-actions.md#markers). |
| Stack count `×N` | top-right | The tile is a stack's face | A pill: min-width 28px, height 20px, `--text-xs`. A click opens or folds the stack. Rules, labels and the look of an open stack are in [Organising](05-organising.md#stacks). |
| Score | bottom-right | Always | A pill: min-width 24px, height 18px, `--text-xs`, tabular figures. The photo score, or "–" until the photo has been analysed. Label "Score N" or "Score not worked out yet". |
| Video length | bottom-left, after the star | The row's `kind` is `video` | A pill, height 18px, `--text-xs`: a play triangle and the length. See [Video](#video). |
| Animated | bottom-left, after the star | The row's `kind` is `image` and its `duration_ms` is set | A pill reading "GIF" or "WebP". The tile does not animate (see [Media and scanning](03-media-and-scanning.md#animated-pictures)). |

**Outlines.** Later rows win.

| State | Outline |
|---|---|
| Member of an open stack | See [Organising](05-organising.md). |
| In the selection | 2px `--select`, offset −2px. |
| The current photo | 3px `--select`, offset −3px. |

Outlines are drawn in a layer above the picture, so an edited or dimmed picture never covers them.

**Pointer**

| Event | Effect |
|---|---|
| Click | Select (see [Selection](#selection)), with the Ctrl and Shift state. |
| Double-click | Open the viewer on the photo. |
| Press and drag | Drag the photo, or the selection if the photo is in it, out of the window (see 06). |
| Right-click | Select the photo if it is not already in the selection, then open the photo menu (see 06). |

A click or double-click on the star button or the stack count is not a click on the photo.

## Selection

### Clicking

| Click | Result |
|---|---|
| Plain | The photo becomes the current photo, the whole selection and the anchor. |
| Ctrl on a photo not selected | It is added. It becomes the current photo and the anchor. |
| Ctrl on a selected photo | It is removed. If it was the current photo, the current photo becomes the selected photo nearest to it in the result (the next one after it, else the one before). The anchor becomes the clicked photo. Removing the last selected photo leaves nothing selected. |
| Shift | The selection becomes every photo from the anchor to the clicked photo, inclusive. The clicked photo becomes the current photo. The anchor does not move. With no anchor, Shift-click is a plain click. |
| Ctrl+Shift | The same range is added to the selection instead of replacing it. |

A Shift range is worked out by index. Ids for rows the page already holds are read from them; the rest come from one `query_ids(token, offset, limit)` call. Until it answers, the tiles in the range that are on screen are shown selected by index. A step onto a row not fetched yet is shown the same way, at its index, until the row arrives. While either waits, the next `query_run` keeps the selection as it was before; the new result drops the waiting change and any late answer to it. A click on a tile still showing the previous result's row (see [No blank flash](#running-a-query)) does nothing.

Selecting another photo while a photo is being edited ends edit mode first, as its Done button would (see 06).

### Keys that change the selection

When an album opens with no current photo, the first photo of the result becomes current (and the anchor and selection) as soon as its row arrives, so the preview and the details panel show it and the arrow keys work at once (#119). A photo already current, kept from before or picked while the rows were arriving, is left alone, and once any photo has been current the rule does not apply again until the album is opened again.

Ctrl+A selects every photo shown in the result (one `query_ids` call) and leaves the current photo where it is, or makes the first photo current when there was none. "Shown" means the tiles: photos folded inside a closed stack are not included, as [Organising](05-organising.md#stacks) says. Ctrl+D selects nothing: the selection is emptied, there is no current photo and no anchor, the preview shows its hint and the details panel is empty. Esc, when more than one photo is selected, reduces the selection to the current photo, which also becomes the anchor.

### What acts on what

The preview, the details panel and the viewer always show the current photo. Star, hide, score, delete, copy and drag act on the selection; their exact targets are defined in 05 and 06. The page never orders the selection: a command that needs the photos in result order takes the `token` along with the ids.

### The preview and details never disagree

Both read one value, the current photo's id. Neither looks a photo up for itself in any list. The current photo is always a member of the current result, because every `query_run` answer settles it before anything is drawn. If `photo_detail` answers `photo_not_found`, the page re-runs the query, which settles the current photo again.

## Preview pane

A region labelled "Preview", with a stage and a caption. With no current photo, the stage shows "Select a photo to see it here." (muted, centred) and there is no caption.

**Caption** (padding 8px 12px, a top border). In order:

- the photo's path relative to the album root, in bold, cut with an ellipsis at the start when it does not fit; its tooltip is the full path on this computer;
- badges: **Starred** (`--accent`) when starred; **Hidden** when hidden; **"{place} of {size} in stack"** when the photo is in a stack (from the row's `stack`; see 05); the edit badge when the photo has an edit: **Edited** (`--marker-edited`) when `edit_target` is `album`, where Done is the save and nothing is waiting, or **Edited · not saved** (`--marker-unsaved`) when `edit_target` is `file`, until Save writes the edit into the file (see [Editing and file actions](06-editing-and-file-actions.md#markers));
- at the right end, the edit actions defined in [Editing](06-editing-and-file-actions.md).

When more than one photo is selected, the caption starts with "N selected ·" before the path.

### Which picture is shown

| Step | Rule |
|---|---|
| 1 | If the medium (1600px) picture is ready in the image cache, show it at once. The small one is then not loaded (the mini map still loads it when zoomed in). |
| 2 | Otherwise show this photo's small (512px) thumbnail and ask the cache for the medium one. |
| 3 | Swap to the medium picture when it is decoded. The view does not move. |
| 4 | Never show another photo's picture under this photo's name. If neither picture of this photo is ready, the stage is empty for that moment. |

A new photo, or a new edit of the same photo, always starts fitted.

### Zoom and pan

The same zoom component is used by the preview and the viewer.

| Constant | Value |
|---|---|
| `MIN_ZOOM` | 1 (fitted) |
| `MAX_ZOOM` | The larger of 8 and the zoom at which one picture pixel is 2 screen pixels. |
| `WHEEL_STEP` | 1.2 per wheel notch |

The picture is always laid out at its fitted size and placed with `transform: translate(x, y) scale(zoom)` from its top-left corner. It is hidden until its size is known.

- **Fitted size.** The largest size with the picture's shape that fits wholly in the stage: `scale = min(stageW / imgW, stageH / imgH)`. It is 0 × 0 if any input is 0 or less.
- **Fit view.** Zoom 1, centred.
- **Clamping the pan.** On each axis: if the scaled picture is no bigger than the stage, centre it; otherwise keep its position within `[stage − size, 0]`, so no gap shows at an edge.
- **Zooming at a point.** `zoom = clamp(zoom × factor, MIN_ZOOM, MAX_ZOOM)`. If that is 1, use the fit view. Otherwise keep the picture point under the pointer fixed, then clamp the pan.
- **Wheel.** Up zooms in by 1.2, down zooms out by 1/1.2. The wheel never scrolls the page.
- **Drag.** With zoom above 1, a drag with the primary button pans. The cursor is `grab`, and `grabbing` while pressed. At fit in the preview, a press and drag drags the photo out of the window, as from a tile (see 06).
- **Double-click.** When zoomed in, back to fit. At fit: in the preview it opens the viewer; in the viewer it zooms to actual size (one picture pixel to one screen pixel) at the pointer.
- **Right-click.** Opens the photo menu.
- **Resizing the stage** (a splitter, the window) goes back to fit.

**Full-size swap.** When the picture is drawn larger than the medium picture has pixels (`fittedWidth × zoom × devicePixelRatio > mediumWidth`), the page loads `owlbum://full/<id>?v=<thumb_key>` and swaps it in when it is decoded. The view does not move. At most one full-size picture is held at a time, and it is let go when another photo is shown. A photo whose original is no larger than the medium picture never asks for it.

**Where the sizes come from.** The picture's shape is the row's `aspect`, so the layout never follows the size of whichever picture has loaded and a swap cannot move the view. The original's size (for `MAX_ZOOM`, `mediumWidth` and the "no larger" rule) is `width` and `height` from `photo_detail`, which the review screen reads once for the details panel and the preview. Until that answer arrives `MAX_ZOOM` is 8 and no full-size picture is asked for.

### Mini map

Shown while zoomed in, in the preview and in the viewer.

- **Placement.** Top-right of the stage, 10px in (in the viewer, below its top bar). Its long side is 160px; the other side follows the picture's shape. It shows the small thumbnail, edited if the photo is.
- **Look.** Opacity 0.75, and 1 under the pointer. A 1px border of white at 50%. Crosshair cursor.
- **View rectangle.** The visible part of the picture is outlined with a 2px `--accent` border, and everything outside it is darkened with `--crop-shade`.
- **Pointer.** Pressing or dragging on it centres the view on that point. Its pointer, double-click and right-click events do not reach the stage behind.

## Image cache and preloading

Stepping through photos must feel instant. On every step, the next 20 photos and the previous 5 are already decoded in memory, so the next one paints with no wait.

One cache is shared by the preview and the viewer. It holds decoded medium pictures.

| Constant | Value | Meaning |
|---|---|---|
| `AHEAD` | 20 | Photos preloaded in the direction of travel. |
| `BEHIND` | 5 | Photos preloaded the other way. |
| `LEAD` | 3 | The nearest photos ahead, loaded before anything behind. |
| `MAX_IN_FLIGHT` | 4 | Background loads running at once. |
| `CAPACITY` | 40 | Decoded pictures kept. A medium picture is about 10 MB decoded. |
| `FAILED_KEPT` | 500 | Failed addresses remembered; the oldest is forgotten first. |
| `FAILED_FOR_MS` | 15,000 | How long a failure blocks a retry. |

### Order

`preloadOrder(index, count, direction)` gives the indexes to load, most wanted first. It never includes `index` itself, a duplicate, or anything outside `0 … count − 1`.

1. The next `LEAD` places in the direction of travel.
2. One place behind.
3. The rest of the places ahead, up to `AHEAD`.
4. The places behind from 2 to `BEHIND`.

`direction` is +1 or −1: −1 when the new index is below the last one, +1 when above, unchanged when equal. It starts at +1. A direction of −1 mirrors the order.

### Cache rules

- **Wanting.** On every change of the current photo (preview) or the shown photo (viewer), the page calls `want([current, ...preloadOrder])`, mapped to medium addresses. Rows needed to build those addresses are fetched as blocks first; an index whose row has not arrived is skipped and picked up when it does.
- **Cancelling.** A `want` call replaces the wanted list. A load that is running but no longer wanted is cancelled. Holding an arrow key therefore never leaves stale loads queued ahead of the photo on screen.
- **Priority.** The first wanted picture, the one on screen, starts at once with high priority even when `MAX_IN_FLIGHT` loads are running. Every other picture starts only while fewer than `MAX_IN_FLIGHT` loads for pictures other than the one on screen are running, with low priority, in order.
- **Ready** means loaded and decoded. A picture that loaded but would not decode ahead of time still counts as ready if it has a size.
- **Eviction.** After each success, ready pictures beyond `CAPACITY` are dropped, least recently wanted first. A wanted picture is never dropped.
- **Failures.** A failed load is remembered for `FAILED_FOR_MS`, so a broken file is not asked for on every step.
- **Edited photos.** The cached picture is the plain medium thumbnail. The edit is drawn over it from the row's `edit_plan` when the picture is shown, so a change to the edit needs no new load and no new address (see 06).
- **Videos.** The cached picture is the video's poster frame. The video file itself is never preloaded.
- **Letting go.** Closing the album empties the cache. An `album_changed` with `rekeyed` drops the cached pictures of those photos (the cache keeps each picture's photo id for this).

Lookups from id to index are by map, never by walking a list.

## Full-screen viewer

The viewer covers the whole window: a `role=dialog` with `aria-modal`, labelled with the shown photo's file name, background `--viewer-bg`.

**Opening.** A double-click on a tile, a double-click on the preview at fit, or Enter or Space with a current photo. It does not open while a photo is being edited. Opening first takes focus away from any field, so text being typed is saved.

**What it steps through.** The current result, in its order, exactly as the grid or timeline shows it. In the Map view it is the map's result (every filtered photo, stacks unfolded).

```text
+--------------------------------------------------------------------------------------------+
| IMG_0042.jpg  [star] [eye-off]  12 / 340     ← → move · S or F star · H hide · I details · Esc close  [x] |
|   (top bar: padding `--space-3` `--space-5`, gradient from 70% black to clear)                           |
|                                                                        +---------------+   |
|                                                                        | mini map      |   |
|                      PHOTO: fitted to the whole window;                | (when zoomed) |   |
|                      wheel zooms, drag pans, edits shown               +---------------+   |
|                                                                                            |
|                          (180px star mark plays on S or F, 0.8 s)                          |
|                                                          +-------------------------------+ |
|                                                          | DETAILS (I): 320px wide,      | |
|                                                          | 16px from right and bottom,   | |
|                                                          | max-height 100% − 80px        | |
|                                                          | IMG_0042.jpg                  | |
|                                                          | Taken   12 Jun 2024, 14:03    | |
|                                                          | Camera  …                     | |
|                                                          | Size    4032 × 3024 px · 3.1 MB| |
|                                                          | People  Ann, Bob              | |
|                                                          | Description …                 | |
|                                                          +-------------------------------+ |
+--------------------------------------------------------------------------------------------+
```

**Top bar.** The file name in bold (tooltip: the full path); a filled star in `--accent` when starred; an eye-off icon when hidden; the edit badge when the photo has an edit (**Edited** or **Edited · not saved**, as in the caption); "{index + 1} / {count}"; the key hint "← → move · S or F star · H hide · I details · Esc close" (small, muted, hidden below 900px of width); a close button, tooltip "Close (Esc)". The bar fades out after 3 seconds without pointer movement and returns on any pointer movement or key.

**Picture.** The same rules as the preview: medium picture from the cache, else this photo's small thumbnail until the medium one is decoded. **The picture shown is the edited one whenever the photo has an edit**, the same as the tile and the preview. **Zoom, pan, the mini map and the full-size swap work exactly as in the preview.** While the next picture is being decoded the previous photo is not left on screen under the new name: the new photo's small thumbnail is shown at once.

**Details panel.** Toggled with I. It is read-only and shows: the file name; Taken (or "Unknown"); Camera (when known); Size ("W × H px · size"); Length (videos); People (when any); Description (when any). Whether it is open is kept for the session.

**Stepping.** Right goes to the next photo and stops at the last. Left goes to the previous and stops at the first. Home and End go to the first and last. Arrow keys repeat when held. Each step updates the current photo, so the grid behind is already in the right place when the viewer closes.

**When the shown photo leaves the result** (hidden under the default filter, starred under "Not starred", deleted), the viewer shows the photo now in its place, by the rule in [When the result changes](#when-the-result-changes). If the result becomes empty the viewer closes.

**Closing.** Esc, the close button, or Enter. The last photo shown becomes the current photo and the whole selection. Esc while zoomed in goes back to fit first; a second Esc closes.

**Keys.** While the viewer is open nothing else on the page receives keys. The full list is in [Keys](#keys).

## Video

Decoding, poster frames and delivery are defined in [Media and scanning](03-media-and-scanning.md). This is what the person sees.

| Place | Behaviour |
|---|---|
| Tile | The poster frame, with a length pill bottom-left: a play triangle and the length. |
| Preview | The poster frame with a 64px round play button in the middle. A click on it, or Space with focus on the stage, replaces the poster with a player showing the system's standard controls, and starts playing with sound. |
| Viewer | The same, sized to the window. Space plays and pauses. |
| Details | The Photo section adds **Length**. The Histogram section is not shown. |
| Map, timeline | A video is a photo like any other. |

- **Length text.** `m:ss` under an hour, `h:mm:ss` from an hour, rounded down to whole seconds. A video with no known length shows only the play triangle.
- **Nothing plays by itself.** A video never starts on selection.
- **Stopping.** Moving to another photo, closing the viewer, opening the viewer from the preview, or closing the album stops playback and lets the player go. Opening the viewer on a playing preview starts again from the beginning, paused.
- **No zoom.** Wheel zoom, pan, the mini map and the full-size swap do not apply to a video.
- **Keys.** Left and Right still move between photos while a video is playing; they never seek. Seeking is done with the player's own bar.
- **Cannot play.** If the player reports an error, the stage shows the poster with "This video can't be played on this computer." and a **Show in folder** button (see 06).

## The details panel

The panel shows the current photo. With no current photo the column is an empty panel with the text "No photo selected" (muted). All its local state starts again for each photo.

```text
+---------------------------------------+   column: default 340px, 260–560px, background --panel, left border
| +-----------------------------------+ |   body: padding 12px, gap `--space-5`, scrolls
| | Photo score                    74 | |   score card (see 05)
| +-----------------------------------+ |
| (note: view-only / save error)        |
| v Photo                               |
|   Taken       12 Jun 2024, 14:03 [changed]
|   Size        4032 × 3024 px · 3.1 MB |   fact rows: grid 88px | 1fr
|   Resolution  12.2 MP · 4:3           |
|   Shape       Landscape               |
|   Format      JPEG                    |
|   Folder      2024/Trip/Day 2         |
|   Path        D:\Photos\2024\…\IMG_0042.jpg [copy]
|   [copy Copy]                         |   copies all of the panel
| v What's in it      (see 07)          |
| v Camera                              |
|   Camera / Lens / Focal length / Aperture / Shutter / ISO / Exposure / Flash
| v Histogram   [80px chart]            |
|   Shadows clipped 0.4 %  Highlights clipped 2.3 %
| v Place                               |
|   Country     France                  |
|   Coordinates 48.85837, 2.29448       |
|   [map-pin Pick on map] [map Show on map]
|   LOCATION                    Saved   |
|   [Search for a country…]             |
|   Latitude [____]  Longitude [____]   |
|   Place name [__________]             |
| v People                              |
|   (Ann x) (Bob x)                     |
|   [Add a name, press Enter]           |
| v Date and time                       |
|   [2024-06-12 14:03:00  ]             |
|   Use the camera's date               |
| v Description and notes               |
|   DESCRIPTION [__________]            |
|   NOTES [text area, 4 rows]           |
| v EXIF                                |
|   Make        Owlbum                  |
|   ExposureTime 1/250 …                |
+---------------------------------------+
```

**Copy.** A small **Copy** button (icon `copy`, tooltip "Copy all of these details as plain text") at the end of the Photo section, the section below the photo score (owner's feedback, #183; it was in a head over the panel before). It is there once the photo's details have arrived, and folds away with the Photo section. It reads the score (`photo_score`) and the EXIF tags (`photo_exif`) afresh and puts everything the panel shows on the clipboard as plain text with `file_copy_text`: the photo score, then each section that has something to say, its title on a line of its own and one "Label: value" line per row, with a blank line between sections; the EXIF section lists every tag as "Name: value". A score or EXIF read that fails leaves that part out. The toast says "Details copied". The panel has no head: no file name and no close button (owner's feedback, #167): the Path row names the file, and Ctrl+D still selects nothing.

**Icons on buttons.** The panel's buttons carry an icon from Owlbum's set (see [Visual design](09-visual-design.md#icons)) as well as their words (owner's feedback, #183): Copy and Copy path `copy`, Regenerate `rescan`, Pick on map `map-pin`, Show on map `map`, Use the camera's date `undo`, Clear the date `x`. Links inside a sentence (Settings, Retry) and the chips stay as they are.

**Loading.** The panel calls `photo_detail(id)` when the current photo changes, and again when an `album_changed` event arrives for a revision newer than the detail's `revision` (also when such an event arrived while the detail was being read). While the first answer is on its way, the body is empty; the previous photo's facts are never shown for the new one. If the read fails, the body says "The details of this photo could not be read." An answer for a photo that is no longer current is thrown away.

**Notes under the score card.** In a view-only album: "This album is view-only, so nothing here can be changed." When the last save of this photo failed: the error's message, in a note with an `--error` border, until a later save succeeds.

### Sections

Each section has a header button with a chevron (down when open, right when folded) and `aria-expanded`. All are open by default. The state is kept in the setting `sections`, a JSON object `{ [id]: boolean }`; a missing id counts as open. A folded section renders nothing, so a folded Histogram costs nothing.

| id | Title | Content |
|---|---|---|
| `photo` | Photo | The fact rows below. |
| `whatsinit` | What's in it | Defined in [Tagging](07-tagging.md). |
| `camera` | Camera | The camera rows below. |
| `histogram` | Histogram | See [Histogram](#histogram). Not shown for videos. |
| `place` | Place | Country and Coordinates rows when the photo has a place, then the Location field. |
| `people` | People | The People field. |
| `date` | Date and time | The Date and time field. |
| `notes` | Description and notes | The Description and Notes fields. |
| `exif` | EXIF | Every EXIF tag the file carries, in file order, one row each: the tag's EXIF name (tooltip: its group, `Image`, `Exif`, `GPS` or `Interop`) and its value as text. Read with `photo_exif(id)` while the section is open; "No EXIF data in this file" when there are none, "The EXIF data could not be read." when the read fails. See [EXIF section](#exif-section). |

### Facts

| Row | Text |
|---|---|
| Taken | The effective date as "12 Jun 2024, 14:03" (day, short month, year, 24-hour time), or "Unknown". It is formatted from the written digits and never passed through the computer's time zone. A **changed** badge follows when the date was typed here; its tooltip is "The date was changed here; the camera's is kept in the photo". |
| Size | "W × H px · size". Size is "x.x GB" from 1 GB, "x.x MB" from 1 MB, else "N KB" with a minimum of 1. 1 MB is 1,048,576 bytes. |
| Length | Videos only. The same text as the tile's length. |
| Resolution | "x.x MP · ratio". Megapixels are `W × H / 1,000,000` to one decimal. |
| Shape | "Square" when the sides differ by less than 1%, else "Landscape" or "Portrait". |
| Format | The format in upper case ("JPEG", "CR3", "MP4"). |
| Folder | The photo's folder relative to the album root. Not shown for a photo in the album root. |
| Path | The file's absolute path on this computer, wrapping anywhere, with a **Copy path** icon button beside it that copies it (`file_copy_paths` with this photo's id, as the photo menu's Copy path does). |

**Ratio names.** The ratio is the long side over the short side. It takes the first name it is within 2% of: 1:1, 5:4, 4:3, 3:2, 16:9, 2:1. Otherwise it is written "r.rr:1".

**Camera rows.** A row appears only when its value is known. With none: "No camera details in this file".

| Row | Text |
|---|---|
| Camera | The camera's name. |
| Lens | The lens name. |
| Focal length | "N mm", then " (M mm in 35 mm terms)" when the file gives a 35 mm value that differs by 1 or more. |
| Aperture | "f/N" |
| Shutter | "1/N s" below one second, else "N s". |
| ISO | The number. |
| Exposure | "+0.7 EV", "−1 EV" (a real minus sign) or "0 EV". |
| Flash | "Fired" or "Did not fire". |

**Place rows.** **Country** is the country that contains the photo's effective place, or "…" while it is being looked up, or absent when none is found. A place at sea within 12 nautical miles (22.2 km) of a coast is in that coast's country, which also covers the simplified coastline (see [Country search](#how-typed-fields-save)); farther out there is none. **Coordinates** is latitude then longitude, each to 5 decimals.

### EXIF section

Owner's feedback (#167): the panel lists what is in the file's EXIF, not only the facts Owlbum picks out. `photo_exif(id)` answers `ExifField[]`, `{ group, name, value }`, in file order, read from the file each time and never stored. Rust reads the tags with `nom-exif` (already used for video tracks), which finds the EXIF block in JPEG, HEIF and AVIF, PNG, WebP, TIFF and the TIFF-based RAW files, CR3 and RAF, and names the tags it knows; an unnamed tag is "Tag 0x1234". When `nom-exif` finds nothing (JPEG XL, for one), the media handler's own EXIF reader is asked instead, so a tag the scan used is never missing. The thumbnail IFD (IFD 1) is left out. Values: text as written, numbers, ratios as "1/250" ("35" when the denominator is 1), lists joined with ", ", dates as "2024-06-12 14:03:00", and bytes that are not printable text in hex, at most 32 of them, then "… (N bytes)". A value is cut at 300 characters with "…". Errors: `photo_not_found` as for `photo_detail`.

### Histogram

The page calls `photo_histogram(id)` when the section is open and the photo or its `thumb_key` changes. It is counted on demand and never stored: a folded section costs nothing, and the scan and the album database are unchanged. Rust answers with four arrays of 64 counts: luminance, red, green and blue. They are counted from the photo's medium thumbnail, reduced so its long side is at most 256px. The histogram describes the file, not the edit, as [Editing and file actions](06-editing-and-file-actions.md#where-edits-are-shown) says, so it agrees with the clipping line beneath it, which comes from the scan's analysis of the file. Luminance is `0.2126 R + 0.7152 G + 0.0722 B`. Transparency is ignored.

- **Chart.** 80px high and as wide as the panel. Four filled areas, luminance behind (`--hist-luma` at `--hist-luma-fill`) and red, green, blue in front (`--hist-red`, `--hist-green`, `--hist-blue` at `--hist-fill`), all scaled to the single highest count among the four.
- **Clipping line.** When the photo has been analysed, a line beneath reads "Shadows clipped x.x %" and "Highlights clipped x.x %". Either is in `--error` when it is 2.0 % or more.
- **Failure.** "The histogram could not be drawn."

### How typed fields save

Every typed change goes through `photo_set_info([id], patch)`. A null in the patch clears that field. Typed fields always change the current photo only, never the rest of the selection. The one exception is the [place picker](#the-place-picker), which the owner asked to work for a selection (#169).

- **Confirmation.** On success the field's label shows "Saved" in `--ok` (`role=status`) for 1,800 ms.
- **Failure.** An error toast shows the error's message, the note under the score card appears, and the field keeps what was typed.
- **View-only.** Every field is disabled.

| Field | Control | When it saves | What is sent |
|---|---|---|---|
| Description | One-line text box, label "Description" | On blur. Enter blurs. | `{ description }`: the text with leading and trailing spaces removed, or null when that is empty. Not sent when unchanged or already on its way. |
| Notes | Text area, 4 rows, resizable downwards, label "Notes" | On blur. Enter adds a line. | `{ notes }`: the same rule. |
| People | Chips, plus a text box with the placeholder "Add a name, press Enter" and suggestions | Enter, a comma (also inside pasted text), or blur with text in the box adds a name. A chip's **x** removes one. Backspace in an empty box removes the last chip. | `{ people }`: the whole list, or null when empty. |
| Date and time | One-line text box, label "Date and time taken" | On blur or Enter. | `{ date_override }`; see the rules below. |
| Location | A group: "Search for a country…", Latitude, Longitude, Place name. The **Pick on map** button above it opens the [place picker](#the-place-picker). | When focus leaves the whole group, or Enter in any of its boxes. | `{ location: { lat, lon, name } }` or `{ location: null }`; see the rules below. |

**People rules.**

- A name has its leading and trailing spaces removed, and runs of spaces inside it made one. Empty names are dropped. A name is at most 100 characters.
- Names are compared ignoring case. A name already on the photo is not added twice. When a name matches a person already known in the album, the album's spelling is used (first spelling wins, as [Data](02-data.md) says).
- Suggestions come from `album_people`, which [Organising](05-organising.md#people-suggestions) defines together with the rules: names already on the photo are left out, typing narrows the list to names with a word that starts with the typed text (ignoring case and accents), at most 8 are shown, most used first.

**Date rules.** The box shows the effective date as `YYYY-MM-DD HH:MM:SS`, or is empty with the placeholder "YYYY-MM-DD HH:MM" when the photo has no date. A plain text box is used, because the browser's own date control is not dependable on every platform Owlbum runs on.

| What was typed | Result |
|---|---|
| The same as what was shown | Nothing is sent. Any error is cleared. |
| `YYYY-MM-DD HH:MM:SS`, `YYYY-MM-DD HH:MM` or `YYYY-MM-DD`, with a space or a `T` between date and time | Missing seconds become `00`; a missing time becomes `12:00:00`. If it is a real calendar date and time, with a year from 1800 to 2200, `{ date_override: "YYYY-MM-DDTHH:MM:SS" }` is sent. |
| The same moment as the camera's date | `{ date_override: null }` is sent, so no pointless override is stored. |
| Empty, and the photo has a typed date | `{ date_override: null }` is sent. |
| Empty, and the photo has no typed date | The shown value is put back. |
| Anything else | Nothing is sent. The error "That is not a valid date and time. Use YYYY-MM-DD HH:MM." shows under the box. |

No time zone is ever applied. When a typed date exists, a link **Use the camera's date** under the box sends `{ date_override: null }`. When the file has no date of its own to go back to, the link reads **Clear the date** instead.

**Location rules.** The boxes start from the effective place (the typed one, else the camera's GPS) and the typed place name. They are reset whenever those change and the group holds nothing unsaved.

| What is in the boxes | Result |
|---|---|
| A latitude or longitude that is not a number | "Latitude must be a number." or "Longitude must be a number." |
| A latitude outside −90 to 90 | "Latitude must be between -90 and 90." |
| A longitude outside −180 to 180 | "Longitude must be between -180 and 180." |
| Only one of the two | "Enter both latitude and longitude." |
| Neither, with a place name | "Add a latitude and longitude for this place, or clear the name." |
| Neither, with no name, and the photo has a typed location | `{ location: null }` is sent. The boxes then show the camera's GPS again, if any. |
| Neither, with no name, and no typed location | Nothing is sent. The camera's values are put back. |
| Both, valid, and equal to the camera's GPS (to 5 decimals) with no name | `{ location: null }` is sent when a typed location exists; otherwise nothing. |
| Both, valid | `{ location: { lat, lon, name } }`, with `name` null when empty. |

Each coordinate has spaces removed from its ends; an empty box counts as absent. A decimal comma is accepted. Only the first error is shown, under the group.

**Country search.** Typing in "Search for a country…" lists up to 6 countries whose name contains the text, ignoring case and accents (`app_place_search`): names that start with the text first, then names with a word that starts with it, then the rest, each group in name order. Up and Down move through the list, Enter or a click chooses, Esc closes it. The Country row and this search both use the Natural Earth country outlines that [Architecture](01-architecture.md#dependencies-chosen) bundles, with a point-in-outline lookup in Rust; nothing is fetched. Choosing one puts its name in Place name. If the latitude and longitude boxes are empty, it also fills them with the country's central point (Natural Earth's label point, or a point on the largest island when that lies at sea). Then the group saves.

**Which country.** Every Natural Earth admin-0 feature counts as a country, under its English name, so dependencies and territories (Greenland, Puerto Rico, Hong Kong) have their own. Where outlines overlap, the smaller one wins. Disputed areas follow Natural Earth's default view, the side that administers an area on the ground: Crimea is in Russia, each side of the Kashmir Line of Control is in India or Pakistan and Aksai Chin in China, Laayoune is in Morocco; areas Natural Earth draws on their own keep their own name (Western Sahara east of the berm, Kosovo, Northern Cyprus, Somaliland, Taiwan, Palestine, Siachen Glacier). Owlbum takes no position on any dispute.

**Text is never lost.**

- **Drafts.** A text box's draft is kept apart from the saved value. The draft follows the saved value only while it holds nothing unsaved, so neither a save's answer nor a change from elsewhere overwrites text still being typed.
- **Leaving without blur.** When the panel is about to show another photo or close, it first saves a changed description, changed notes, a valid changed date and a valid changed location. Invalid text is dropped.
- **Order.** Saves for one photo are sent one after another. The panel is updated from the answer to the last one.
- **Sent once.** A value on its way is remembered, so blur followed at once by a photo change does not send it twice.

### The place picker

At the owner's request (#169), the Place section has a **Pick on map** button (small, map-pin icon, tooltip "Choose where the photo was taken on a map") above the Location group. It is disabled, with the view-only reason as its tooltip, in a view-only album. It opens a wide dialog titled "Where was this photo taken?" (or "Where were these N photos taken?") holding a map 480 px high (at most 60 % of the window's height), with the hint "Click the map where the photo was taken, or click another photo to use its place." above it and **Cancel** below.

- **Which photos.** When the photo the panel shows is the grid's current photo and several photos are selected, the whole selection; otherwise that photo alone.
- **The map** is the Map view's (`ui/src/features/map/photoMap.ts`): the same markers, clusters, zoom and tile layer. It holds every located photo of the album other than the ones being changed, from a query of its own with no filter, no search and no stacks, so a filter on the grid does not hide them. It starts on the world view (latitude 20, longitude 0, zoom 2) and fits those photos as the Map view's first fit does; with none it stays on the world.
- **A click on the map** sets the place: the clicked point, its longitude wrapped into −180 to 180, each coordinate rounded to 6 decimals. **A click on another photo's marker** uses that photo's place. A cluster still zooms in.
- **What is saved.** One `photo_set_info(ids, { location: { lat, lon, name } })`, so it is one undo step (Ctrl+Z), like any typed change. `name` is the country at that point from the bundled outlines (`app_country_at`), as choosing a country in the search does; at sea it is null. A typed place name the photos had is replaced. While it saves, the hint reads "Saving the place…"; on success the dialog closes and the panel follows from `album_changed`. A failure shows its message as a toast and the dialog stays.
- **Map tiles** follow `maps_online` exactly as the Map view does (see [Loading map tiles](#loading-map-tiles)): the same card while it is unset, the same switch, and no tile layer until the user says yes. Without tiles the markers sit on the plain background and a click still works.
- **Cancel**, Esc or the dialog's close button closes it and changes nothing.

### Show on map

At the owner's request (#183), **Show on map** (small, `map` icon, tooltip "Switch to the map with this photo's marker in the middle") sits beside Pick on map whenever the photo has a place: GPS, or a typed place with coordinates (the effective `place.lat` and `place.lon` are set). It works in a view-only album too, as it changes nothing.

- **What it does.** It switches to the [Map view](#map-view), saving the setting `view` as the view switch does, and the map centres this photo's marker at the zoom it has, opening a cluster that holds it. The photo is the page's current photo, so its marker is already the highlighted one (the `--select` border). Already on the map, it only centres.
- **When.** The map centres the marker once an answer of `query_map` holds it, after the first fit, which the centring replaces. If the answer does not hold the photo (a filter in the map leaves it out), the request is dropped and the map behaves as usual. The request lives in `ui/src/features/map/focus.ts`.

## Timeline view

The timeline shows the same photos as the grid, in date order, with a heading over each day or month.

```text
+-----------------------------------------------+
| [Grid|Timeline|Map]            [ Day | Month ]|   view switch, unit switch at the right
+-----------------------------------------------+
| Tuesday 14 July 2026 · 37 photos          |2026|   sticky heading (32px); month rail at the right (44px)
| [tile][tile][tile][tile]                  | Jan|
| [tile][tile][tile][tile]                  | Feb|
| ...                                       |>Jul|   the month in view is marked
| Wednesday 15 July 2026 · 12 photos        |2027|
| [tile][tile][tile][tile]                  | Mar|
| ...                                       |    |
| No date · 5 photos                        |    |
| [tile][tile][tile]                        |    |
+-----------------------------------------------+
```

### Rules

| Topic | Rule |
|---|---|
| Order | Always the effective date, oldest first; photos with the same date are in the name order of [Organising](05-organising.md#sorting). Photos with no date come last. The Sort control (see 05) is shown only in the grid (owner's feedback, #166 and #182), not in the timeline or the map. The chosen sort is kept and applies again in Grid, and to the order the map's photos are stepped through. |
| Filters, search, stacks, hidden photos | Exactly as in the grid. A stack sits where its face sorts. |
| Which date | The effective date: the typed date, else the camera's. |
| Grouping | By the written calendar date: the first 10 characters (`YYYY-MM-DD`) for Day, the first 7 (`YYYY-MM`) for Month. The date is never converted through a time zone, so a photo taken at 23:30 abroad stays on its day. |
| No date | One last group, headed "No date". |
| Unit | A **Day / Month** segmented control, labelled "Group by". Until the user picks, the unit is automatic: Month when the dated photos in the result span more than 90 calendar days from the first one's day to the last one's, Day otherwise (2026-01-01 to 2026-04-01 is 90 days and stays Day). The switch shows the unit the result used. A pick is saved in the setting `timeline_unit` and used for every album. |
| Heading text | Day: "Tuesday 14 July 2026 · 37 photos". Month: "July 2026 · 412 photos". No date: "No date · 5 photos". One photo reads "1 photo". The count is the photos in the group that pass the filter, with stacks unfolded. |
| Sticky headings | The heading of the group at the top of the view stays at the top while its group scrolls. The next heading pushes it out. |
| Clicking a heading | Selects every photo in the group and makes the first one the current photo. Ctrl-click adds the group to the selection. The heading is a button; Enter and Space do the same. |
| Keys | The same as the grid. Left and Right step in timeline order; Home and End go to the first and last photo. |
| Empty | "No photos match the filter", as in the grid. |

### Layout

`query_run` with `view: "timeline"` answers with `groups` and `unit` (fields of `QueryResult`, in [Organising](05-organising.md#query_run)). `groups` is in order: each is `{ key, start, count, photos }`. `key` is `"2026-07-14"`, `"2026-07"` or `"none"`. `start` is the index of the group's first tile and `count` the number of tiles in it; `photos` is the number for the heading. The page builds labels from `key` in its text module and lays out rows from the groups alone, without needing any row:

```text
for each group, in order:
  one heading row, 32px high
  ceil(count / cols) tile rows, each `stride` high (cols, side and stride as in the grid)
  8px of space after the group
tops[] holds each row's top; the visible rows are found by binary search on tops[]
```

Tiles, placeholders, block fetching, overscan, thumbnail size, the very-tall-content rule and keeping the current photo in view all work as in the grid, with a row's top read from `tops[]`. When bringing a photo into view, the 32px sticky heading counts as covered space at the top. Grouping and layout of 100,000 photos over ten years in Day mode (about 3,650 headings) must take under 50 ms in the page.

### Month rail

A narrow column (44px) down the right edge of the timeline, beside the scroll box (which narrows by as much), a `navigation` region labelled "Jump to a month". Each line is 20px high; a month button is named "July 2026" and the marked one has `aria-current`.

- It lists each year that has photos, in bold, followed by the months of that year that have photos, as three-letter names ("Jan"). "No date" is not listed.
- A click scrolls so that month's first heading is at the top. In Day mode that is the first day of the month that has photos.
- The month holding the top of the view is marked with `--accent` text.
- When the list is taller than the column, months are left out and only years show; a click on a year goes to its first month. If the years alone are still too tall, the rail scrolls by itself.
- The rail is not shown when the result has fewer than two months.

## Map view

The **Map** button replaces the thumbnails with a map. The preview and the details panel stay.

```text
+--------------------------------------------+
| [Grid|Timeline|Map]                        |   view switch (padding 8px 12px)
+--------------------------------------------+
| 3 photos have no location                  |   note (12px, --panel), only when some lack one
+--------------------------------------------+
| MAP STAGE (fills the rest; --panel-2)      |
| [+]                         [Map: online]  |   zoom control; toggle top-right, 10px in
| [-]        (12)    (o)(o)                  |   cluster bubble 40px; photo markers 36px, round
|                 (o)                        |   current photo's marker: 3px --select border + white ring
|     +--------------------------------+     |
|     | The map is downloaded from     |     |   first-use card, centred,
|     | OpenStreetMap over the         |     |   width min(360px, 100% − 24px), padding 16px
|     | internet. … Load maps from     |     |
|     | the internet?                  |     |
|     |           [Load maps] [Not now]|     |
|     +--------------------------------+     |
|        [ Map unavailable offline ]         |   status pill, top-centre, when a tile fails
|              © OpenStreetMap contributors  |   attribution
+--------------------------------------------+
```

### What the map holds

The map is the one view that needs every matching photo at once, so it does not use row windows. With `view: "map"` the result is every photo that passes the filter, in the chosen sort order, with stacks unfolded. The page then calls `query_map(token)`, which answers with `{ points, unlocated }`. `points` is one compact entry per located photo: `[id, lat, lon, thumb_key, file_name, edit_plan]` (the file name is the marker's tooltip; `edit_plan` is the stored edit's plan, or null, which the marker's thumbnail is drawn through). For 100,000 photos that is a few megabytes, sent once per result.

- **Located** means the effective place (typed, else GPS) has a latitude from −90 to 90 and a longitude from −180 to 180.
- **Note.** "1 photo has no location" or "N photos have no location", shown only when `unlocated` is above 0.
- **Empty.** When no photo of the result has a place, the stage shows the empty state "No photos with a location".
- **When the result changes**, markers whose id, place and `thumb_key` are unchanged are kept; the rest are added or removed. The map does not move.

### The map

- Zoom from 1 to 19, a zoom control, wrapping across the date line, and no keyboard handling of its own (the arrow keys belong to stepping through photos).
- The first view is latitude 20, longitude 0, zoom 2.
- **First fit.** When the first points of an album arrive, the map fits all of them with 24px of padding, zooming in no further than 15, without animation. It fits again only for another album.
- **Markers.** One per located photo: a 36px round picture of the small thumbnail with a 2px white border and a shadow. The thumbnail is loaded only when the marker is drawn. The tooltip is the file name. A click makes the photo the current photo and the whole selection; Ctrl-click adds or removes it. A right-click opens the photo menu, as on a tile (see 06).
- **Current photo.** Its marker has a 3px `--select` border and a white ring, and is drawn above the others. When the current photo changes and its marker is off screen, the map pans to it and keeps the zoom. If it is inside a cluster, the cluster is opened far enough to show it.
- **Clusters.** Markers within 48px join into a cluster: a 40px `--accent` circle with the count in bold and a 3px white border. A click zooms in to split it.
- **The library's default marker image is never requested.**
- **Many markers.** Markers are made 20,000 at a time, and the cluster group adds each batch in slices of about 100 ms with a short pause between, so a whole album's markers (100,000 take a few seconds to cluster) never hold the page up for long. Points that arrive while a load runs wait for it to end, and the newest wins; a pan to the current photo asked for meanwhile happens when it ends (#146).

### Loading map tiles

Map tiles come from the internet, so they are off until the user says yes. The answer is the setting `maps_online` (true, false, or unset).

| State | What shows |
|---|---|
| Settings not yet read | Nothing extra. |
| Unset | The card, with the exact text "The map is downloaded from OpenStreetMap over the internet. This shows OpenStreetMap roughly where you are looking. Load maps from the internet?" and the buttons **Load maps** (primary) and **Not now**. |
| True or false | A small button, **Map: online** or **Map: offline**, that switches it. Its tooltip is "Stop loading maps from the internet" or "Load maps from the internet". |

- **Switching off** takes effect at once, then is saved. **Switching on** is saved first, then takes effect. If the save fails, the toast "Couldn't change the map setting." shows and tiles stay off.
- **Tiles** are requested only while the setting is true, from `https://tile.openstreetmap.org/{z}/{x}/{y}.png`, with no referrer, up to zoom 19, with the attribution "© OpenStreetMap contributors".
- **A tile that fails** shows the pill "Map unavailable offline" (`role=status`). The next tile that loads hides it.
- **Without tiles**, markers sit on a plain `--panel-2` background. Everything else works.

### Selection and keys on the map

Left, Right, Home and End step through the map's result, in its order, whether or not each photo has a place. The preview and details follow. Enter and Space open the viewer on the current photo. Switching back to Grid or Timeline keeps the current photo and the selection; if the current photo is folded inside a stack there, stack rules make it visible (see 05).

## Keys

One table for the whole review screen and the viewer. "Owner" names the part that defines what the key does; this part defines only the rows marked 04. Which key reaches which action is held in one file, `ui/src/keys/keymap.json`, whose rows are listed in [Desktop, release and tests](08-desktop-and-release.md#every-key); this table and that one must agree exactly.

**When keys are ignored.** Outside the viewer, a key is ignored when a dialog is open, when a photo is being edited (edit mode has its own keys, see 06), or when the event was already handled. "Typing" means focus is in a text box, text area, drop-down or dialog. "On a control" means typing, or focus on a button or link. Keys with Alt or Meta are always ignored. Keys with Ctrl are ignored except those listed.

| Key | Review screen | Viewer | Repeats when held | Owner |
|---|---|---|---|---|
| Left, Right | Previous or next photo in the result, stopping at the ends. The photo becomes the current photo and the whole selection. Not while typing. | The same. | Yes | 04 |
| Shift+Left, Shift+Right | Move the current photo one place and select the range from the anchor to it. Not while typing. | — | Yes | 04 |
| Home, End | First or last photo. Not while typing. | The same. | No | 04 |
| Enter, Space | Open the viewer on the current photo. Ignored on a control or with no current photo. | Enter closes. Space plays or pauses a video and does nothing for a still photo. | No | 04 |
| Esc | Closes the photo menu if open. Otherwise, with more than one photo selected, reduces the selection to the current photo. | Back to fit if zoomed in; otherwise close. | No | 04 |
| Ctrl+A | Select every photo shown in the result. Not while typing. | — | No | 04 |
| Ctrl+D | Select nothing. Not while typing. | — | No | 04 |
| T | Hide or show the thumbnails (see [Hiding and moving the panels](#hiding-and-moving-the-panels)). Not while typing. | — | No | 04 |
| I | Hide or show the details panel. Not while typing. | Show or hide the viewer's details panel. | No | 04 |
| `+` (or `=`), `-` | — | Zoom in or out by 1.2 about the centre. | Yes | 04 |
| `0` | — | Back to fit. | No | 04 |
| S, F | Star or unstar. Both keys do the same thing everywhere. Not while typing. | The same, for the shown photo, with the star mark. | No | 05 |
| H | Hide or unhide. Not while typing. | The same, for the shown photo. | No | 05 |
| Up, Down | Nudge the photo score by +1 or −1. They never move the selection. Not while typing. | See 05. | No | 05 |
| Ctrl+Z | Undo. While typing, the text box's own undo. | See 05. | No | 05 |
| E | Start editing the current photo (edit mode). Not while typing. | — | No | 06 |
| `[`, `]` | Rotate left or right: every selected photo that is shown. Not while typing. | The same, for the shown photo. | No | 06 |
| `\` (hold) | Before/after: show the original while held. | The same. | Hold | 06 |
| Ctrl+C | Copy: the picture for one photo, the files for several. While typing, the text box's own copy. | Copy the shown photo. | No | 06 |
| Delete | Move the selected photos to the Recycle Bin or Trash, after the confirm. Not while typing. | — (left out of the viewer on purpose; see 06) | No | 06 |
| Esc, Enter, `[`, `]`, `\` (in edit mode only) | Cancel, Done, rotate left, rotate right, and before/after. Edit mode takes these keys before anything in this table, and no other review key acts while it is open. | — | No | 06 |
| `?` | Open or close the shortcut overlay. Not while typing. | The same. | No | 08 |
| Ctrl+O, Ctrl+W | Open folder, close folder (menu accelerators). | The same. | No | 08 |
| F1, F2 | About Owlbum; send feedback (menu accelerators). | The same. | No | 08 |
| F6, Shift+F6 | Move focus to the next or previous region. | The same. | No | 09 |

This part reserves no key beyond its own rows. The 05, 06, 08 and 09 rows are listed so the table is complete; where this table and the owning part differ, the owning part wins and both tables are corrected. The one rule this part does fix for other owners is that **S and F are always the same key**, on the review screen, in the viewer and in the photo menu's hint. Ctrl+Q (quit) and F11 (full screen) are window keys listed only in 08.

Rules for the 04 rows:

- **With no current photo**, Left, Right and Home select the first photo of the result, and End the last.
- **Stepping needs a row.** The page moves the current photo's index at once. If the row at that index has not been fetched, it asks for its block and shows a placeholder in the preview until the row arrives. A held key keeps moving the index; only the block for where it ends up is still wanted when the key is let go.
- **The photo menu** closes on any key in the table, and the key then acts. While focus is in the menu, its own keys (arrows, Home, End, Enter, Space, Esc, Tab) move and choose inside it instead (see [Desktop, release and tests](08-desktop-and-release.md#the-key-table)).
- **In the viewer** every key in the table has its default action blocked, as have Tab, PageUp and PageDown, and no key reaches the page behind.
- **Letters** act the same in either case.

## Toasts and other dialogs

**Toasts** follow the Toast component of [Visual design](09-visual-design.md#toasts): fixed at the bottom centre, 16px above the edge, stacked with an 8px gap (`--space-3`), newest at the bottom, at most 3 on screen (the oldest goes first).

- Each is `--panel-3` with a 3px left edge and a matching icon: `--select` for information, `--error` for an error, `--ok` for success. Maximum width 620px (`--size-dialog-wide`), `--text-md`.
- Its **x** dismisses it. An information or success toast goes by itself after 4,000 ms, or 8,000 ms when it carries an action such as Undo; an error stays until dismissed. The timer pauses while the pointer is over the toast or focus is on it.
- An identical toast already on screen is not repeated; its timer restarts. (Not built yet: an identical toast is shown again; see #69 final pass.)
- Toasts are in a `role=status` region; errors use `role=alert`.

Toasts this part raises:

| Text | When |
|---|---|
| "Couldn't open <folder name>: <message>" | `album_open` failed. |
| "The last folder isn't available: <folder name>" | Reopening at start found the folder gone. |
| "Drop a folder to open it." | A drop held nothing that could be opened. |
| "Couldn't change the map setting." | Saving `maps_online` failed. |
| "This album is view-only." | A changing key was pressed in a view-only album. (Not built yet: see #69 final pass.) |
| The error's `message` | Any failed save of a typed field, and any other failed command with no wording of its own. |

**Dialogs** are centred on `--backdrop`, 440px wide (the small size) unless stated wide (620px), with the title at the top and buttons right-aligned at the bottom, the primary one last. Esc and a click on the backdrop do what Cancel does. Focus starts on the safest button (Cancel, or Close) and returns to where it was when the dialog closes. Keys inside a dialog do not reach the page behind. This part defines three: the stale-lock question and the damaged-database question (see [Opening a folder](#opening-a-folder)), and **Files not shown** (see [Notices](#notices)). For the stale-lock question the safest button is Open view-only. The close question that is asked only when `edit_target` is `file` belongs to 06 and is a native box; Tidy belongs to 05.

## Reopening where I was

Owlbum starts where the person left off: the same folder, view, sort, filter, photo and scroll position.

### What is kept

Per recent folder, in `recent.session` in `app.sqlite` (see [Data](02-data.md)):

| Field | Meaning |
|---|---|
| `view` | `grid`, `timeline` or `map`. |
| `sort` | The chosen sort (see 05). |
| `filter` | The filter (see 05). |
| `search` | The search text (see 05). |
| `selected` | The current photo, stored as its path relative to the album root. The page sends and receives an id; Rust converts. |
| `scroll` | The index of the first photo in the top row of the grid or timeline. An index, not pixels, so it still fits after the window or thumbnail size changes. |

Kept for all albums, as settings: `thumb_size`, `panel.grid_width`, `panel.details_width`, `panel.grid_shown`, `panel.details_shown`, `panel.grid_place`, `panel.strip_height`, `sections`, `stack_similar`, `timeline_unit`, `maps_online`, `reopen_last`, and `view` (the view used for an album that has no session yet).

The session carries the `Query`'s own fields under their own names, so a field the `Query` gains later is kept with no change to the session. The `Query` fields that are not kept are `open_stacks` and `want_facets` (forgotten on close) and `timeline_unit` and `stack` (settings for every album).

Not kept: the rest of a multi-selection, open stacks, zoom, whether the viewer was open, and the map's position.

### Saving

The page calls `app_session_save(session)` 1,000 ms after the last change to any of these fields, and at once when the album is closed, another is opened, or the window is asked to close. On a close the shell emits `app_closing`; the page saves the session and then answers with `app_close_ready`, within the 3 seconds that [Desktop, release and tests](08-desktop-and-release.md#closing-and-quitting) allows. Saving also moves the folder to the top of the recent list.

### At start

The page's first call is `app_startup`, defined in [Desktop, release and tests](08-desktop-and-release.md#commands-and-events-this-part-adds). Among other things it answers with the settings, the recent list, the app version, `launch_folder` (a folder from the command line, or null) and `reopen` (the folder to reopen, or null). A folder given on the command line arrives this way and never as an `open_folder` event, so it cannot arrive before the page listens.

| Situation | What happens |
|---|---|
| A folder was given on the command line | It opens. Its own saved session, if it has one, is restored. |
| A picture was given on the command line (`launch_photo`) | Its folder opens with that picture current, as in [Opening a picture](#opening-a-picture). |
| `reopen_last` is false | The start screen. |
| The recent list is empty | The start screen. |
| The newest recent folder exists | It opens and its session is restored. |
| The newest recent folder is not found | The start screen, with that entry greyed out, and the toast "The last folder isn't available: <folder name>". |
| A second launch passes a folder while the first is starting | It wins: the `open_folder` event opens it. |

The check for whether a folder exists must never hold up the start. Rust checks all recent folders at once and gives each at most 1.5 seconds; one that has not answered counts as not found.

**Settings › Reopen the last album at start** (on by default) is the `reopen_last` setting. The dialog itself is defined in 08.

### Restoring a session

`album_open` answers with `session` when the folder has one: `{ view, sort, filter, search, selected_id, scroll }`. `selected_id` is null when the saved photo is no longer in the album (no row with that path, or the row is missing). Rust checks the stored values on the way out: each `Query` field and each filter is tried on its own, and one that is not allowed (from a newer or older Owlbum) is dropped, so a stored session can never stop the folder opening; a session that cannot be read at all opens the folder without one.

1. The page sets the view, sort, filter and search, and runs the query, before drawing the review screen. The start screen is not shown in between.
2. If `selected_id` is set and `query_locate` finds it in the result, it becomes the current photo and is brought into view. That is the scroll position: by photo, not by offset. (`keep` is not used for this: its settling replaces a missing photo with its neighbour.)
3. If nothing is selected, or the photo is gone, the photo at index `scroll` (clamped to the result) becomes current once its row arrives and is brought into view, so the album opens where it was, with no message. This replaces the first photo of **First selection** (see [When the result changes](#when-the-result-changes)).
4. If the album is still being scanned and the selected photo's row exists from an earlier scan, step 2 works at once, because rows persist in the album database.
5. A filter that now matches nothing is still restored; the filtered row's **Clear filter** is the way out.
6. A photo the user picks before the restore settles wins. Saving starts once the restore has settled, so a quick close never overwrites the stored session with the defaults.

Opening a folder from the recent list, by drop, or from the dialog restores its session in the same way.

## Commands and events

Names follow the prefixes in [Architecture](01-architecture.md). Every command answers `Result<T, AppError>`. Commands owned by other parts are not repeated here.

### Commands

| Command | Arguments | Answer | Notes |
|---|---|---|---|
| `app_startup` | none | Defined in [Desktop, release and tests](08-desktop-and-release.md#commands-and-events-this-part-adds). This part uses `version`, `settings`, `recent`, `launch_folder`, `launch_photo` and `reopen`. | Called once when the page loads. |
| `app_settings_get` | none | `Settings` (every key in the table in [Data](02-data.md#appsqlite), with defaults filled in) | |
| `app_settings_set` | `patch` (keys to values) | `Settings` | Unknown keys are kept. A value outside its range answers `setting_out_of_range` and changes nothing; the page clamps the thumbnail size and the column widths itself before it sends them. |
| `app_recent_list` | none | `Recent[]`, newest first | `Recent = { root, name, opened_at, exists }`. |
| `app_recent_remove` | `root` | `Recent[]` | |
| `app_pick_folder` | none | `string \| null` | Shows the native folder dialog. Defined in 08; listed here because this part calls it. |
| `app_session_save` | `Session` with `selected_id` | nothing | Stores against the open album's folder. A no-op when no album is open. |
| `app_place_search` | `text` | `CountryMatch[]` = `{ name, lat, lon }[]`, at most 6 | Countries whose name contains the text, in the order [Country search](#how-typed-fields-save) gives; empty text answers `[]`. |
| `app_country_at` | `lat`, `lon` | `string` or null | The country that holds the point, from the same outlines; null at sea. The [place picker](#the-place-picker) uses it as the place name. |
| `album_open` | `path`, `options?: { take_lock?, view_only?, restore_backup? }` | `{ status: AlbumStatus, session: Session \| null, notices?: string[] }` | `notices`, left out when empty, are sentences shown as error toasts, such as a save into a file that could not be finished (see 06). Closes any open album first. It returns as soon as the database is open and starts the scan in the background. Errors: the shared `not_a_folder` and `folder_unreadable`, and this part's `album_lock_stale` and `album_damaged`, each with a `details` object. |
| `album_close` | none | nothing | Releases the lock. Drops every token. Like `album_open`, it runs off the main thread, and the two never overlap: an open waits for a close, so the old album's lock is gone before the new one is taken. |
| `album_status` | none | `AlbumStatus \| null` | Null when no album is open. |
| `query_ids` | `token`, `offset`, `limit` | `number[]` | The ids at those places of the token's list, in order: one id per tile, so the face of a closed stack and not its folded members, and the meaning segment when the range reaches it. For Shift ranges, heading clicks and Ctrl+A. No limit on `limit`. Error: `query_expired`. |
| `query_map` | `token` | `{ points: [id, lat, lon, thumb_key, file_name, edit_plan][], unlocated }` | For a token made with `view: "map"`. Error: `query_expired`. |
| `photo_detail` | `id` | `PhotoDetail` | Error: `photo_not_found`. |
| `photo_find` | `path` | `number \| null` | The id of the photo whose file is `path` (a full path) in the open album, or null when it is not there or not scanned yet ([Opening a picture](#opening-a-picture)). On Windows the name matches whatever its case. Error: `no_album`. |
| `photo_exif` | `id` | `ExifField[]`, `{ group, name, value }` | Every EXIF tag of the file ([EXIF section](#exif-section)). Error: `photo_not_found`. |
| `photo_set_info` | `ids`, `patch` | `{ changed: number[], revision }` | Validates again in Rust; errors `invalid_date`, `invalid_location`, and the shared `view_only` and `photo_not_found`. `changed` lists the photos whose values changed. The patch fields `star` and `hidden` are defined in 05. |
| `photo_histogram` | `id` | `{ l, r, g, b }`, each 64 numbers | |

Commands this part calls and other parts define: `query_run`, `query_window` (at most 500 rows a call; this part asks for blocks of 200) and `query_locate` in [Organising](05-organising.md#commands-and-events), with the shapes `Query`, `Keep`, `QueryResult`, `Kept`, `Group` and `Row`; `album_people` there too; `album_rescan`, `album_scan_report`, `album_scan_errors`, `album_unsupported` and `album_copy_not_shown` in [Media and scanning](03-media-and-scanning.md#commands-events-and-urls); `search_meaning` in [Tagging](07-tagging.md#commands-and-events).

### Events

The names and payloads are those in [Architecture](01-architecture.md#events).

| Event | Payload | What this part does with it |
|---|---|---|
| `scan_progress` | `{ phase, found, done, failed, finished, cancelled }` | Draws the reading bar. On `finished` (which is also sent for a cancelled scan, with `cancelled: true`), hides it and reads `album_status` for the notices. |
| `album_changed` | `{ revision, reason, rekeyed? }` | Re-runs the query when the reason calls for it (see [Running a query](#running-a-query)), refreshes the details panel, drops cached pictures of the photos in `rekeyed`, and, when `reason` is `status`, reads `album_status` for the banners. |
| `open_folder` | `{ path, photo }` | Opens the folder, as described in [Opening a folder](#opening-a-folder). `photo` is a picture a second launch named, or null; see [Opening a picture](#opening-a-picture). |
| `folder_drop` | `{ phase: 'over' \| 'left' \| 'refused' }` | Shows the drop outline while `over`, hides it on `left`, and shows the toast "Drop a folder to open it." on `refused`. |

### Shapes

```ts
type AlbumStatus = {
  root: string;              // absolute path of the album root
  scope: string;             // path of the opened folder relative to the root, with '/'; '' when the root itself was opened
  folder: string;            // the opened folder, absolute: what the recent list holds and what reopening passes
  name: string;              // the opened folder's own name, for messages
  view_only: null | 'read_only' | 'locked' | 'chosen' | 'newer' | 'damaged';   // the five reasons in Data
  in_memory: boolean;        // true when there is no database file and nothing is saved (a read_only folder with no album)
  lock_host: string | null;  // the other computer, for 'locked' and 'chosen'
  conflict_file: string | null;
  scanning: boolean;
  photos: number;            // photos in the scope that are not missing and could be read, hidden ones included
  unsupported: { count: number; labels: string[] };   // files recognised but not supported; they have no row
  unreadable: number;        // rows in the scope with a scan_error
  scan: object;              // { phase, found, done, failed, watching, unsupported, skipped }: defined in Media and scanning
  revision: number;
};

// Query, Keep, QueryResult, Kept, Group and Row are defined in Organising and are not repeated here.

type PhotoDetail = {
  id: number;
  rel_path: string; rel_dir: string; file_name: string;
  abs_path: string;          // for tooltips only; never sent back
  kind: 'image' | 'raw' | 'video'; format: string;
  width: number; height: number; size: number; duration_ms: number | null;
  thumb_key: string;
  date: { effective: string | null; camera: string | null; typed: string | null };
  place: {
    lat: number | null; lon: number | null;          // effective
    gps: { lat: number; lon: number } | null;         // from the file
    typed: boolean; name: string | null; country: string | null;
  };
  camera: {
    camera: string | null; lens: string | null; focal_mm: number | null; focal35_mm: number | null;
    aperture: number | null; shutter_s: number | null; iso: number | null; bias_ev: number | null;
    flash: boolean | null;
  };
  clipping: { shadows: number; highlights: number } | null;   // fractions, 0 to 1
  star: boolean; hidden: boolean;
  people: string[]; description: string | null; notes: string | null;
  revision: number;          // the album revision when this detail was read (read before the row, as query_run does)
  edit_plan: object | null;  // EditPlan for the stored edit, as on Row; defined in 06
  edit: object | null;       // PhotoEdit, the stored edit itself, where edit mode starts (06); null for none
  tags: object | null;       // PhotoTags; defined in 07; null until "What's in it" (#55), which hides its section until then
  // The score breakdown is not here: it is read with photo_score (05).
};

type Session = Partial<Query> & {   // the Query's own fields (05), less open_stacks,
                                     // want_facets, timeline_unit and stack
  selected_id: number | null;
  scroll?: number;
};
```

## Tests

### Rust tests (`owlbum-core`, with temp folders and the fixture albums)

- **Query windows.** `query_window` returns the right rows for offsets at the start, middle and end; a limit past the end returns the tail; an unknown or dropped token answers `query_expired`.
- **Locate and ids.** `query_locate` finds every photo of a result and answers null for one filtered out. `query_ids` matches the ids of the same range fetched as rows.
- **Counting.** `total` leaves out hidden photos under the default filter and includes them when the filter shows hidden. A folded stack of 5 counts as 5 in `total` and in each timeline group's `photos`. Missing, unsupported and unreadable files are in no count.
- **Settling.** With `keep`: a photo still present keeps its id and gets its new index; a removed photo is replaced by the one now in its place; removing the last photo selects the new last; an empty result gives null; an expired old token falls back to the old index. `kept.selection` holds only photos still present.
- **Timeline groups.** A photo at `2026-07-14T23:30:00` is in day `2026-07-14` whatever the computer's time zone. Three photos on two days plus one undated give keys `2026-07-14`, `2026-07-15`, `none` with the right `start`, `count` and `photos`. A span of 90 days gives Day and 91 gives Month under `auto`. Order is oldest first with ties in name order, whatever sort is set. 100,000 photos over ten years group in under 100 ms.
- **Map points.** Only photos with a valid effective place are in `points`; a typed place wins over GPS; `unlocated` is the rest; stacks are unfolded.
- **Typed info.** Each date form in the date table is accepted or refused as stated; 30 February and year 1700 are refused; a date equal to the camera's clears the override. Each location rule in the table. People: trimming, case-insensitive de-duplication, first spelling wins across photos. Empty text is stored as NULL. Every change answers `view_only` in a view-only album and writes nothing.
- **Photo detail.** Effective date and place follow the override rules; `country` is right for a point inside a country and null in mid-ocean; a video has `duration_ms`; an unknown id answers `photo_not_found`.
- **Country outlines.** Cities, points a few kilometres either side of a border, enclaves (Lesotho, San Marino, Vatican City, Monaco), mid-ocean (none), the sea near a coast, both sides of the antimeridian and longitude ±180, the poles, bad input, and each disputed area named in [Which country](#how-typed-fields-save). Every country's central point is inside it. 100,000 lookups take about half a second in an unoptimised build. The search's matching, order and limit.
- **Histogram.** A pure red fixture puts every red count in the last bin; an edited photo's histogram equals the unedited file's, because the histogram describes the file.
- **Album status.** Each row of the locking table in [Data](02-data.md) gives the matching `view_only` value; a conflict-named file beside the database sets `conflict_file`; a scoped open reports `root` and `scope`; a failed `quick_check` answers `album_damaged`, and `restore_backup` renames the damaged file and opens for writing.
- **Problems.** The fixture's unsupported and unreadable files are counted, listed by `album_unsupported` and `album_scan_errors`, and absent from every result.
- **Sessions.** Save then open round-trips all five fields; `selected` survives a rename of the file made outside Owlbum (it follows the row); a deleted photo comes back as `selected_id: null`; the recent list keeps 12 and orders newest first; `app_recent_remove` removes one entry.
- **Start.** `app_startup` gives `reopen` for an existing newest folder, null when `reopen_last` is false, null when the folder is gone, and `launch_folder` wins. A folder on an unreachable path does not delay the answer beyond 1.5 seconds.
- **Opening.** A path to a file opens its folder; opening a subfolder of an album answers a scoped status; opening a new folder creates the database.

### Page tests (Vitest, against the faked API)

- **Grid maths.** Columns, side, stride, total, first and last index for several widths and sizes; the scaled mapping above `MAX_SCROLL` reaches the first row at 0 and the last row at the bottom.
- **Windowing.** With a fake result of 100,000 rows: only tiles in view plus overscan are in the document; scrolling asks for the right blocks, viewport first; a jump to the end asks for the last blocks and not the ones passed over; no more than 3 requests are in flight; no more than 30 blocks are held; the current photo's block is never dropped.
- **Placeholders.** Tiles without rows are placeholders that ignore clicks; they become pictures when the block arrives.
- **Re-running.** An `album_changed` event re-runs the query with `keep`; old rows stay until new ones arrive; a late answer to an older call is ignored; `query_expired` causes exactly one re-run and retry; events during a scan cause at most one re-run per second.
- **Body states.** "Looking for photos…" while the scan runs with no photos; "No photos in this folder." after it; "No photos match the filter" only when the album has photos. The text "No photos match the filter" never appears between `album_open` and the first row. During a first scan, the review screen and its first tiles appear as soon as a `query_run` answer has `album_photos > 0`, before any `scan_progress` event with `finished: true`.
- **Counts.** The filter bar shows `total` from the answer; a hidden photo does not change it under the default filter.
- **Selection.** Every row of the click table; a Shift range across rows not fetched calls `query_ids` once; Ctrl+A; Esc reduces the selection; a heading click selects its group.
- **Keys.** Every 04 row of the key table, including: S and F both star on the review screen and in the viewer; keys are ignored while typing and in dialogs; Left with no current photo selects the first; a held Right does not pile up block requests; the viewer blocks keys from the page behind.
- **Preview.** The medium picture is shown when cached, else this photo's small one; never another photo's picture; a new photo starts fitted; wheel, drag, double-click and clamping follow the zoom rules; the full-size picture is asked for only past the medium picture's pixels; the mini map shows and its rectangle matches the view.
- **Preloading.** `preloadOrder` at both ends of a result, in both directions, and for short results; the cache respects the in-flight cap, cancels unwanted loads, evicts beyond 40 and never evicts a wanted picture; after opening the viewer the next 20 and previous 5 are asked for, the next one first; stepping to a preloaded photo asks for nothing new.
- **Viewer.** It shows the edited picture for a row whose `edit_plan` is not null; zoom works; Esc goes to fit, then closes; closing selects the last photo shown; when the shown photo leaves the result the next one takes its place; an empty result closes it.
- **Details and preview agree.** After any re-run, both show the same id; a `photo_not_found` answer triggers a re-run; a detail answer for a photo no longer current is ignored.
- **Details fields.** Each row of the save table; "Saved" shows for 1,800 ms; a failed save keeps the text and shows a toast; a draft survives an answer from an earlier save; changing photo with unsaved text sends it once; every date and location message; everything is disabled in a view-only album.
- **Timeline.** Layout rows and tops for a group of 9 at 4 columns (one heading, rows of 4, 4 and 1); binary search returns only rows in view; heading labels in Day and Month; the Sort control is not shown; the month rail lists only months with photos, marks the month in view, and collapses to years when too tall.
- **Map.** The first-use card's exact text; no tile request is made before **Load maps**; "Not now" leaves markers on a plain background; a failed setting save shows the toast and keeps tiles off; the note's singular and plural; a marker click selects; the viewer, a dialog and a toast are each above the map stage (computed stacking order).
- **Banners and notices.** Each `view_only` value shows its exact sentence; the conflict banner names the file and can be dismissed; a scoped status offers Show whole album; the window title names the open folder; notice texts for 1 and many files and the list of formats; the Files not shown dialog lists what `album_unsupported` and `album_scan_errors` return.
- **Opening.** Each route calls `album_open` once; a cancelled dialog does nothing; `album_lock_stale` and `album_damaged` show their dialogs and call again with the chosen option; a failed open shows the toast and stays on the start screen; recent entries that do not exist are disabled.
- **Reopening.** With a session in the answer, the view, sort, filter and current photo are set before the review screen is drawn; `selected_id: null` selects nothing; `scroll` is used only without a selected photo; the session is saved 1,000 ms after a change and at once on close.
- **Settings only.** No test finds a `localStorage` or `sessionStorage` call in the page; thumbnail size, column widths and sections are written through `app_settings_set`.
- **Video.** A video tile shows its length; nothing plays until asked; stepping away stops playback; a player error shows the cannot-play message.

## Open questions

1. **The height limit in each webview.** `MAX_SCROLL = 8,000,000` px is a safe guess below the known limits of the two engines. The real limits of WebView2 and WebKitGTK need measuring, and the scaled-scroll path needs trying with a touchpad on both.
2. **Dropping a folder on Linux.** Folder drops are expected to work through the shell on both platforms, but this has not been tried on WebKitGTK, nor together with drag-out of photos. If it cannot be made reliable, it ships on Windows only, as the Linux rule in the README allows.
3. **Memory for decoded pictures.** Forty medium pictures is about 400 MB on a large screen. Whether `CAPACITY` should shrink on machines with little memory, or the cache should hold pictures in a cheaper form, needs measuring on real hardware.
4. **Video in the preview on Linux.** [Architecture](01-architecture.md) notes a known problem with video over a custom scheme on Linux. If the fallback in 03 cannot play inside the page, the preview and viewer show the poster and the cannot-play message there, with **Show in folder**.
5. **Opening a file rather than a folder.** Answered (owner's feedback, #184): a picture named by a launch (command line, second launch, Explorer) opens its folder with that picture current and selected; a dropped file only opens its folder. The page finds the picture's id with `photo_find` after `album_open`, so `album_open` itself is unchanged. See [Opening a picture](#opening-a-picture).
6. **Timeline order.** Oldest first is specified. Some people expect newest first in a timeline. A setting is possible but not planned.
