# Changelog

All notable changes to Owlbum are written down in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and Owlbum uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Versions below 1.0.0 are test builds: each one raises the last number, and
the window's title shows the version that is running.

## [Unreleased]

### Added

- Show on map: beside Pick on map in the details panel, for a photo with a
  place. It switches to the map with the photo's marker in the middle.
- The photo menu opens with a right-click in the full-screen viewer and on
  a photo's marker on the map.
- Photos can be dragged out of the preview into other programs, as from a
  tile, while the preview is not zoomed in.
- Videos the system shell has no thumbnail for get their poster frame
  from Windows' own video decoders (Media Foundation), a second into the
  video.
- The details of MKV and WebM videos name their video and audio codecs.

### Changed

- The window title names the open folder after the version, for example
  "Owlbum 0.1.3 - C:/Photos/Trip"; the folder is no longer shown in the
  top bar.
- The search box moved into the top bar, at its left, in line with Open
  folder and Close folder.
- Sort moved beside the Grid / Timeline / Map switch, shown while the grid
  is the view, the way Day / Month sit there for the timeline.
- The details panel's Copy button moved into the Photo section, under the
  photo score. Its buttons have icons as well as words, including
  Regenerate and the date's "Use the camera's date".
- Copy (Ctrl+C) of one photo copies the picture with its edit applied,
  as Owlbum shows it.
- A key that does something on the review screen closes the photo menu
  and then does it, instead of being ignored while the menu is open.
- Large albums open about twice as fast (under a second to the first
  screen at 100,000 photos), and sorting by score, most interesting or
  colour with stacks on is several times quicker.

## [0.1.3] - 2026-11-27

### Added

- Search by meaning: when a keywords model is chosen, searching for words
  of 3 letters or more also finds photos whose picture is close to what you
  typed, even when no word matches. They follow the word matches on a fresh
  row under "Also matching by meaning", best match first, at most 200, and
  respect the other filters. Copies of one picture show once.
- After an update, Owlbum shows what changed since the version you last
  ran. Help > What's new shows this version's notes at any time.
- Updates: Help > Check for updates, Check now in Settings and the About
  box look for a new version on GitHub. A new version can be downloaded
  and installed from Owlbum, which closes and starts again. The daily check
  starts only after you answer the notice on the first start, and can be
  turned off. Until the project's release signing is set up, Owlbum says
  it cannot update itself.
- The About box has a Licence button.
- If Owlbum fails badly it shows a message, with Copy details, Open the
  log folder and Restart, instead of vanishing. Errors in the window are
  written to Owlbum's log.
- F6 and Shift+F6 move the keyboard focus between the top bar, the filter
  bar, the grid, the preview and the details panel, and back to where it
  was in each.
- Settings > Photo tagging: choose Off or a model for descriptions and for
  keywords, download a model from Hugging Face after a yes, follow its
  progress, cancel it, and remove a model you no longer want.
- A tagging bar under the top bar while photos are being tagged, with
  Pause and Resume, the count of photos that could not be tagged and Try
  failed photos again, and a clear line when tagging stopped or a model is
  not working.
- "What's in it" in the details panel: the description of the photo, its
  keywords and the objects in it. Click a keyword or object to show only
  photos with it; click again to stop. Regenerate asks the model for a
  new description.
- The photos you are looking at are tagged first.
- A Windows installer (Owlbum_<version>_x64-setup.exe) that installs for
  the current user without an administrator prompt, fetches WebView2 from
  Microsoft if Windows 10 lacks it, and carries the licence, the
  third-party notices and ONNX Runtime, so photo tagging works in an
  installed Owlbum.

### Changed

- The app window is locked down: it loads nothing from the internet except
  map tiles (after you say yes), never leaves its own page or opens new
  windows, and the page can only call Owlbum's own commands. Links such as
  the project page still open in your browser.

### Fixed

- Closing Owlbum stops photo tagging and any model download first, and
  does not wait forever for a model that hangs.

## [0.1.2] - 2026-11-27

### Added

- Pick on map, in the Place section of the photo info: a map with the
  album's other located photos, where a click sets where the photo was
  taken (or the whole selection's place). Ctrl+Z undoes it. Map tiles still
  load only after you say yes.
- F1 or Help > About Owlbum shows the About box, with the owl logo, the
  version, where Owlbum keeps its data and a link to the project on
  GitHub.
- F2 or Help > Give feedback on GitHub opens the project's issues page in
  your browser.
- A menu bar: File (Open folder, Close folder, Rescan, Open the data
  folder, Quit), View (Full screen) and Help (Keyboard shortcuts, Check
  automatically, What's new). Ctrl+O opens a folder, Ctrl+W closes it and
  goes back to the start screen, Ctrl+Q quits and F11 makes the window full
  screen.
- Settings, from the gear at the right of the top bar: reopen the last
  album, the thumbnail cache's size, limit and Clear cache, whether edits
  stay in the album or are saved into the photo file, loading maps from the
  internet, and checking for updates.
- Filter by media type: tick off Stills, Animated, RAW or Videos in the
  new Type group of the filter panel (for example, no videos), or pick
  file formats to show only those (for example, only PNGs). Each has its
  count, shows as a chip, and can be typed in the search box as
  kind:video, no:video or format:png.
- The details panel has a Copy button that copies everything it shows as
  plain text, a Path row with the file's full path and a button to copy
  it, and an EXIF section listing every tag in the file.
- Press ? for a list of every keyboard shortcut, with the keys for where
  you are first. It has a button that opens Settings.
- Starting Owlbum again, or opening a folder with it, brings the running
  window to the front and opens that folder there.

### Changed

- New program icon: a gold owl on a dark tile.
- The window opens where it was last closed, at the same size, maximised if
  it was. It starts at 1440 by 900 and cannot be made smaller than 960 by
  600.
- Quit from the menu finishes edits and saves, and asks about unsaved edits,
  just as closing the window does.
- The search box is now at the left of the filter bar, with Sort, Stack
  similar, Filters and the count at the right. Sort shows only in the grid;
  the timeline and the map no longer show it.
- After Up or Down the photo shows only the change, such as +1, and quick
  taps in one direction add up (+5), rather than the new total.
- The star, hide and score marks that flash over a photo are now 40% of
  the photo's shown size, so they are much easier to see.
- In the photo score's pop-up each label sits at the top of its row, and a
  total below zero is shown in red.

### Removed

- The file name at the top of the details panel and its Close details
  button. Ctrl+D still clears the selection.

### Fixed

- A right-click on the preview showed two menus on top of each other:
  Owlbum's photo menu and the window's own. Only the photo menu shows now.

## [0.1.1] - 2026-11-27

### Added

- The photo menu (right-click or the menu key): Show in folder, Copy path,
  Rotate left and right, Flip, Edit, Save, Save as, Save all and Revert to
  original.
- Delete moves photos to the Recycle Bin, after asking. A drive without a
  Recycle Bin refuses, so nothing is ever deleted outright.
- Ctrl+C copies one photo as a picture or several as files. Photos can be
  dragged out of the grid into Explorer and other programs.
- Edit mode (E): crop, straighten, rotate, flip, light and colour, and Auto.
  Hold `\` or press Before to see the original. Edits show on tiles, in the
  preview, the viewer, the map and the zoom mini map.
- `[` and `]` rotate the selected photos without opening edit mode.
- Saving edits: Save, Save as, Save all with progress, and Revert. Saved
  files keep the original's EXIF, XMP and colour profile, and PNG keeps
  16-bit colour. An interrupted save is finished or undone the next time the
  album opens, and the original goes to the Recycle Bin.
- Closing the window while a photo is being edited asks first, instead of
  losing the edit.
- Animated GIF and WebP play in the preview and the viewer.
- Camera RAW files, using the camera's own preview when it is big enough.
- Video files: length, size, date and place, a thumbnail from Windows, and
  playback through the system's codecs.
- A notice for files Owlbum cannot show yet (HEIC, AVIF) or could not read,
  with a list that can be copied.
- The review screen's panels can be resized by dragging, and keep their
  widths.
- The timeline has a month rail for jumping between months.
- The window title shows Owlbum's version. This changelog.

### Changed

- In a narrow window the search box shrinks first, then the hint line
  hides, so the filter bar still fits.
- Opening or folding a stack keeps the grid where it was. A selected photo
  that gets folded is replaced by the stack's top photo.
- The map adds its markers a slice at a time, so 100,000 photos no longer
  hold the window up.

## [0.1.0] - 2026-11-26

The first test build: milestones 0 (foundation), 1 (browse) and 2
(organise).

### Added

- Open a folder as an album; it is scanned into `.owlbum/owlbum.sqlite` and
  watched for changes. Recent albums on the start screen.
- The grid, selection, preview with zoom, full-screen viewer and details
  panel, with typed info (description, notes, people, date, place).
- Star, hide, photo score, sorting, bursts and stacks, filters, query words,
  text search, undo, and Tidy.
- Timeline and map views, and reopening where you were.

[Unreleased]: https://github.com/funvill/Owlbum/compare/6ffea27...HEAD
[0.1.3]: https://github.com/funvill/Owlbum/tree/6ffea27
[0.1.2]: https://github.com/funvill/Owlbum/tree/308f9d2
[0.1.1]: https://github.com/funvill/Owlbum/tree/485f1a7
[0.1.0]: https://github.com/funvill/Owlbum/tree/7060f13
