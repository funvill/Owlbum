# Data

Owlbum keeps data in two places:

- **The album database**, `.owlbum/owlbum.sqlite` in the album root. It is the only copy of what is known about the photos.
- **The app data folder**, per user. It is a cache plus settings; deleting it loses nothing about any photo.

## The album database

### Where it lives

```text
<album root>/
  photo files and subfolders…
  .owlbum/
    owlbum.sqlite            the database
    lock                     who has the album open for writing (present only while open)
    backup/
      owlbum-2026-11-25.sqlite   daily copies, newest three kept
```

On Windows the `.owlbum` folder is given the hidden attribute when it is created. The scanner never looks inside it.

### Finding the album root

When the user opens folder `F`:

1. Walk upward from `F` to the drive root. The nearest folder that contains `.owlbum/owlbum.sqlite` is the album root. `F` itself counts.
2. If none is found, `F` becomes a new album root and the database is created there.
3. If the root is an ancestor of `F`, the album opens **scoped** to `F`: only photos under `F` are scanned and shown. The top bar shows both the album root and the scope; there is no banner for it.

If a folder inside an album later gets its own `.owlbum` (for example a subfolder that was an album before its parent was opened), the nearest one wins for photos beneath it, and the outer album skips that subtree. Rows the outer album already held for that subtree become missing there. Owlbum never merges two databases and never moves rows between them.

### Opening, locking and view-only

The database is opened with `journal_mode = DELETE` and `synchronous = FULL`. Write-ahead logging is not used, because its extra files do not work on network shares and confuse sync tools. Between sessions the only file is `owlbum.sqlite`.

The `lock` file is JSON: `{ "host", "pid", "user", "app_version", "opened_at", "heartbeat" }`. The holder rewrites `heartbeat` every 30 seconds and deletes the file on close.

| Situation on open | Result |
|---|---|
| No lock file | Take the lock; open for writing. |
| Lock from this host whose process is no longer running | Stale: replace it; open for writing. |
| Lock from another host with a heartbeat under 2 minutes old | Open **view-only**. The banner names the other computer. |
| Lock from another host with an older heartbeat | Ask: "This album looks open on <host>, last seen <time>." with **Open anyway** (replaces the lock) and **Open view-only**. |
| The folder or the database cannot be written | Open **view-only**. The banner says the folder is read-only. |
| No database and the folder cannot be written | Open view-only with an in-memory database for this session; nothing is saved. |

**View-only** means browsing, filtering, the viewer, the map, copy, drag-out, Save as… (to another place) and search by meaning over existing vectors all work. Star, hide, notes, edits, moving to the Recycle Bin, tagging and tidy are disabled. Each disabled control's tooltip is the first sentence of the banner in force. Scan facts for files the database does not know are kept in memory for the session, with ids above the database's largest.

`album_status` reports why an album is view-only, as one of: `read_only` (the folder cannot be written), `locked` (another computer has it), `chosen` (the user picked Open view-only; the banner offers **Open for changes**), `newer` (made by a newer Owlbum), `damaged` (the check failed).

### Backups and damage

- On the first writable open of each calendar day, the database is copied to `.owlbum/backup/owlbum-<date>.sqlite` using SQLite's backup API. The newest three are kept. The copy leaves out the `vectors` table, which can be made again by tagging and would otherwise add hundreds of megabytes to each backup of a large album.
- On every open, `PRAGMA quick_check` runs. If it fails, Owlbum does not write to the database. It offers: restore the newest backup (the damaged file is renamed `owlbum.damaged-<timestamp>.sqlite`, never deleted), or open view-only.
- If a file matching a sync tool's conflict naming sits beside the database (for example `owlbum (conflicted copy).sqlite`, `owlbum-PC-NAME.sqlite`), the banner warns that another computer changed this album at the same time and names the file. Owlbum does not merge it.

### Schema

`PRAGMA user_version` holds the schema version, starting at 1. Migrations are numbered SQL scripts (`crates/owlbum-core/src/album/migrations/`) compiled into the program and run in a transaction on open; a file at version 0 is treated as new. A database with a version newer than the program knows opens view-only with the message "This album was last used by a newer Owlbum."

```sql
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
-- keys: album_id (a UUID made at creation), created_at (ms since the epoch, as text),
--       created_by (app version),
--       save:<photo id> (a save-into-file in progress; see Editing and file actions)

CREATE TABLE photos (
  id            INTEGER PRIMARY KEY,
  rel_path      TEXT NOT NULL UNIQUE,   -- relative to the album root, '/' separators
  rel_dir       TEXT NOT NULL,          -- '' for the root
  file_name     TEXT NOT NULL,
  missing       INTEGER NOT NULL DEFAULT 0,  -- 1 when the file is gone: not found by a scan, or moved to the Recycle Bin

  -- identity and change detection
  size          INTEGER NOT NULL,
  mtime_ms      INTEGER NOT NULL,
  file_key      TEXT NOT NULL,          -- 'fk1:<size>:<sha1 of size, first 16 KiB, last 16 KiB>'
  thumb_key     TEXT NOT NULL,          -- sha1(album_id | file_key | mtime_ms), hex; names the thumbnails

  -- scan facts
  kind          TEXT NOT NULL,          -- 'image' | 'raw' | 'video'
  format        TEXT NOT NULL,          -- registry id: 'jpeg', 'png', 'cr3', 'mp4', …
  width         INTEGER NOT NULL,       -- upright, after orientation; 0 when the file could not be read
  height        INTEGER NOT NULL,
  duration_ms   INTEGER,                -- video, and animated stills (kind 'image' with a duration is animated)
  captured_at   TEXT,                   -- 'YYYY-MM-DDTHH:MM:SS', wall-clock, no zone
  gps_lat       REAL,
  gps_lon       REAL,
  camera        TEXT,
  exif          TEXT,                   -- JSON; key names are fixed in Media and scanning
  analysis      TEXT,                   -- JSON; key names are fixed in Media and scanning.
                                        -- NULL with no scan_error means "waiting for the pixels pass"
  scan_version  INTEGER NOT NULL,       -- bumped in code when the analysis changes, to force a re-read
  scan_error    TEXT,                   -- '<code>: <sentence>' when the file could not be read

  -- typed info
  star          INTEGER NOT NULL DEFAULT 0,
  hidden        INTEGER NOT NULL DEFAULT 0,
  score_adjust  INTEGER NOT NULL DEFAULT 0,
  date_override TEXT,                   -- same form as captured_at
  loc_lat       REAL,                   -- overrides GPS when both are set
  loc_lon       REAL,
  loc_name      TEXT,
  description   TEXT,
  notes         TEXT,
  edit          TEXT,                   -- JSON PhotoEdit applied when the photo is shown; NULL for none
  info_updated_at INTEGER               -- ms since the epoch, last change to typed info
);
CREATE INDEX photos_dir       ON photos(rel_dir);
CREATE INDEX photos_file_key  ON photos(file_key);
CREATE INDEX photos_thumb_key ON photos(thumb_key);
CREATE INDEX photos_captured  ON photos(coalesce(date_override, captured_at));

CREATE TABLE people (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE      -- first spelling wins
);
CREATE TABLE photo_people (
  photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  PRIMARY KEY (photo_id, person_id)
) WITHOUT ROWID;

CREATE TABLE tags (
  photo_id    INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  part        TEXT NOT NULL,            -- 'describe' | 'keywords'
  model_id    TEXT NOT NULL,
  status      TEXT NOT NULL,            -- 'done' | 'failed'
  description TEXT,                     -- describe
  objects     TEXT,                     -- describe: JSON [{label, count}]
  keywords    TEXT,                     -- keywords: JSON [{label, score}]
  error       TEXT,
  made_at     INTEGER NOT NULL,
  PRIMARY KEY (photo_id, model_id)
) WITHOUT ROWID;

CREATE TABLE vectors (
  photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  model_id TEXT NOT NULL,
  dim      INTEGER NOT NULL,
  vector   BLOB NOT NULL,               -- little-endian float32, unit length, dim × 4 bytes
  PRIMARY KEY (photo_id, model_id)
) WITHOUT ROWID;

CREATE TABLE photo_labels (             -- one row per keyword or object, for exact tag filters
  kind     TEXT NOT NULL,               -- 'keyword' | 'object'
  label    TEXT NOT NULL,               -- lower case
  photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  PRIMARY KEY (kind, label, photo_id)
) WITHOUT ROWID;

CREATE VIRTUAL TABLE search USING fts5(
  file_name, description, notes, people, place, tag_description, keywords, objects,
  content='', contentless_delete=1, prefix='2 3',
  tokenize='unicode61 remove_diacritics 2'
);
-- rowid = photos.id; kept in step by the writer thread whenever a source field changes
```

`photo_labels` and the three tag columns of `search` are derived from `tags`. When a photo has results from more than one model for a part, they use the newest `done` row for that part. The details panel prefers the row of the model chosen in Settings, so the two can differ briefly after a model is switched back.

Rules:

- **Effective values.** The date shown and sorted by is `date_override`, else `captured_at`. The place is `loc_lat`/`loc_lon` when both are set, else the GPS pair.
- **Booleans** are 0 or 1. **Empty text** is stored as NULL, never `''`.
- **`edit`** is never an identity edit; an edit that changes nothing is stored as NULL.
- **`score_adjust`** is an integer with absolute value at most 100,000.
- **Vectors** are checked on write: `length(vector) = dim * 4`, `dim` equals the model's `dim` in the catalogue, and every value is finite. Search by meaning ignores vectors whose `dim` differs from the query's.
- **Tag rows.** An empty `objects` or `keywords` list is stored as NULL. A `done` keywords row and its `vectors` row are written in one transaction. A `done` describe row may have a NULL description.
- **Duplicate pictures** (same bytes under two names) are two rows. Tagging copies results between rows that have the same `file_key` **and** the same `dHash`, instead of running the model twice. The `file_key` alone reads only 32 KiB of the file and is not enough to call two pictures the same.
- **A file that changes in place.** When a scan finds a known path with a new `file_key`, the writer deletes that photo's `tags`, `vectors` and `photo_labels` rows and clears its analysis. Typed info is kept.
- **People.** A name no photo uses any more is deleted from `people`.
- **Rows with `scan_error`** are left out of every result, like missing rows. They are counted and listed (see [Media and scanning](03-media-and-scanning.md)).
- **Unsupported files** (HEIC, AVIF and other recognised but unsupported types) get no row. Each scan counts and lists them in memory. The one exception is a file with a supported extension whose content is an unsupported type (a HEIC named `.jpg`): it gets a row with `scan_error` code `unsupported`.
- **An album with no database on disk** (a read-only folder opened for the first time) is reported by `album_status` as `in_memory`, alongside the `read_only` reason.
- **Everything goes through the writer thread** in short transactions. A typed change is committed before the command that made it returns.

### Identity: renames, moves and missing files

A photo's row is found by `rel_path`. The scanner keeps rows attached to files that were renamed or moved outside Owlbum:

1. After the walk, any row whose path was not found is a candidate **missing** row.
2. Any file found with no row is a candidate **new** file. Its `file_key` is computed (this reads at most 32 KiB).
3. Missing rows with the same `file_key` are the candidates for that file. Rows already marked missing by an earlier scan count too, so a move seen in two halves by the folder watcher still keeps its row. A candidate with the same file name is preferred; then the first in path order. Each row is given to at most one file, and files are taken in walker order. The chosen row's `rel_path`, `rel_dir` and `file_name` are updated and everything else is kept.
4. A file with no candidate gets a new row, and unmatched missing rows are marked `missing = 1`.

Missing rows keep their typed info and tags. They are left out of every result. If the file comes back, the row is used again. Moving a photo to the Recycle Bin marks its row missing at once, so restoring the file from the bin restores its star and notes. **Tidy** (see Organising) lists missing rows and deletes the ones the user confirms. In a scoped open, only rows under the scope are considered.

### What the database never holds

- Thumbnails or any pixels.
- Absolute paths, computer names or user names (the `lock` file does, and it is removed on close).
- App settings.

## The app data folder

| Platform | Location |
|---|---|
| Windows | `%LOCALAPPDATA%\OwlbumData` |
| Linux | `$XDG_DATA_HOME/owlbum`, else `~/.local/share/owlbum` |

On Windows the folder is deliberately not `%LOCALAPPDATA%\Owlbum`, because a per-user installer puts the program itself there. A development build uses `OwlbumData Dev` / `owlbum-dev` so it never touches an installed copy. `OWLBUM_DATA_DIR` overrides the location and switches off single-instance handling (used by tests). Uninstalling leaves app data alone.

```text
<app data>/
  app.sqlite               settings, recent albums, session, thumbnail index
  thumbs/<aa>/<thumb_key>_s.jpg, _m.jpg     thumbnails, fanned out by the key's first two characters
  full/<thumb_key>.jpg     full-size upright copies made for zooming
  renders/                 edited files waiting to be moved into place; swept after an hour,
                           except a file named by a save record in an album's meta table
  models/<model id>/…      downloaded tagging models, their .part files and a verified.json marker
  webview/                 the webview's own data
  logs/owlbum.log          and owlbum.previous.log; rotated at 2,000,000 bytes when first opened in a run
```

If the app data folder cannot be written, logs go to a folder in the system temp directory. A damaged `app.sqlite` is renamed `app.damaged-<timestamp>.sqlite` and a new one is made; only preferences and the recent list are lost.

### Thumbnail keys and the cache

`thumb_key = sha1(album_id | file_key | mtime_ms)` in hex, stored on the photo's row. It changes when the file's content or modified time changes, and it is the same for a file that was only renamed or moved, so a rename costs no new thumbnail.

`app.sqlite` has a table of cache entries (`thumb_key`, `kind`, `bytes`, `last_used`), where `kind` is `s`, `m` or `full`. The cache has a size limit, 20 GB by default and changeable in Settings (whole gigabytes, 1 to 1000). When it is exceeded, the least recently used entries are deleted, `full` first. A thumbnail that is missing is made again on demand, so pruning is always safe. A scan stops making medium thumbnails ahead of need once the cache is 70% full; they are then made on demand. Settings shows the cache's size and has **Clear cache**, which deletes `thumbs/`, `full/` and their entries.

### `app.sqlite`

```sql
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);   -- JSON values
CREATE TABLE recent (
  root       TEXT PRIMARY KEY,     -- absolute path of the folder that was opened
  opened_at  INTEGER NOT NULL,
  session    TEXT                  -- JSON: view, sort, filter, the current photo's rel_path, and the
                                   -- index of the first photo in view. Unknown fields are kept.
);
CREATE TABLE cache_entries (
  thumb_key TEXT NOT NULL, kind TEXT NOT NULL, bytes INTEGER NOT NULL, last_used INTEGER NOT NULL,
  PRIMARY KEY (thumb_key, kind)
) WITHOUT ROWID;
```

Settings keys (each has a default in code; an unknown key is ignored and kept):

| Key | Default | Meaning |
|---|---|---|
| `thumb_size` | 160 | Tile size in the grid, 96 to 320. |
| `panel.grid_width`, `panel.details_width` | 420, 340 | Column widths. |
| `sections` | all open | Which details sections are folded. |
| `view` | `grid` | `grid`, `timeline` or `map`. Used for an album with no saved session; a reopened album uses its session's view. |
| `timeline_unit` | unset | `day` or `month`, once the user has picked one. |
| `stack_similar` | true | |
| `maps_online` | unset | The answer to "Load maps from the internet?". |
| `edit_target` | `album` | Where an edit ends up: `album` (kept in the database; the file is never touched) or `file` (Save writes it into the photo file). |
| `reopen_last` | true | Reopen the last album at start. |
| `updates.auto` | true | Check for updates once a day. |
| `updates.last_check` | null | Milliseconds since the epoch. |
| `updates.notice_seen` | false | The first-start note about the update check has been shown. |
| `tagging.describe`, `tagging.keywords` | null, null | Chosen model id for each part, or null for Off. There is deliberately no saved "paused" setting; pause lasts until Resume or the next album. |
| `cache.limit_gb` | 20 | Whole number, 1 to 1000. |
| `window` | 1440 × 900 | `{ x, y, width, height, maximised }` in physical pixels. |
| `whats_new.last_seen` | null | The last version whose notes were shown. |

The recent list keeps 12 entries; an entry can be removed from the start screen. The page keeps nothing in `localStorage`; every preference goes through `app_settings_get` and `app_settings_set`.

## Test fixtures

`owlbum-fixtures` generates a deterministic synthetic album (`trip`, about two dozen files) and a large one (`large-N`) from a seeded generator. The trip album covers: mixed orientations, a panorama, a burst of near-duplicates, a blurred frame, an EXIF-rotated file, an exact duplicate, a low-resolution file, a PNG with no metadata, one file per supported still format, a small DNG, and a short video. Each part's "Tests" section lists further files it needs (for example an animated GIF, a transparent PNG, a wide-gamut PNG, a corrupt file, a file with a `.heic` name, a nested album root); the generator makes all of them. No personal photos are ever committed.
