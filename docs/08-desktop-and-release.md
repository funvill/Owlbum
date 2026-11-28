# Desktop, release and tests

This part covers what makes Owlbum a desktop program rather than a page: starting and quitting, the window, menus, the Settings dialog, the keyboard shortcut overlay, updates, About, What's new, logs and failures. It then covers how Owlbum is packaged, released and tested.

Behaviour of the grid, viewer, filters, edits and tagging belongs to parts 4 to 7. Where this part names one of their features it says which file owns it.

## Identity

| Field | Value |
|---|---|
| Product name | `Owlbum` |
| Identifier | `com.funvill.owlbum` |
| Binary name | `owlbum` (`owlbum.exe` on Windows) |
| Repository | `github.com/funvill/Owlbum` |
| Licence | MIT |
| First version | `0.1.0` |
| Description | "Look through folders of photos, star the best ones, find things by what is in them, and drag photos into other programs." |

**The version has one source.** It is `version` under `[workspace.package]` in the root `Cargo.toml`. Both crates inherit it. `src-tauri/tauri.conf.json` has no `version` key, so Tauri reads it from the crate. `ui/package.json` is private and its version is never shown; the page learns the version from `app_startup`.

**Every test build has a new version.** `scripts/build-test.sh` raises the last number of the workspace version, turns the `Unreleased` section of `CHANGELOG.md` into that version's section, commits both to `main` (no tag: a version tag starts a real release, which only a person makes) and builds the exe. `CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/): each change that a user would notice is added under `Unreleased` in the same pull request as the change.

**A development build** is one compiled with debug assertions. It uses the identifier `com.funvill.owlbum.dev`, the name `Owlbum Dev` and the `OwlbumData Dev` / `owlbum-dev` app data folder (see [Data](02-data.md)), so it runs beside an installed copy and never touches its data. The identifier and name come from `src-tauri/tauri.dev.conf.json`, merged by the page's `app:dev` script (`npm --prefix ui run app:dev`); the shell picks the `Dev` app data folder when the build has debug assertions **or** carries an identifier ending in `.dev` (`owlbum_lib::is_dev_build`), so a release-profile build started with the dev config still never touches an installed copy's data.

## Starting

### Command line

```text
owlbum [folder]
```

| Rule | Detail |
|---|---|
| The folder | The first argument that does not begin with `-`. |
| A relative path | Resolved against the working directory of the launch that gave it. |
| A file instead of a folder | The folder that holds the file is opened. |
| Arguments beginning with `-` | Ignored. Owlbum defines no flags in version 1. |
| A path that cannot be opened | Owlbum starts normally and the page shows the `album_open` error (see [Browsing](04-browsing.md)). |

### Which album opens at start

The first rule that applies wins.

| Order | Situation | Result |
|---|---|---|
| 1 | A folder was given on the command line | That folder opens. |
| 2 | `reopen_last` is true and the newest entry in `recent` still exists | That album reopens where it was (see [Browsing](04-browsing.md)). |
| 3 | Otherwise | The start screen. |

### Startup sequence

1. Work out the app data folder: `OWLBUM_DATA_DIR`, else the location in [Data](02-data.md). Create it. Owlbum computes this path itself and does not use Tauri's default folders, which are named after the identifier.
2. Open the log (see [Logs](#logs)) and install the panic hook.
3. Register the single-instance plugin. If another copy is running, hand it the arguments and exit (see below).
4. Open `app.sqlite`. If it is damaged, rename it `app.damaged-<timestamp>.sqlite`, start with a new one, and tell the user once: "Owlbum's settings could not be read and were reset. Your photos and albums are not affected."
5. Log the start line: version, platform, package kind, app data folder, development or not.
6. Build the menu.
7. Register the `owlbum://` protocol and the commands.
8. Create the window, hidden, with the restored geometry.
9. Load the page. The page calls `app_startup` once it has drawn. The reply carries everything the page needs to begin: version, platform, the settings, the recent list, the launch folder if any, the folder to reopen if any, the first-start notice flag, and What's new notes if any are due.
10. Show the window when `app_startup` arrives, or after 3 seconds, whichever is first.
11. Start the update timer (see [Updates](#updates)).

If `app_startup` has not arrived 30 seconds after the page was asked to load, that is a fatal error of phase `page` (see [Failure handling](#failure-handling)).

### Single instance and second launches

There is one window and one running copy per app data folder.

- A second launch sends its arguments and working directory to the first copy and exits with code 0.
- The first copy restores its window from minimised, shows it and gives it focus.
- If the second launch named a folder, the first copy opens it, exactly as File > Open folder would. Opening another album asks no question, because every edit is already in the album (see [Editing and file actions](06-editing-and-file-actions.md)).
- The single-instance plugin is not registered when `OWLBUM_DATA_DIR` is set, so tests can run several copies at once, each with its own data folder.

### Handing a folder to the page

A folder can arrive before the page is listening, so an event alone is not enough.

| Moment | What happens |
|---|---|
| A folder arrives before `app_startup` | It is kept as the pending folder. A later one replaces it. |
| The page calls `app_startup` | The reply includes the pending folder, which is then cleared. |
| A folder arrives after `app_startup` | The `open_folder` event is emitted at once. |

## The window

One window, titled `Owlbum <version>` (for example `Owlbum 0.1.3`), so a tester can always tell which build is running. It has no child windows.

| Property | Value |
|---|---|
| Default size | 1440 × 900 |
| Minimum size | 960 × 600 |
| Background while loading | The theme's window colour (see [Visual design](09-visual-design.md)), so there is no white flash. |
| Webview data folder | `<app data>/webview`. Nothing Owlbum relies on is kept there. |
| Developer tools | Development builds only. |
| Files dropped on the window | Never navigate the webview. Rust receives the drop; a dropped folder is opened through `open_folder` (see [Browsing](04-browsing.md)). |

### Geometry

The `window` setting holds `{ "x", "y", "width", "height", "maximised" }`. The four numbers are the bounds of the window when it is not maximised, in physical pixels.

**Saving.** The shell remembers the bounds whenever the window moves or is resized while it is neither maximised, minimised nor full screen. It writes the setting 2 seconds after the last change and again when the window closes. Full screen is never saved.

**Restoring.**

| Saved value | Result |
|---|---|
| Missing or not an object | Default size, centred. |
| Any number missing or not finite | Default size, centred; `maximised` is still honoured. |
| Width or height below the minimum | Raised to the minimum. |
| Width or height larger than the display it lands on | Reduced to that display's work area. |
| Position fails the off-screen check | The size is kept and the window is centred on the primary display. |
| `maximised` is true | The window is placed, then maximised. |

**Off-screen check.** The position is used only if the title bar can be reached on some display `d`: `x + width > d.x + 100` and `x < d.x + d.width − 100` and `y >= d.y − 10` and `y < d.y + d.height − 50`.

On Linux under Wayland a program cannot read or set its window position. There only the size and the maximised state are restored.

As built (#58): the window is made hidden (`visible: false`), placed, and shown when `app_startup` arrives or 3 seconds after start, whichever is first. The saved bounds are the window's outer position and inner size, which is what Tauri sets back. "The display it lands on" is the display that holds the saved top-left corner, else the primary display. The loading background colour is not set yet.

### Closing and quitting

Closing the window quits Owlbum. Quit, the window's close button, and an update install all go through the same steps.

1. `app_closing` is emitted with an empty payload; the shell keeps the reason (close, quit or update) to itself. The page ends edit mode as Done, saves the session (see [Browsing](04-browsing.md#saving)), waits for a running save as [Editing and file actions](06-editing-and-file-actions.md#closing-and-quitting) describes, and calls `app_close_ready`. If the page has not answered within 3 seconds and no save is running, the close carries on without it.
2. Only when `edit_target` is `file`: the shell asks Rust how many photos have an edit that is not saved into its file (`waiting` from `edit_status`). If the count is above zero it shows the native question whose wording and buttons are owned by [Editing and file actions](06-editing-and-file-actions.md#closing-and-quitting). "Go back" cancels the close. An update install that has already asked does not ask again. With `edit_target = album` Done is the save, nothing is waiting, and no question is asked.
3. The window geometry is saved.
4. The open album is closed: its writer thread finishes, the `lock` file is removed, and tagging and scanning stop.
5. The process exits with code 0.

As built (#47, `src-tauri/src/closing.rs`): the window's close request is held and the steps run on a thread of their own; a second close while they run is ignored. Before step 2 the shell asks a running save to stop after its current photo and waits for it (at most 2 minutes). The native question uses the dialog plugin's two custom buttons, which cannot choose the default button, so "Close anyway" is the default on Windows (Esc is "Go back"), the same limit as the trash confirm (#148). "Go back" lets saves run again and allows the next close. While it waits the page shows "Finishing saving…" in the place of Save all (#160). File > Quit and `app_quit` start these steps with `closing::requested` (#58); the update install must do the same when the updater exists (#62). Step 3 writes the bounds the window last had (`src-tauri/src/window.rs`).

If step 4 takes longer than 5 seconds the process exits anyway and the log says so. Every typed change was already committed when its command returned, so nothing typed is lost.

## Menus

The menu bar has three menus. `&` marks the mnemonic.

| Menu | Item | Accelerator | Action |
|---|---|---|---|
| File | `&Open folder…` | `Ctrl+O` | A native folder dialog titled "Open a folder of photos". A chosen folder is handed to the page as `open_folder`. |
| File | `&Close folder` | `Ctrl+W` | Closes the album and shows the start screen (see [Browsing](04-browsing.md)). Disabled when no album is open. |
| File | `&Rescan` | none | Rescans the open album (see [Media and scanning](03-media-and-scanning.md)). Disabled when no album is open. |
| File | separator | | |
| File | `Open the &data folder` | none | Opens the app data folder in the file manager. |
| File | separator | | |
| File | `&Quit` | `Ctrl+Q` | Quits, through the steps in [Closing and quitting](#closing-and-quitting). |
| View | `Toggle &full screen` | `F11` | Makes the window full screen, or returns it. This is the window, not the full-screen viewer. |
| View | `&Reload` (development builds only) | `Ctrl+R` | Reloads the page. |
| View | `Toggle &developer tools` (development builds only) | `Ctrl+Shift+I` | |
| Help | `&Keyboard shortcuts` | shown as `?` | Opens the shortcut overlay. |
| Help | separator | | |
| Help | The update item (see [Updates](#updates)) | none | Depends on the update state. |
| Help | `Check &automatically` (checkbox) | none | Toggles `updates.auto`. |
| Help | `What's &new` | none | Shows the notes for this version. |
| Help | separator | | |
| Help | `&Give feedback on GitHub…` | `F2` | Opens the feedback address in the user's browser. |
| Help | separator | | |
| Help | `&About Owlbum` | `F1` | Opens the About box. |

Rules:

- **Every accelerator is set explicitly.** None is left to a platform default. Each comes from its `menu` row in the [key table](#the-key-table); Reload and Toggle developer tools have rows marked `dev`.
- **`?` is not a native accelerator.** It must not fire while the user is typing, so the page handles it. The menu only shows the key beside the item.
- **F1, F2, F11, `Ctrl+O`, `Ctrl+W` and `Ctrl+Q` are native accelerators.** They work whenever the window has focus, including while typing and while a dialog is open. The page must not bind them.
- **Items that act in the page** (Close folder, Rescan, Keyboard shortcuts, What's new, About, and the update item) emit `app_menu` with `{ item }`, the item's id. The page does the rest.
- **The menu is rebuilt** when the update state changes, when `updates.auto` changes, and when an album opens or closes.
- **Menu text** lives in the shell's text module, the Rust counterpart of `ui/src/text/`.

As built (#58, `src-tauri/src/menu.rs`): the items are one table, `menu::ENTRIES`; a shell test fails if a `menu` row of the key table has no item or an item's accelerator has no row. Instead of rebuilding the menu, the shell enables Close folder and Rescan when an album opens or closes and ticks Check automatically when `updates.auto` changes (from the menu or from Settings). The update item came with the updater (#62); its text and whether it can be chosen follow the update state, and it acts in Rust. About and Give feedback (F1, F2) came with #61: About sends `app_menu about`; Give feedback is done in Rust, which opens the feedback address itself. What's new sends `app_menu`; the page shows the running version's notes (#63). The `?` beside Keyboard shortcuts is drawn as the item's key column on Windows (text after a tab); Linux shows the label alone. Open folder runs the native dialog on a thread of its own and hands a chosen folder over as `open_folder`. The single-instance plugin (`tauri-plugin-single-instance`) is registered first, and only when `OWLBUM_DATA_DIR` is not set.

## Settings dialog

The gear button in the top bar opens Settings. It is a modal dialog. Esc, the close button or a click on the backdrop closes it. Every change is saved at once through `app_settings_set`; there is no Save button.

```text
+----------------------------------------------------------------+
| Settings                                                   [×] |
|----------------------------------------------------------------|
| General                                                        |
| [x] Reopen the last album at start                             |
|                                                                |
| Thumbnail cache                                                |
| Uses 12.4 GB of [ 20 ] GB                       [Clear cache]  |
| Thumbnails are made again when needed. Clearing loses nothing. |
|                                                                |
| When I edit a photo                                            |
| (o) Keep the edit in the album                                 |
| ( ) Save edits into the photo file                             |
|----------------------------------------------------------------|
| Maps                                                           |
| [ ] Load maps from the internet                                |
| Map pictures come from OpenStreetMap. Your photos and their    |
| locations are never sent.                                      |
|----------------------------------------------------------------|
| Photo tagging                                                  |
| (see Tagging)                                                  |
|----------------------------------------------------------------|
| Updates                                                        |
| [x] Check for a new version once a day                         |
| Owlbum 0.1.0 · last checked today at 09:14    [Check now]      |
|                                                        [Close] |
+----------------------------------------------------------------+
```

| Section | Control | Key | Rules |
|---|---|---|---|
| General | Reopen the last album at start (checkbox) | `reopen_last` | |
| General | Cache limit (whole number of GB) | `cache.limit_gb` | 1 to 1000. Saved when the field loses focus or Enter is pressed. A value outside the range is put back and the field says why. Lowering the limit prunes at once. |
| General | Cache use (text) | none | From `app_cache_usage`: the bytes held by `thumbs/` and `full/`. Read when the dialog opens and after a clear. |
| General | Clear cache (button) | none | Opens a confirm strip: "Clear 12.4 GB of thumbnails? They are made again as you browse." with **Cancel** and **Clear**. Clear calls `app_cache_clear`, which deletes `thumbs/`, `full/` and their `cache_entries` rows. Models, settings and albums are not touched. |
| General | When I edit a photo (two radio buttons) | `edit_target` | `album` ("Keep the edit in the album": Done is the save and the file is never touched) or `file` ("Save edits into the photo file": an edit is "Edited · not saved" until Save writes it into the file). The option text and what each does are owned by [Editing and file actions](06-editing-and-file-actions.md#what-save-means). |
| Maps | Load maps from the internet (checkbox) | `maps_online` | Ticked is true, cleared is false. Unset shows as cleared. This is the same answer the Map tab's question records (see [Browsing](04-browsing.md)). |
| Photo tagging | The model choices, downloads and removal | `tagging.describe`, `tagging.keywords` | Owned entirely by [Tagging](07-tagging.md). This part only gives it a place in the dialog. |
| Updates | Check for a new version once a day (checkbox) | `updates.auto` | The same setting as Help > Check automatically. Changing either updates the other. |
| Updates | Version and last check (text) | `updates.last_check` | "never" when null. |
| Updates | Check now (button) | none | The manual check, the same as the Help item. |

In a view-only album every control here still works, because none of them writes to the album.

As built (#59, `ui/src/features/app/SettingsDialog.tsx`): Check now is left out until the updater exists (#62). The Photo tagging section is a slot, `ui/src/features/app/TaggingSettings.tsx`, which #57 fills. A refused change shows "Couldn't keep the setting: <reason>" as a toast. Changing `edit_target` reads `edit_status` again, because what waits to be saved depends on it. The cache limit's message reads "Use a whole number from 1 to 1000. The limit was put back."

## Keyboard shortcuts

### The key table

There is one table of keys, `ui/src/keys/keymap.json`. Three things read it and nothing else defines a key:

- the page's key handlers, which look up the pressed key in the table and run the action with that id;
- the shortcut overlay, which lists the table;
- the shell, which compiles the file in and takes its menu accelerators from the rows marked `menu`.

A key that changes in the table changes in all three. A page test fails if a handler exists for an id that is not in the table, or a row has no handler. A shell test fails if a `menu` row has no menu item, or a menu accelerator has no row.

Each row has these fields:

| Field | Meaning |
|---|---|
| `id` | A stable name such as `browse.star`. The label shown in the overlay is the text-module entry with the same id. |
| `place` | `everywhere`, `browse`, `viewer` or `edit`. |
| `keys` | One or more keys. Each is a `KeyboardEvent.key` value, with `Ctrl+`, `Shift+` and `Alt+` in front when needed, in that order. Letters are written in upper case and match in either case. The space bar is written `Space` (its `key` is a single space). The first key is the one hints show, such as the photo menu's "F" for Star. |
| `hold` | True when the action lasts only while the key is down. |
| `repeat` | True when holding the key repeats the action. |
| `menu` | True when the key is a native menu accelerator. A `menu` row has exactly one key. |
| `dev` | True for a menu accelerator that exists only in development builds. The overlay leaves these rows out. |
| `typing` | True when the row also acts while typing in a text field (never in a dialog). |

A flag that is false is left out of the file. The page checks the file when it loads (`ui/src/keys/table.ts`) and matches keys with `ui/src/keys/match.ts`; the shell reads it through `src-tauri/src/keymap.rs`.

Matching rules, the same for every row:

| Rule | Detail |
|---|---|
| Menu rows | The page never binds a `menu` row; the native accelerator does the work. |
| Typing | A row that is not `menu` is ignored while focus is in a text field, a select, an editable element, a dialog or the Filters panel, unless it is marked `typing` (F6 and Shift+F6, so focus can leave the search box). Rows marked `typing` are still ignored in a dialog and the Filters panel. Edit mode's sliders do not count as typing. The Filters panel is marked `data-keys="dialog"`; a full-page layer that has its own rows, such as the viewer, is marked `data-keys="surface"` so it is not taken for a dialog. While a dialog is open the page ignores the rows wherever focus is. |
| Controls | While a button, link, checkbox, radio button, switch or tab has focus, Enter and Space are left to it. Other rows still act. A grid tile is not such a control. |
| Menus | While focus is in a menu, its own keys (arrows, Home, End, Enter, Space, Esc, Tab) are left to it. Any other row closes the menu and then acts. |
| Handled keys | A key that a control already handled (a slider's or a splitter's arrows) or that is part of composing text is ignored. |
| Modifiers | A key pressed with a modifier the row does not name is ignored. Meta (the Windows or Super key) never matches. `Ctrl+` is the Ctrl key: version 1 runs on Windows and Linux only. |
| Symbols | Every key that is a single character other than a letter (`?`, `[`, `]`, `\`, `+`, `=`, `-`, `0`) matches on the character produced, so it works on keyboards where it needs Shift or AltGr. Windows reports AltGr as Ctrl+Alt. |
| Letters | On a layout of another alphabet, where a key gives a letter that is not Latin (Cyrillic, Greek), a letter row matches the physical key (`KeyboardEvent.code`). Punctuation and symbols never do, so Dvorak's `.` is not E and AltGr+E's `€` is not E. |
| Repeat | An auto-repeated press acts only on a `repeat` row. A `hold` row starts on the first press and ends when its key is released, whatever the modifiers or focus are by then. |
| Place | `browse` rows apply in the grid, the timeline, the map and the preview. While the viewer or edit mode is open, only its own rows and the `everywhere` rows apply. |
| Dialogs and menus | Esc closes the topmost dialog, menu or overlay first. Nothing else reaches the rows beneath. |

### Every key

What each action does is owned by the part named in the last column. This table owns only which key reaches it.

| Id | Place | Keys | Action | Flags | Owner |
|---|---|---|---|---|---|
| `app.shortcuts` | everywhere | `?` | Open or close the shortcut overlay | | this part |
| `app.about` | everywhere | `F1` | About Owlbum | menu | this part |
| `app.feedback` | everywhere | `F2` | Give feedback on GitHub | menu | this part |
| `app.open_folder` | everywhere | `Ctrl+O` | Open folder | menu | this part |
| `app.full_screen` | everywhere | `F11` | Toggle full screen | menu | this part |
| `app.quit` | everywhere | `Ctrl+Q` | Quit | menu | this part |
| `app.close_folder` | everywhere | `Ctrl+W` | Close folder | menu | 04 |
| `app.reload` | everywhere | `Ctrl+R` | Reload the page (development builds only) | menu, dev | this part |
| `app.developer_tools` | everywhere | `Ctrl+Shift+I` | Toggle developer tools (development builds only) | menu, dev | this part |
| `app.next_region`, `app.previous_region` | everywhere | `F6`, `Shift+F6` | Move focus to the next or previous region: top bar, filter bar, grid, preview, details panel | typing | [09](09-visual-design.md#accessibility-baseline) |
| `browse.previous`, `browse.next` | browse | `ArrowLeft`, `ArrowRight` | Previous or next photo | repeat | [04](04-browsing.md) |
| `browse.extend_previous`, `browse.extend_next` | browse | `Shift+ArrowLeft`, `Shift+ArrowRight` | Extend the selection one photo | repeat | 04 |
| `browse.first`, `browse.last` | browse | `Home`, `End` | First or last photo | | 04 |
| `browse.open_viewer` | browse | `Enter`, `Space` | Open the full-screen viewer | | 04 |
| `browse.escape` | browse | `Escape` | Close the photo menu, else reduce the selection to the current photo | | 04 |
| `browse.select_all` | browse | `Ctrl+A` | Select every photo shown in the result | | 04 |
| `browse.select_none` | browse | `Ctrl+D` | Select nothing | | 04 |
| `browse.star` | browse | `F`, `S` | Star or unstar | | [05](05-organising.md) |
| `browse.hide` | browse | `H` | Hide or unhide | | 05 |
| `browse.score_up`, `browse.score_down` | browse | `ArrowUp`, `ArrowDown` | Photo score up or down one | | 05 |
| `browse.undo` | browse | `Ctrl+Z` | Undo | | 05 |
| `browse.edit` | browse | `E` | Edit the current photo | | [06](06-editing-and-file-actions.md) |
| `browse.rotate_left`, `browse.rotate_right` | browse | `[`, `]` | Rotate anticlockwise or clockwise | | 06 |
| `browse.before` | browse | `\` | Show the original | hold | 06 |
| `browse.copy` | browse | `Ctrl+C` | Copy the photo, or the files when several are selected | | 06 |
| `browse.trash` | browse | `Delete` | Move to the Recycle Bin or Trash | | 06 |
| `viewer.previous`, `viewer.next` | viewer | `ArrowLeft`, `ArrowRight` | Previous or next photo | repeat | 04 |
| `viewer.first`, `viewer.last` | viewer | `Home`, `End` | First or last photo | | 04 |
| `viewer.close` | viewer | `Enter` | Close the viewer | | 04 |
| `viewer.escape` | viewer | `Escape` | Back to fit if zoomed in, else close the viewer | | 04 |
| `viewer.play` | viewer | `Space` | Play or pause a video | | 04 |
| `viewer.details` | viewer | `I` | Show or hide the details panel | | 04 |
| `viewer.zoom_in`, `viewer.zoom_out` | viewer | `+` or `=`, `-` | Zoom in or out | repeat | 04 |
| `viewer.fit` | viewer | `0` | Back to fit | | 04 |
| `viewer.star` | viewer | `F`, `S` | Star or unstar the shown photo | | 05 |
| `viewer.hide` | viewer | `H` | Hide or unhide the shown photo | | 05 |
| `viewer.score_up`, `viewer.score_down` | viewer | `ArrowUp`, `ArrowDown` | Photo score up or down one | | 05 |
| `viewer.undo` | viewer | `Ctrl+Z` | Undo | | 05 |
| `viewer.rotate_left`, `viewer.rotate_right` | viewer | `[`, `]` | Rotate anticlockwise or clockwise | | 06 |
| `viewer.before` | viewer | `\` | Show the original | hold | 06 |
| `viewer.copy` | viewer | `Ctrl+C` | Copy the shown photo | | 06 |
| `edit.done` | edit | `Enter` | Done | | 06 |
| `edit.cancel` | edit | `Escape` | Cancel | | 06 |
| `edit.rotate_left`, `edit.rotate_right` | edit | `[`, `]` | Rotate anticlockwise or clockwise | | 06 |
| `edit.before` | edit | `\` | Show the original | hold | 06 |

Delete has no `viewer` row on purpose: [Editing and file actions](06-editing-and-file-actions.md#move-to-the-recycle-bin-or-trash) keeps the key out of the viewer so a stray press removes nothing; the photo menu's item is still offered there. `E`, `Ctrl+A` and `Ctrl+D` have no `viewer` rows either. `?` opens the overlay from every place, edit mode included.

The timeline has no keys of its own. It uses the `browse` rows; Left and Right step in date order, and Home and End go to the first and last photo (see [Browsing](04-browsing.md)).

A cell that names two ids stands for two rows of the file, one per key. If this table and the key tables in parts 4 to 6 ever disagree, the owning part is right and this table and `keymap.json` are corrected.

### The overlay

`?` or Help > Keyboard shortcuts opens the overlay. `?` again, Esc or a click outside closes it. It opens over the grid, the viewer and edit mode. It does not open while typing.

```text
+------------------------------------------------------------------+
| Keyboard shortcuts                                           [×] |
|------------------------------------------------------------------|
| Grid and preview  (where you are)   | Everywhere                 |
|  ←  →          Previous, next photo |  ?        Keyboard shortcuts|
|  Home  End     First, last photo    |  F1       About Owlbum      |
|  Enter  Space  Open the viewer      |  F2       Give feedback     |
|  S  F          Star or unstar       |  Ctrl+O   Open folder       |
|  H             Hide or unhide       |  F11      Full screen       |
|  ↑  ↓          Photo score up, down |  Ctrl+Q   Quit              |
|  …                                  |                             |
|------------------------------------------------------------------|
| Full-screen viewer                  | Edit mode                   |
|  ←  →          Previous, next photo |  Enter    Done              |
|  S  F          Star or unstar       |  Esc      Cancel            |
|  …                                  |  …                          |
+------------------------------------------------------------------+
```

- The list for the place the user is in comes first and is marked "(where you are)". "Everywhere" sits beside it. The other places follow.
- Place headings are "Grid and preview", "Full-screen viewer", "Edit mode" and "Everywhere". In the timeline or map the first heading reads "Timeline" or "Map".
- Keys are drawn as key caps. Arrow keys are drawn as arrows. Two keys for one action sit side by side. A hold key reads "hold \".
- Rows whose action is unavailable on this platform (a Linux gap, see the README) are left out.
- The overlay is read-only. Keys cannot be changed in version 1.

As built (#60, `ui/src/features/app/ShortcutOverlay.tsx`): each row of the table is one line, its keys side by side as caps and its label from `ui/src/text/keys.ts`; rows are not merged into pairs such as "Previous, next photo". Focus starts on the list rather than the close button, so the first Esc closes the overlay. At the owner's request the overlay has a **Settings…** button, which closes it and opens Settings. About and Give feedback are listed since #61. No row is left out for a platform yet, because no row is known to be missing on Windows or Linux.

## Updates

Owlbum uses `tauri-plugin-updater`. The feed is a static file on GitHub Releases:

```text
https://github.com/funvill/Owlbum/releases/latest/download/latest.json
```

`latest.json` is written by the release workflow. It names the newest version and, for each installer, its address and signature. A draft release is invisible at that address, so nobody is offered a version until a person publishes it.

The plugin never checks by itself. Owlbum calls it at the moments below and nowhere else.

**What a check sends.** One HTTPS request for `latest.json`, and after the user agrees, one for the installer. No identifier, no album information and nothing about the computer is added. GitHub sees the address the request came from, as any web server does.

### When it checks

| Check | When | Shows a result |
|---|---|---|
| Quiet | 10 seconds after `app_startup`, then every 6 hours while Owlbum stays open, the rule below is tested. | Only when a newer version exists. A failure is logged and never shown. |
| Manual | Help > Check for updates…, the About box, or Settings > Check now. | Always. |

The quiet check runs only when all of these hold:

- the build is installed (not a development build) and the updater is set up (see [The signing key](#the-signing-key));
- `OWLBUM_NO_UPDATE_CHECK` is unset or empty;
- `updates.auto` is true;
- the first-start notice has been answered: `updates.notice_seen` is true (see below);
- `updates.last_check` is null, or is later than now (the clock was turned back), or is at least 24 hours old.

`updates.last_check` is a number, milliseconds since the epoch. It is written after any check that got an answer, including "no release has been published yet". It is not written after a failure, so a check that failed offline is tried again at the next start.

The manual check ignores `updates.auto`, the environment variable and the 24 hours.

One check, one offer, one download and one install question exist at a time. A second request joins the one in progress. A manual check while an update is already downloaded offers the install again.

### What the first start says

On the first start, the page shows a notice on the start screen. No update check runs before it is answered.

> Owlbum checks GitHub for a new version once a day. Nothing about you or your photos is sent. You can turn this off in Help or Settings.
>
> **Turn off** · **OK**

OK leaves `updates.auto` true. Turn off sets it false. Either answer sets the setting `updates.notice_seen` to true (through `app_update_notice_answer`) so the notice is never shown again, and after OK the quiet check may run in the same session. If the user opens an album without answering, the notice stays as a bar above the grid until answered.

### Turning it off

| Way | Effect |
|---|---|
| Help > Check automatically, or Settings > Updates | Sets `updates.auto` to false. Stops the quiet check. |
| `OWLBUM_NO_UPDATE_CHECK` set to any non-empty value | Stops the quiet check for that run. Does not change the setting. |

The manual check always works.

### States

The updater is a small state machine in the shell. Each change is sent to the page as `update_state` with `{ state, version?, percent? }`.

| State | Meaning | Help menu item |
|---|---|---|
| `unavailable` | A development build, or the updater is not set up. | `Check for &updates…` (answers with the message below) |
| `idle` | Nothing in progress. | `Check for &updates…` |
| `checking` | A check is running. | `Checking for updates…` (disabled) |
| `available` | A newer version was found and offered. | `Download &update <version>…` |
| `downloading` | The installer is downloading. | `Downloading update… N %` (disabled) |
| `ready` | The installer is downloaded and verified. | `Install &update <version>…` |
| `installing` | Owlbum is closing to install. | `Installing update…` (disabled) |

A failed check or download returns to `idle`, or to `available` when a version is already known.

### Messages

These are page dialogs titled `Owlbum`. The first button is the default.

| Moment | Message | Buttons |
|---|---|---|
| Update found | `Owlbum <latest> is available (you have <current>).` | `Download`, `Release notes`, `Later` |
| Up to date (manual check) | `You have the newest version of Owlbum (<current>).` | `OK` |
| Check failed (manual check) | `Could not check for updates: <reason>` | `OK` |
| Development build | `Updates only work in the installed Owlbum.` | `OK` |
| Updater not set up | `This copy of Owlbum cannot update itself. Download new versions from the release page.` | `Open the release page`, `Close` |
| Download failed | `The update did not download: <reason>` | `Try again`, `Close` |
| Downloaded | `Owlbum <version> is ready to install. Owlbum will close and start again.` | `Restart now`, `Later` |
| Cannot install | `Owlbum cannot install this update by itself: <problem> Download it from the release page and install it from there.` | `Open the release page`, `Close` |
| Install failed | `The update did not install: <reason>` | `Open the release page`, `OK` |

- `<reason>` is the first non-empty line of the error, trimmed, at most 300 characters.
- **Release notes** opens `https://github.com/funvill/Owlbum/releases/tag/v<version>` in the user's browser, and the offer stays open.
- **Open the release page** opens `https://github.com/funvill/Owlbum/releases/latest`.
- **Later** keeps the state. The Help item offers the download or the install again.
- A feed that answers 404 means no release has been published yet. That is "up to date", not a failure.
- An older version is never offered.

### Download

Download starts after the user chooses it and never before. While it runs:

- the Help item reads `Downloading update… N %`;
- the taskbar or dock entry shows a progress bar where the platform has one;
- `update_state` is sent each time the rounded percentage changes.

The plugin checks the installer's signature against the public key compiled into Owlbum. A file that fails the check is deleted and reported as a failed download.

### Install

Choosing **Restart now**:

1. Check that the update can be installed (table below). A problem shows "Cannot install" and nothing is closed.
2. Run the closing steps with reason `update`. If the user chooses "Go back", the install is called off and the state stays `ready`.
3. Install, then start the new version.

| Package | How it installs | What can stop it |
|---|---|---|
| Windows (NSIS) | The installer runs quietly as the current user. No administrator prompt. It starts Owlbum again when done. | The downloaded file is gone: `the downloaded update is gone.` |
| Linux AppImage | The running AppImage file is replaced by the new one, and Owlbum starts again from it. | The folder holding the AppImage cannot be written: `it cannot write to <folder>.` |
| Linux `.deb` | The package is installed through `pkexec`, so **the system asks for an administrator password**. Owlbum says so before it closes: "Your system will ask for an administrator password to install the update." with `Continue` and `Cancel`. | `pkexec` is missing or refuses: `there is no way to ask for the administrator password (pkexec).` Dismissing the password prompt calls the install off without a message. |

If the install fails after Owlbum has closed its album, Owlbum shows "Install failed" and starts the current version again.

### The signing key

The updater plugin cannot work without a signing key pair. It verifies every download with it, and this cannot be switched off.

| Item | Where it lives |
|---|---|
| Private key and its password | GitHub Actions secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Never in the repository. A person keeps a copy somewhere safe; if it is lost, installed copies can never be updated again. |
| Public key | `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. |

**The key is created by a person at final review**, with `npm run tauri signer generate`. Until then:

- the public key in the configuration is an empty string;
- Owlbum treats an empty public key as "the updater is not set up": the state is `unavailable`, no check is ever made, and the manual check shows the "cannot update itself" message;
- the update rules, the state machine, the menu text and the dialogs are all built and tested against a fake updater;
- the real path (download from GitHub, verify, install, restart) **cannot be exercised end to end**. It is tested by a person once the key exists, using the test feed below, before the first release is published.

Updater artefacts are only built by the release workflow. `bundle.createUpdaterArtifacts` is false in `tauri.conf.json` and true in `src-tauri/tauri.release.conf.json`, which the workflow merges. A local `tauri build` therefore needs no key.

### Test feed

`OWLBUM_UPDATE_FEED=<url>` replaces the feed address for that run. It is logged. The files it serves must be signed with the same key, because verification still applies. A release build accepts only `https://` addresses; a development build also accepts `http://127.0.0.1`.

### As built (#62)

- **Where it lives.** The rules (when the quiet check runs, the feed address, the menu text, `<reason>`) are plain functions in `owlbum_core::app::update`. The state machine is `src-tauri/src/update.rs`; the network work is behind its `UpdateSource` trait. `run` installs `PluginSource`, which drives `tauri-plugin-updater`; shell tests install a fake, and one test runs the real plugin against a feed served on `127.0.0.1` (`src-tauri/tests/lifecycle.rs`). No test reaches the network.
- **The key.** `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` is `""`, with the GitHub feed as its endpoint and `windows.installMode` `quiet`. While it is empty the state is `unavailable` and the manual check answers "cannot update itself". Putting the public key there is the only change needed. The plugin is registered in every build, but its commands are not granted to the page; the page calls only Owlbum's own `app_` commands.
- **Development builds** never check, unless `OWLBUM_UPDATE_FEED` is set, so the test feed can be tried from a development build.
- **Outcomes are events.** Each change is `update_state`; each message of the table above is an `update_message` event `{ kind, … }` (`found`, `up_to_date`, `check_failed`, `dev_build`, `not_set_up`, `download_failed`, `downloaded`, `cannot_install`, `install_failed`), and the page draws it. `app_update_state` answers the state for the page's first look.
- **404 and other answers that are not a success.** The plugin reports them all the same way, so a 404 and a server error both read as "no release has been published yet" (up to date). A network failure is a failed check.
- **Help's update item acts in Rust**: it checks, starts the download, or offers the install again, by state. It does not send `app_menu update`.
- **The first-start notice** is a banner under the top bar, on the start screen and above an open album alike, until answered (`app_update_notice_answer`). After OK the quiet check's clock starts in the same session.
- **The quiet clock** starts at `app_startup` (or at the notice's answer) only when the updater is set up: 10 seconds, then every 6 hours, testing the rule each time.
- **Install.** Restart now runs the closing steps with reason `update` (`closing::requested_for`); "Go back" puts the state back to `ready`. After step 4 the plugin installs; on Windows the NSIS installer takes over and the process ends. If the install fails, the log says why and the current version starts again; the "Install failed" message is not shown after that restart.
- **Not built yet:** the taskbar progress bar; the Linux install checks (AppImage folder writable, `.deb` through `pkexec` with its question). Linux was not run.

## About box

F1 or Help > About Owlbum opens a page dialog titled `About Owlbum`. Its content comes from `app_about`. The owner's rule (#61): it shows the version and a link to the GitHub repository.

```text
+--------------------------------------------------------------+
| About Owlbum                                             [×] |
|--------------------------------------------------------------|
| [owl]   Owlbum 0.1.0                                         |
|         Version 0.1.0 (windows x86_64)                       |
|                                                              |
| Look through folders of photos, star the best ones, find     |
| things by what is in them, and drag photos into other        |
| programs.                                                    |
|                                                              |
| Everything stays on this computer. …                         |
|                                                              |
| Its thumbnails, settings and models are kept in:             |
| C:\Users\sam\AppData\Local\OwlbumData                        |
|                                                              |
| Built from the open-source project funvill/Owlbum on GitHub, |
| under the MIT License. Copyright (c) 2026 …                  |
| It includes software from other projects, each under its own |
| licence: see "Third-party notices".                          |
|                                                              |
| [Check for updates] [Licence] [Third-party notices]     [OK] |
+--------------------------------------------------------------+
```

The paragraphs, in order:

1. `Owlbum <version>` and `Version <version> (<platform> <architecture>)`. On Linux the package kind follows: `deb` or `AppImage`.
2. The product description.
3. The network statement, word for word: "Everything stays on this computer. Owlbum uses the network only for OpenStreetMap map tiles, and only after you choose Load maps on the Map tab; to download photo-tagging models from Hugging Face when you ask for one in Settings; and to check GitHub for a new version once a day, unless you turn that off in Help."
4. "Its thumbnails, settings and models are kept in:" and the app data folder.
5. The credits: "Built from the open-source project funvill/Owlbum on GitHub, under the MIT License." then the copyright line from `LICENSE`, then "It includes software from other projects, each under its own licence: see "Third-party notices"."

As built (#61, `ui/src/features/app/AboutDialog.tsx`): the logo is the owner's gold owl (see [Visual design](09-visual-design.md)), a 128 px copy at `ui/src/assets/owlbum-icon.png` drawn at 64 px and bundled with the page. Under the credits is the repository's address, `https://github.com/funvill/Owlbum`, drawn as a link; choosing it calls `app_open_repository`, and Rust opens the address in the user's browser. The buttons are **Give feedback** (as F2) and **OK** (focused; Esc also closes). The other buttons came with #62 and the rest of #61: **Check for updates** shows while the updater's state is not `unavailable`; **Licence** opens `LICENSE.txt` from the resources (`bundle.resources` copies the repository's `LICENSE` there) or, when a development build has none, a copy of the licence compiled into the program, written to the app data folder; **Third-party notices** shows only when `THIRD-PARTY-NOTICES.txt` is among the resources (`app_about` answers `has_notices`). Generating that file (`scripts/notices.mjs`) is not built yet, so the button is hidden for now. The package kind is not shown on Linux yet.

Buttons:

| Button | Action |
|---|---|
| `OK` (default; Esc) | Closes the box. |
| `Check for updates` | Closes the box and runs the manual check. Hidden when the state is `unavailable`. |
| `Licence` | Opens `LICENSE.txt` from the program's resources. |
| `Third-party notices` | Opens `THIRD-PARTY-NOTICES.txt` from the program's resources. |

A file that cannot be opened is shown in its folder instead, and the failure is logged.

## Feedback

F2, Help > Give feedback on GitHub… or the About box's **Give feedback** opens the project's issues page in the user's own browser. This is the owner's decision on #61; it replaces a prefilled new-issue address.

```text
https://github.com/funvill/Owlbum/issues
```

Nothing about the computer is put in the address. No folder, file name, user name or log line is ever sent. The user finds or opens an issue there and decides what to write.

## What's new

After an update, Owlbum says what changed.

**The notes** are the released sections of the repository's `CHANGELOG.md` (Keep a Changelog), compiled into the program. Nothing is fetched. Each `## [x.y.z]` section is one entry, with its `### Added`, `### Changed`, `### Fixed` and `### Removed` groups and one item per bullet (wrapped lines joined). `Unreleased` is never shown, and a version with no items has no entry. A test checks that the file parses, is newest first, and has no version above the workspace version.

As built (#63, `owlbum_core::app::whats_new`): this replaces the planned `src-tauri/whats-new.json`, so the notes are written once, in the changelog. `app_startup` carries `whats_new`, the entries to show; empty shows nothing.

**When it shows.** `app_startup` compares the running version with `whats_new.last_seen`, part by part as numbers (so `0.1.10` is newer than `0.1.9`).

| Situation | Shown | `whats_new.last_seen` afterwards |
|---|---|---|
| `last_seen` is null (a first install) | Nothing | The running version, recorded at once |
| Running version is newer, and entries exist above `last_seen` up to the running version | Those entries, newest first | The running version, recorded when the dialog is closed |
| Running version is newer, and no such entries exist | Nothing | The running version |
| Running version equals `last_seen` | Nothing | Unchanged |
| Running version is older (a downgrade) | Nothing | The running version |
| A development build, or `OWLBUM_NO_WHATS_NEW` is set and not empty | Nothing | Unchanged |

Recording on close means a crash during start shows the notes again next time.

**How it looks.** A page dialog titled `What's new in Owlbum <version>`, with one bullet per item under its group's heading, and **OK**. When more than one entry is shown, each sits under its version number.

**Help > What's new** shows the running version's entry at any time, or "Nothing new to show for this version." It never changes `whats_new.last_seen`.

## Logs

| Topic | Rule |
|---|---|
| File | `<app data>/logs/owlbum.log`. |
| Fallback | If that folder cannot be written: `<system temp>/owlbum-logs/owlbum.log`. If neither works, logging is off and nothing else is affected. |
| Rotation | When the log is first opened in a run and `owlbum.log` is over 2,000,000 bytes, it is renamed `owlbum.previous.log`, replacing the older one. It is not rotated again during that run. At most two files exist. |
| Line format | `[<UTC time, ISO 8601 with milliseconds>] <LEVEL> <area> <message>` |
| Levels | `ERROR`, `WARN`, `INFO`. `DEBUG` lines are written only when `OWLBUM_LOG=debug`. |
| Areas | `app`, `album`, `scan`, `thumbs`, `tagging`, `edit`, `file`, `update`, `page`. |
| Write failures | Ignored. A full disk never stops the program because of the log. |
| Development builds | Lines also go to the console. |

**What is logged:** the start line; an album opening and closing, with its folder and whether it is view-only; scan totals; each file that could not be read, with its path and the reason; trash, copy, reveal, drag and save-edit results as counts; model downloads starting, finishing and failing; update lines (the feed when overridden, each check and its outcome, download, install, call-off); refused navigations and new windows, with their addresses; errors and warnings the page reports through `app_log`; panics with their backtrace; fatal errors with their technical block.

As built: the log file, its fallback and its rotation are `owlbum_core::appdata::log`. `app_log` (#61) writes the page's lines under area `page`; the page sends every uncaught error and unhandled promise rejection (`ui/src/features/app/pageLog.ts`). Lines over 2,000 characters are cut, and lines past 100 in a minute are dropped.

**What is never logged:** pixels or thumbnails; descriptions, notes, people's names, keywords or tags; search text; GPS positions; anything read from a file other than its path and format.

The log does hold folder and file paths. It never leaves the computer unless the user sends it. Nothing reads it but the user.

## Failure handling

There is no crash reporter and nothing is uploaded. A failure is always a message, never a blank window.

### What counts as fatal

| Failure | Result |
|---|---|
| A command fails | Not fatal. The command returns an `AppError` and the page shows its message. |
| The album database cannot be written | Not fatal. The album becomes view-only (see [Data](02-data.md)). |
| `app.sqlite` is damaged | Not fatal. It is set aside and remade (see [Startup sequence](#startup-sequence)). |
| A panic in a scan, thumbnail, tagging or render job | Logged with its backtrace. The job reports failure for that photo; the program carries on. |
| A panic in an album's writer thread, or on the main thread | Fatal, phase `run`. |
| The app data folder cannot be created or written | Fatal, phase `start`. |
| The webview cannot be created, or the program's own files are missing | Fatal, phase `start`. |
| The page does not call `app_startup` within 30 seconds, or the webview process ends | Fatal, phase `page`. |

The release profile keeps `panic = "unwind"`, so a panic can be caught at a thread's edge and logged.

### The fatal error dialog

It is shown once. Nothing fatal is reported while Owlbum is already quitting.

| Phase | Title | Message | Advice |
|---|---|---|---|
| `run` | `Owlbum stopped working` | `The part of Owlbum that reads your photos stopped unexpectedly. Stars and notes up to your last change are saved.` | `Restart Owlbum. If this keeps happening, send the details below as feedback on GitHub.` |
| `page` | `Owlbum could not show its window` | `Owlbum started, but its window could not load.` | `Restart Owlbum. If this keeps happening, reinstall it.` |
| `start`, data folder cannot be written | `Owlbum could not start` | `Owlbum could not save to its data folder:\n<app data>` | `Check that the disk is not full and that you are allowed to save files in that folder, then start Owlbum again.` |
| `start`, files missing | `Owlbum could not start` | `Some of Owlbum's own files are missing or damaged.` | `Reinstall Owlbum.` |
| `start`, no webview | `Owlbum could not start` | `Owlbum could not open its window. A part of the system that Owlbum needs to draw its window is missing.` | Windows: `Reinstall Owlbum; the installer adds what is missing.` Linux: `Install your system's WebKitGTK 4.1 package, then start Owlbum again.` |
| `start`, anything else | `Owlbum could not start` | `Owlbum ran into a problem while starting.` | `Start it again. If this keeps happening, reinstall Owlbum or send the details below as feedback on GitHub.` |

Below the message and advice, the native message box shows the **technical block**, at most 2,500 characters. The page-drawn dialog offers it through `Copy details` only:

```text
Owlbum <version> (<platform> <architecture>, <package kind>)
Data folder: <app data>
Log file: <log file>

<the error and its backtrace>
```

| Button | Action |
|---|---|
| `Copy details` | Puts the technical block on the clipboard. The dialog stays. |
| `Open the log folder` | Opens the log's folder in the file manager. The dialog stays. |
| `Restart` | Starts Owlbum again and ends this copy with exit code 1. In phase `start` this button reads `Close` and only ends the program. |

The dialog is drawn in one of two ways:

- **By the page**, when the page is alive (phase `run`). Rust emits `app_fatal` with `{ title, message }`, where `message` holds the message and, after a blank line, the advice. The page shows the dialog over everything, with a fourth button, `Close`. The page is not sent the technical block: `Copy details` calls `app_copy_details`, and Rust puts the block on the clipboard.
- **By a native message box**, when the page cannot be trusted (phases `start` and `page`). It has the three buttons above. Closing the box by its title bar ends the program.

As built (#61, `src-tauri/src/fatal.rs`, words in `owlbum_core::app::fatal`):

- A panic hook logs every panic with its thread, place and backtrace. A panic on the main thread is shown in the **native** box at once, because it ends the event loop and the page can no longer be reached. A panic on an album's writer thread outside its jobs (a job's own panic is caught and rolled back) is sent to the page as `app_fatal`. Panics on other threads are only logged.
- Phase `page` rests on the 30-second `app_startup` rule alone: whether Tauri reports the webview process ending is not checked (open question 2).
- Phase `start`: a failure to find or open the app data folder, or Tauri failing to start, shows the native box. An error naming the webview reads as "no webview". "Files missing" is not told apart yet; it reads as "anything else".
- The native box is `rfd`'s three-button message box. Copy details and Open the log folder show it again; Restart starts the program's own file again and exits with code 1.
- The page's dialog has Copy details, Open the log folder, Close and Restart (the default). Restart is `app_restart` (Tauri's restart, which skips the closing steps); Close is `app_quit`, through the closing steps.
- Nothing fatal is reported once the closing steps have begun, and only the first fatal error is shown.

## Network policy

The rules are in [Architecture](01-architecture.md). This section gives their exact configuration.

### Content-Security-Policy

`app.security.csp` in `tauri.conf.json`, on one line:

```text
default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' owlbum: http://owlbum.localhost data: blob: https://tile.openstreetmap.org; media-src owlbum: http://owlbum.localhost; font-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; frame-src 'none'; worker-src 'self' blob:; base-uri 'self'; form-action 'none'
```

| Directive | Why |
|---|---|
| `img-src … owlbum: http://owlbum.localhost` | Thumbnails and full-size pictures. A custom scheme is addressed as `owlbum://…` on Linux and as `http://owlbum.localhost/…` on Windows, so both forms are listed. |
| `img-src … https://tile.openstreetmap.org` | Map tiles. The only remote host in the policy. |
| `media-src` | Video through `owlbum://video/…`, in both forms of the scheme, and nothing else. `blob:` would be added here only if the Linux playback fallback that builds a `Blob` is adopted (see [Media and scanning](03-media-and-scanning.md#video-playback)). |
| `connect-src … ipc: http://ipc.localhost` | Tauri's command channel, in its Linux and Windows forms. Nothing else can be fetched. |
| `style-src … 'unsafe-inline'` | Leaflet and the grid set styles on elements. Scripts get no such exception. |
| `object-src`, `frame-src`, `form-action` | Nothing embeds, frames or posts. |

`app.security.devCsp` is a second policy used only by `tauri dev`. It adds the Vite server's address and its websocket (`http://localhost:5173`, `ws://localhost:5173`) to `connect-src`, and `'unsafe-inline'` to `script-src` for the small inline script Vite's React plugin puts in the page for hot reloading. It is never in a release build.

`app.security.dangerousDisableAssetCspModification` is `["style-src"]`. Tauri adds nonces and hashes to `script-src` and `style-src` for inline content it finds in the page; a nonce in `style-src` would make browsers ignore `'unsafe-inline'` there and break the styles Leaflet and the grid set on elements. `script-src` is still left to Tauri, which hashes nothing because the page has no inline script.

Three tests guard the policy: a shell test (`src-tauri/tests/network.rs`) compares the configured string with the text above and checks that `devCsp` only adds the Vite server; a page test (`ui/src/test/builtPage.test.ts`) builds the page and asserts it contains no remote address other than the tile host; the same test asserts the built page has no inline script. Addresses in the bundle that are never requested are allowed by name in that test: XML namespace names (`http://www.w3.org/…`), React's error-page prefix in its minified errors, and Leaflet's attribution prefix, which the map turns off.

### Navigation and new windows

| Request | Answer |
|---|---|
| Navigation to the app's own page | Allowed. |
| Navigation anywhere else, including by a redirect | Refused and logged as `refused navigation <address>`. |
| A new window or pop-up | Refused and logged as `refused new window <address>`. |
| A link the user should follow (release notes, feedback, the release page) | Never a link in the page. The page calls an `app_open_…` command, and Rust opens one of its own fixed addresses in the user's browser. |

The page cannot pass an address to be opened. Each `app_open_…` command builds its address in Rust.

The rules are the main window's navigation and new-window handlers (`src-tauri/src/navigation.rs`). The app's own page is `tauri://localhost` on Linux and `http://tauri.localhost` on Windows; under `tauri dev` the Vite server's origin (`build.devUrl`) is the app's page too.

### Map-tile consent

A Content-Security-Policy is fixed when the program is built, so the tile host is always in it. Consent is therefore kept by the page:

- The page creates the map's tile layer only when `maps_online` is true. Until then the map shows pins on a plain background and the question from [Browsing](04-browsing.md).
- No tile address appears anywhere in the page except in that one layer's set-up. A page test checks this.
- Turning `maps_online` off removes the tile layer at once.

Tile requests are made by the webview, so they carry what the webview sends. To follow OpenStreetMap's tile rules and to send as little as possible:

- the webview's user agent names Owlbum, so the tile servers can tell which program is asking. OpenStreetMap's tile policy asks for a user agent that names the app and never poses as a browser, and Tauri can only replace a user agent, not add to it, so the shell builds the main window itself (its configuration has `create: false`) with the user agent `Owlbum/<version> (<platform>; +https://github.com/funvill/Owlbum)`, for example `Owlbum/0.1.0 (Windows NT 10.0; Win64; x64; +https://github.com/funvill/Owlbum)`. It keeps the `Windows` token, which the page reads to address its custom scheme;
- tiles are cached by the webview's own HTTP cache, as the tile servers' headers say; the page never prefetches or bulk-loads tiles;
- the page sets `<meta name="referrer" content="no-referrer">`, so no referring address is sent;
- the map shows the "© OpenStreetMap contributors" credit.

### Capabilities

One capability file, `src-tauri/capabilities/main.json`, applies to the window `main` and to local content only. No remote address is given any capability. The file is generated from the command registry by `scripts/bindings.sh` (with the page's bindings), and `src-tauri/build.rs` reads the command names back from it into the app manifest. A command missing from the file therefore cannot be called, and the bindings test fails until the file is rewritten.

| Granted | Why |
|---|---|
| The app's own commands, listed by name | Commands are declared in the app manifest (`tauri_build::AppManifest::commands`), so one that is not granted here cannot be called. |
| `core:event:allow-listen`, `core:event:allow-unlisten` | The page listens for the events in [Architecture](01-architecture.md). |

| Not granted | How the page gets the feature instead |
|---|---|
| Updater, dialog, opener, clipboard, drag | Through `app_` and `file_` commands. The plugins are used from Rust only. |
| File system, shell, HTTP | Not installed at all. |
| `core:window`, `core:webview`, `core:menu` | The shell does all of this itself. |

A shell test reads the capability file and fails if it grants anything outside the first table. Another builds the app with its real permission checks and asserts that a plugin command and any call from a remote page are refused.

## Packaging

### Bundle configuration

| Key in `tauri.conf.json` | Value |
|---|---|
| `productName` | `Owlbum` |
| `identifier` | `com.funvill.owlbum` |
| `mainBinaryName` | `owlbum` |
| `bundle.targets` | `nsis` on Windows; `deb` and `appimage` on Linux |
| `bundle.category` | `Photography` |
| `bundle.shortDescription` | The product description |
| `bundle.licenseFile` | `LICENSE` |
| `bundle.resources` | `LICENSE.txt`, `THIRD-PARTY-NOTICES.txt`, and the ONNX Runtime library if it is not linked in |
| `bundle.icon` | The files in `src-tauri/icons/` |
| `bundle.createUpdaterArtifacts` | `false` (set true by `tauri.release.conf.json`) |
| `plugins.updater.endpoints` | The feed address above |
| `plugins.updater.pubkey` | Empty until final review |

**Windows (NSIS).**

| Setting | Value |
|---|---|
| `bundle.windows.nsis.installMode` | `currentUser`. Per user, no administrator prompt, no elevation. |
| Shortcuts | Start menu and desktop. |
| `bundle.windows.webviewInstallMode` | `downloadBootstrapper`. Windows 11 already has WebView2. On a Windows 10 computer without it, the installer fetches it from Microsoft. The site says so. |
| Languages | English only. |
| Code signing | None in version 1. Windows SmartScreen warns on first run; the site explains how to continue. |
| Uninstall | Removes the program and its shortcuts. It never deletes the app data folder or any `.owlbum` folder. The uninstaller's "delete application data" box, if ticked, removes only the folders Tauri names after the identifier, where Owlbum keeps nothing. |
| Install folder | The per-user default, `%LOCALAPPDATA%\Owlbum`. The app data folder is deliberately a different one, `%LOCALAPPDATA%\OwlbumData` (see [Data](02-data.md#the-app-data-folder)), so an uninstall or reinstall never touches the cache and settings. |

**Linux `.deb`.**

| Setting | Value |
|---|---|
| Package name | `owlbum`. The command `owlbum` is on the path and `owlbum.desktop` is installed. |
| `depends` | `libwebkit2gtk-4.1-0`, `libgtk-3-0` (added by Tauri), and `xdg-utils` (opens folders and addresses). |
| `recommends` | `pkexec` (to install updates), and the codec packages that [Media and scanning](03-media-and-scanning.md) names for video. |
| Floor | Ubuntu 22.04 and Debian 12, the oldest releases with WebKitGTK 4.1. |

**Linux AppImage.**

| Setting | Value |
|---|---|
| Built on | `ubuntu-22.04`, so it runs on that release and newer. |
| `bundle.linux.appimage.bundleMediaFramework` | `true`, so video plays without extra system packages. This needs an Ubuntu build host. |
| Updates | The file replaces itself, so it must sit in a folder the user can write. |

### Icons

The logo is owned by [Visual design](09-visual-design.md). One 1024 px PNG, `src-tauri/icons/source.png`, is the source. `npm run tauri icon src-tauri/icons/source.png` makes the rest (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png`, `icon.ico`), and those files are committed. The window, the installer, the shortcuts and the Linux launcher all use them.

### File names

For version `V`:

| Platform | File |
|---|---|
| Windows | `Owlbum_V_x64-setup.exe` |
| Linux | `Owlbum_V_amd64.deb` |
| Linux | `Owlbum_V_amd64.AppImage` |
| Updater | `latest.json`, and a `.sig` signature file beside each installer |

None has a space in its name.

### Licence and third-party notices

Two text files ship in the program's resources and are opened from the About box.

- `LICENSE.txt` is a copy of the repository's `LICENSE` (MIT).
- `THIRD-PARTY-NOTICES.txt` is generated; it is never edited by hand and is not committed.

`scripts/notices.mjs` builds the notices from four sources:

| Source | How |
|---|---|
| Rust crates | `cargo about generate`, with `about.toml` listing the accepted licences. A crate under a licence not on the list fails the build, so a new licence is always a decision. |
| npm packages | The production dependencies of `ui/package.json` and everything they depend on, read from `ui/node_modules`. Build-only tools are left out. |
| Things that ship but are neither | Hand-written files in `notices/`: ONNX Runtime (MIT) and the libraries inside it, WebView2 and WebKitGTK as system components, and OpenStreetMap's data credit. |
| Models | The tagging models are downloaded on request and are not in the installer. The notices list each model in the catalogue with its licence and source, under the heading "Downloaded only when you ask". [Tagging](07-tagging.md) owns the catalogue. |

A missing licence text fails the release build.

**The LGPL notice.** `rawler`, the camera RAW decoder, is under LGPL-2.1 and is linked into the program. The notices carry this entry, with the exact version taken from `Cargo.lock`:

> Owlbum includes rawler <version>, a camera RAW decoder, under the GNU Lesser General Public License version 2.1. Its source is at https://github.com/dnglab/dnglab. The full text of the licence follows.
>
> You may replace rawler with your own changed version. Owlbum's complete source, with its build instructions, is at https://github.com/funvill/Owlbum under the MIT License: change the `rawler` dependency in `Cargo.toml` to point at your version and build Owlbum again.

### Bundle size

No size is promised yet. The targets are open questions below. The release checklist records each installer's size, so growth is noticed.

## Workflows

There are three workflows. `checks.yml` runs the tests on every push to `main` and every pull request; the other two build and publish, and run no tests (the tests run locally before a tag is pushed, and in `checks.yml`; see [Tests](#tests)).

### `checks.yml`

| Topic | Detail |
|---|---|
| Trigger | A push to `main`, any pull request, and a manual run. |
| Permissions | `contents: read`. |
| Runner | `windows-latest`. A Linux job (`ubuntu-22.04`, with the Tauri build packages) returns when the pause on Linux work ends; the owner paused it on 2026-11-26. |
| Steps | Check out; Node from `ui/.nvmrc`; the Rust toolchain pinned in `rust-toolchain.toml`; a cache of Cargo's downloads and build directory; `./scripts/check.sh` in bash. |
| Time limit | 60 minutes. A newer push to the same branch cancels the older run. |

It was added at the owner's request during the build (2026-11-26); earlier versions of this document said version 1 had no CI test jobs.

### `release.yml`

| Topic | Detail |
|---|---|
| Trigger | A pushed tag matching `v*`. Nothing else. |
| Permissions | `contents: write`. |
| Matrix | `windows-latest` and `ubuntu-22.04`, `fail-fast: false`. |
| Result | A **draft** release named after the tag, holding the three installers, their `.sig` files and `latest.json`. |

Steps on each runner:

1. Check out the tagged commit.
2. **Version check.** Read the workspace version with `cargo metadata`. If `v<version>` is not the tag, fail with: "The tag <tag> does not match the version <version> in Cargo.toml."
3. **Secrets check.** If `TAURI_SIGNING_PRIVATE_KEY` is empty, fail with: "The updater signing key is not set. Create it and add the secrets TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD (see docs/08-desktop-and-release.md, The signing key)." The same check fails if `plugins.updater.pubkey` is empty. This runs before any build, so the failure takes seconds and says what to do.
4. Install Rust (stable) and Node (the version in `ui/.nvmrc`). On Ubuntu, install the Tauri build packages (`libwebkit2gtk-4.1-dev`, `build-essential`, `libssl-dev`, `librsvg2-dev`, `patchelf` and the others Tauri lists).
5. `npm ci` in `ui/`.
6. Build the notices (`node scripts/notices.mjs --strict`).
7. Run `tauri-apps/tauri-action` with `tagName` set to the tag, `releaseDraft: true`, `includeUpdaterJson: true`, and `args: --config src-tauri/tauri.release.conf.json`. It builds the page and the program, signs the updater artefacts, creates the draft release if it does not exist, and uploads the installers and `latest.json`.

A final job, after both runners, lists the draft's files and fails if any of the three installers, a `.sig` file or `latest.json` is missing, or if `latest.json` lacks an entry for Windows or Linux.

The workflow never publishes. If a release for the tag is already published, it fails rather than change files that installed copies may be downloading.

### `pages.yml`

| Topic | Detail |
|---|---|
| Trigger | A push to `main` that changes `site/**`, and a manual run. |
| Permissions | `pages: write`, `id-token: write`. |
| Steps | Check out; `actions/configure-pages`; `actions/upload-pages-artifact` with `path: site`; `actions/deploy-pages`. |
| Build | None. The folder is published as it is. |

It fails until a person switches GitHub Pages on for the repository with "GitHub Actions" as the source. That is done at final review.

## The marketing site

One static page, `site/index.html`, with `site/style.css` and an `site/img/` folder. It is hand-written HTML and CSS. There is no build step, no framework, no script, no web font from another host, no analytics and no cookie. It is published at `https://funvill.github.io/Owlbum/`.

| Section | Content |
|---|---|
| What it is | The name, the logo, one sentence, and three or four short points: star and hide, find by what is in a photo, simple edits, drag into other programs. |
| Screenshots | The grid, the viewer, the map and Settings. |
| Privacy | The About box's network statement, word for word, plus: "No account. No telemetry. The Windows installer may download Microsoft's WebView2 if your computer does not have it." |
| Download | Two buttons, Windows and Linux, both linking to `https://github.com/funvill/Owlbum/releases/latest`. Beneath: the Windows SmartScreen note ("Windows may warn that the publisher is unknown, because the installer is not signed yet. Choose More info, then Run anyway.") and the Linux floor (Ubuntu 22.04, Debian 12, or newer). |
| Open source | "Owlbum is free and open source under the MIT licence." with links to the source, the issues and the third-party notices. |
| Footer | A link to GitHub and the copyright line. |

The buttons link to the release page rather than to a file, because file names carry the version and a static page cannot know it.

**Placeholder.** Until real screenshots exist the page ships with drawn placeholder frames labelled "Screenshot to come", and the download section reads "The first release is on its way." with a link to the repository. Replacing both is on the final review checklist. The site's colours follow [Visual design](09-visual-design.md).

## Tests

### The layers

| Layer | What it covers | Where | Tool |
|---|---|---|---|
| 1. Core unit tests | Pure rules: score, stacks, query parsing, the edit model, update rules, What's new rules, window restore, feedback address, log rotation. | `crates/owlbum-core/src/**` | `cargo test` |
| 2. Core integration tests | Whole features against a real album in a temp folder: scan, thumbnails, database, queries, edits, tagging with fake models. | `crates/owlbum-core/tests/` | `cargo test`, fixtures from `owlbum-fixtures` |
| 3. Shell tests | Commands through Tauri's mock runtime (`tauri::test`) where the wiring matters: argument checks, error mapping, menu text per update state, the capability file, the CSP string, the key table against the menu. | `src-tauri/src/**`, `src-tauri/tests/` | `cargo test` |
| 4. Page tests | Components and flows against a faked `ui/src/api/` module. No Tauri, no Rust. | `ui/src/**/*.test.tsx` | Vitest, Testing Library, jsdom |
| 5. Smoke test (optional) | The built program starts, opens the `trip` fixture, shows tiles, stars a photo, and quits cleanly. | `e2e/` | `tauri-driver` and WebdriverIO |

Rules for the layers:

- **A rule is tested at the lowest layer that can test it.** If it needs only a temp folder, it is a core test.
- **Page tests never import `@tauri-apps/api`.** They use the fake in `ui/src/api/fake/`, which implements the same TypeScript interface as the real module. The compiler therefore keeps the fake honest.
- **The updater is always faked** in tests, behind a small trait in the shell.
- **No test uses the network.** No test reads a personal photo. Model tests use tiny fake models; a test against real models runs only when `OWLBUM_TEST_MODELS_DIR` is set, and is skipped otherwise.
- **The smoke test is not required for an issue.** It is run by hand before a release, on both platforms.

### Running them

Tests run locally, on Windows and on Linux under WSL, and on GitHub Actions on Windows for every push to `main` and every pull request (`checks.yml`, see [Workflows](#workflows)).

| What | Command |
|---|---|
| Everything that must pass | `./scripts/check.sh` (Git Bash on Windows, bash on Linux) |
| Rust format | `cargo fmt --all --check` |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| TypeScript bindings are current | `cargo test -p owlbum --test bindings` |
| Rust tests, all crates | `cargo test --workspace` |
| Page types | `npm --prefix ui run typecheck` (`tsc --noEmit`) |
| Page lint | `npm --prefix ui run lint` (ESLint) |
| Page tests | `npm --prefix ui test` (Vitest, one run) |
| Page build | `npm --prefix ui run build` |
| Core tests only | `cargo test -p owlbum-core` |
| Rewrite the TypeScript bindings | `./scripts/bindings.sh` |
| Smoke test | `npm --prefix e2e test` after `npm --prefix ui run tauri build -- --debug` |

`scripts/check.sh` installs the page's packages when `ui/node_modules` is missing or older than `ui/package-lock.json`, then runs the eight rows after it, in that order, and stops at the first failure. There is one script for both systems; on Windows it runs under Git Bash. It points `CARGO_TARGET_DIR` at the main checkout's `target/` when the variable is not set, so every git worktree of a clone shares one build directory.

**Temporary folders.** `scripts/check.sh` runs the Rust tests with `TMP`, `TEMP` and `TMPDIR` pointing at a new folder of its own, fails when the tests leave anything in it, and then removes that folder only. Tests put albums and app data in a `tempfile::TempDir`, which deletes its folder when dropped and ignores a failure; on Windows the delete fails while a database inside is still open, so the `TempDir` must be dropped after everything that opened files in it. A fixture struct keeps its `TempDir` as its last field, since fields drop in order. Shell tests get their app from `common::window`, whose `TestApp` closes the album and takes the app data and thumbnail cache out of the app when dropped, because Tauri never frees what an app manages.

**Bindings.** The page's types for commands, events and their payloads are generated from Rust into `ui/src/api/bindings/` (with ts-rs) and committed. The command registry in `src-tauri/src/commands/` is the one source: `command!` declares a command, each area module lists its commands and events, and `src-tauri/src/bindings.rs` writes one file per type, one per area and an index. The bindings check fails when the committed files differ from what Rust would write; `scripts/bindings.sh` rewrites them. On a merge conflict inside `ui/src/api/bindings/`, take either side and run the script.

**Windows test binaries.** `src-tauri/build.rs` links `src-tauri/windows-app-manifest.xml` into every binary of the shell crate, tests included, instead of letting Tauri add it to the program only. Without it, shell test binaries fail to start (`STATUS_ENTRYPOINT_NOT_FOUND`).

**WSL.** Use a second clone inside the Linux file system (for example `~/owlbum`), not the Windows folder through `/mnt/c`. A shared folder is slow, and `node_modules` and `target` hold platform-specific files that must not be mixed. Install Tauri's Linux build packages once. Layers 1 to 4 need no display. The smoke test needs `webkit2gtk-driver` and either WSLg or `xvfb-run`.

### Fixtures

`owlbum-fixtures` is described in [Data](02-data.md). Integration tests call it as a library and generate what they need into a temp folder, so no step has to run first and nothing generated is committed. For looking at by hand:

```text
cargo run -p owlbum-fixtures -- trip <folder>
cargo run -p owlbum-fixtures -- large 100000 <folder> [--seed <n>] [--megapixels <n>[-<n>]]
cargo run -p owlbum-fixtures -- extras <folder>
```

The folder must be missing or empty; the generator never writes over existing files. The same seed gives the same bytes on Windows and Linux. A test pins the `trip` album's file list, sizes and a hash of every byte, so that an accidental change to the generator is noticed. The generator and the image crates it uses are built optimised even in the dev profile (a `trip` album takes about half a second); `large 100000` took 103 seconds on Windows and about 430 MB of disk (mostly file-system overhead: the files average 1.5 KB).

### Definition of done

An issue is done when all of these are true:

1. **Tests were written first.** The first commit on the issue adds tests that fail for the right reason. The commits after make them pass. The issue's closing comment names the test files.
2. The behaviour matches these documents. Where it could not, the document is changed in the same piece of work and the reason is a comment on the issue.
3. `scripts/check.sh` passes on Windows (Git Bash) and in WSL. Both results are stated in the closing comment.
4. Every new interface string is in the text module, and every new key is in the key table.
5. Nothing new uses the network. If it must, [Architecture](01-architecture.md)'s network table and the About statement change with it.
6. A feature that cannot be made reliable on Linux ships disabled there with an issue open, as the README allows.
7. A change a user would notice has a line in the `Unreleased` section of `CHANGELOG.md`.

### Versions and releasing

Versions follow `major.minor.patch` and start at `0.1.0`. The version is not bumped on every commit. It is bumped when a release is made.

1. Run `scripts/check.sh` on Windows and in WSL. Run the smoke test on both.
2. Set the new version in the root `Cargo.toml` and run `cargo check` so `Cargo.lock` follows.
3. Rename `CHANGELOG.md`'s `Unreleased` section to the version and its date; What's new shows it.
4. Commit as `chore: version <version>`.
5. Tag that commit `v<version>` and push the tag.
6. `release.yml` builds the installers and creates the draft release.
7. **A person** installs each installer from the draft on a clean Windows and Linux machine, checks that Owlbum starts and opens a folder, and records the installer sizes.
8. **A person** edits the release notes and presses Publish. From then on installed copies are offered the update.

Done once, at final review, by a person: create the signing key, add the two secrets, put the public key in `tauri.conf.json`, switch on GitHub Pages, and test one real update from version `0.1.0` to a test version using `OWLBUM_UPDATE_FEED`.

## Environment variables

| Variable | Read by | Effect |
|---|---|---|
| `OWLBUM_DATA_DIR` | Shell, tests | The app data folder. Also switches off single-instance handling. |
| `OWLBUM_NO_UPDATE_CHECK` | Shell | Any non-empty value stops the quiet update check. An empty value is the same as unset. |
| `OWLBUM_UPDATE_FEED` | Shell | Replaces the update feed address. For testing. |
| `OWLBUM_NO_WHATS_NEW` | Shell | Any non-empty value stops What's new from showing at start. |
| `OWLBUM_LOG` | Shell | `debug` adds debug lines to the log. |
| `OWLBUM_TEST_MODELS_DIR` | Tests | Points the real-model test at downloaded models. Unset skips it. |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | `release.yml` | The updater signing key. Secrets; never set on a developer's computer for ordinary work. |
| `LOCALAPPDATA` | Shell (Windows) | The default app data location. |
| `XDG_DATA_HOME` | Shell (Linux) | The default app data location. |
| `APPIMAGE` | Shell (Linux) | Set by the AppImage runtime. Tells the updater which file to replace. |

## Commands and events this part adds

The events `open_folder` and `update_state` are listed in [Architecture](01-architecture.md#events). Other `app_` commands are defined by the part that owns their behaviour: `app_settings_get`, `app_settings_set`, `app_recent_list`, `app_recent_remove`, `app_session_save` and `app_place_search` in [Browsing](04-browsing.md#commands-and-events); `app_formats` in [Media and scanning](03-media-and-scanning.md#commands-events-and-urls); `app_capabilities` in [Editing and file actions](06-editing-and-file-actions.md#commands-and-events). A value outside its range given to `app_settings_set` answers `setting_out_of_range` and changes nothing.

| Command | Arguments → reply | Purpose |
|---|---|---|
| `app_startup` | → `{ version, platform, arch, package, dev, settings, recent: Recent[], launch_folder: string \| null, reopen: string \| null, show_update_notice, whats_new? }` | The page has drawn. Shows the window and hands over what the page needs to begin. `settings` and `Recent` are the shapes in Browsing. `package` is `nsis`, `deb`, `appimage`, or `dev` for a development build. `launch_folder` is a folder from the command line, or one that arrived before this call (a folder dropped while the page loaded, or a second launch). `reopen` is the folder to reopen by the rule in [Which album opens at start](#which-album-opens-at-start). `show_update_notice` is true while `updates.notice_seen` is false. |
| `app_version` | → `{ version }` | The running version. Added by the scaffold (#1) to prove the command pattern end to end; `app_startup` carries the same value. |
| `app_pick_folder` | → `path` or null | The native "Open a folder of photos" dialog, for the page's own Open folder button. |
| `app_close_ready` | | The page has finished edit mode, the session save and any running save, after `app_closing`. |
| `app_check_updates` | | The manual check. Its result arrives as `update_state` events and the dialogs under [Messages](#messages). |
| `app_update_notice_answer` | `keep_on` | Records the answer to the first-start notice. |
| `app_update_download` | | Starts the download of the offered version. |
| `app_update_install` | | Runs the install steps. |
| `app_about` | → `{ version, platform, arch, package, data_dir, copyright, repository, has_notices }` | The About box's facts; the page holds the words. `copyright` is the line from `LICENSE`; `repository` is the project's address, shown as the link's text; `has_notices` says whether the notices file is among the resources. Whether the updater can run comes from `app_update_state`. |
| `app_update_state` | → `{ state, version?, percent? }` | The updater's state now. |
| `app_whats_new` | → entries | The running version's notes. |
| `app_whats_new_seen` | | Records `whats_new.last_seen`. |
| `app_cache_usage` | → `{ bytes, limit_bytes }` | For Settings. |
| `app_cache_clear` | → `{ bytes_freed }` | Empties the thumbnail cache. |
| `app_open_data_folder`, `app_open_log_folder` | | Open a folder in the file manager. |
| `app_open_licence`, `app_open_notices` | | Open a resource text file. |
| `app_open_feedback`, `app_open_repository`, `app_open_release_page`, `app_open_release_notes` | the last takes no address, only the offered version held by Rust | Open one fixed address in the user's browser. `app_open_feedback` and `app_open_repository` are built (#61); the opener is the shell's `Browser` state, which shell tests replace. |
| `app_copy_details` | | Copies the fatal error's technical block. |
| `app_restart`, `app_quit` | | Used by the fatal dialog and File > Quit. |
| `app_log` | `level`, `message` | The page reports an error or warning for the log. Limited to 2,000 characters a line and 100 lines a minute. |

| Event | Payload | When |
|---|---|---|
| `app_menu` | `{ item }` | A menu item that acts in the page was chosen. `item` is one of `close_folder`, `rescan`, `shortcuts`, `update`, `whats_new`, `about`. |
| `app_closing` | `{}` | The window is about to close, for a close, a quit or an update install. The page answers with `app_close_ready`. |
| `app_fatal` | `{ title, message }` | A fatal error of phase `run` that the page should show. `message` holds the message and the advice. |
| `update_message` | `{ kind, … }` | What a check, download or install came to; the page shows the matching message of [Messages](#messages). |

`update_state.state` is one of `unavailable`, `idle`, `checking`, `available`, `downloading`, `ready`, `installing`.

Error codes this part adds to `AppError`: `update_unavailable`, `update_check_failed`, `update_download_failed`, `update_install_blocked`, `update_install_failed`, `setting_out_of_range`, `cannot_open_folder`.

## Open questions

1. **Native accelerators while the webview has focus.** F1, F2, F11, `Ctrl+O`, `Ctrl+W` and `Ctrl+Q` are specified as native menu accelerators. Whether they fire reliably on both platforms while focus is inside the webview needs a prototype. If they do not, the page handles them from the same key table and the menu only displays them.
2. **The native fatal message box.** Tauri's dialog plugin offers at most three custom buttons. Whether that form behaves the same on Windows and Linux, and what closing it by the title bar returns, needs checking. So does whether Tauri reports the webview process ending; if not, phase `page` rests on the 30-second `app_startup` rule alone.
3. **OpenStreetMap's tile rules.** The old program sent a bare `User-Agent` and almost no other headers. Owlbum cannot rewrite the webview's request headers, so it adds its name to the user agent and sends no referrer. Whether that satisfies the tile usage policy for a desktop program, and whether the webview's own tile cache is enough to meet its caching rule, should be confirmed before release.
4. **Installer sizes.** Unknown until the first build. Proposed targets: Windows installer under 40 MB, `.deb` under 40 MB, AppImage under 120 MB with the media framework bundled. ONNX Runtime is the largest single part; whether it is linked in or shipped as a library, and how much it adds, comes from the tagging prototype.
5. **AppImage video.** Bundling the media framework makes the AppImage much larger. If video through the system's own codecs proves reliable without it, the setting can be turned off. This depends on the video work in [Media and scanning](03-media-and-scanning.md).
6. **Entries in `latest.json`.** The updater must pick the `.deb` on a `.deb` install and the AppImage on an AppImage. The exact entry names that the pinned `tauri-action` and updater versions agree on need confirming with a real draft release, and the final job's check should name them.
7. **Symbol keys on other keyboards.** `[`, `]` and `\` need AltGr on several European layouts, and AltGr reaches the page as Ctrl+Alt on Windows. The matching rule allows for it, but whether these keys are comfortable enough, or need alternatives, should be decided with [Editing and file actions](06-editing-and-file-actions.md).
8. **Windows 10 without WebView2.** The installer downloads WebView2 from Microsoft when it is missing. That is a network request Owlbum's own statement does not cover, because the installer makes it, not Owlbum. The site says so. Whether to ship the larger offline installer instead is open.
