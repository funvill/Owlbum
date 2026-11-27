# Changelog

All notable changes to Owlbum are written down in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and Owlbum uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Versions below 1.0.0 are test builds: each one raises the last number, and
the window's title shows the version that is running.

## [Unreleased]

### Added

- A menu bar: File (Open folder, Close folder, Rescan, Open the data
  folder, Quit), View (Full screen) and Help (Keyboard shortcuts, Check
  automatically, What's new). Ctrl+O opens a folder, Ctrl+W closes it and
  goes back to the start screen, Ctrl+Q quits and F11 makes the window full
  screen.
- Settings, from the gear at the right of the top bar: reopen the last
  album, the thumbnail cache's size, limit and Clear cache, whether edits
  stay in the album or are saved into the photo file, loading maps from the
  internet, and checking for updates.
- Press ? for a list of every keyboard shortcut, with the keys for where
  you are first. It has a button that opens Settings.
- Starting Owlbum again, or opening a folder with it, brings the running
  window to the front and opens that folder there.

### Changed

- The window opens where it was last closed, at the same size, maximised if
  it was. It starts at 1440 by 900 and cannot be made smaller than 960 by
  600.
- Quit from the menu finishes edits and saves, and asks about unsaved edits,
  just as closing the window does.

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

[Unreleased]: https://github.com/funvill/Owlbum/compare/485f1a7...HEAD
[0.1.1]: https://github.com/funvill/Owlbum/tree/485f1a7
[0.1.0]: https://github.com/funvill/Owlbum/tree/7060f13
