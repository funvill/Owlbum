# Changelog

All notable changes to Owlbum are written down in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and Owlbum uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Versions below 1.0.0 are test builds: each one raises the last number, and
the window's title shows the version that is running.

## [Unreleased]

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

[Unreleased]: https://github.com/funvill/Owlbum/compare/308f9d2...HEAD
[0.1.2]: https://github.com/funvill/Owlbum/tree/308f9d2
[0.1.1]: https://github.com/funvill/Owlbum/tree/485f1a7
[0.1.0]: https://github.com/funvill/Owlbum/tree/7060f13
