-- Schema version 1: the album database as docs/02-data.md ("Schema")
-- describes it. Run once, in a transaction, on a database whose
-- user_version is 0. Never edit a migration that has shipped; add the next
-- numbered script instead.

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
  format        TEXT NOT NULL,          -- registry id: 'jpeg', 'png', 'cr3', 'mp4', ...
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
  vector   BLOB NOT NULL,               -- little-endian float32, unit length, dim x 4 bytes
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
