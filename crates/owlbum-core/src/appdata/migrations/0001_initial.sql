-- app.sqlite schema version 1, as docs/02-data.md ("app.sqlite")
-- describes it. Run once, in a transaction, on a file whose user_version is
-- 0. Never edit a migration that has shipped; add the next numbered script.

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
