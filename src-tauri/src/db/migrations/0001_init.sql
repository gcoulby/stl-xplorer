CREATE TABLE libraries (
  id              INTEGER PRIMARY KEY,
  name            TEXT    NOT NULL,
  path            TEXT    NOT NULL UNIQUE,
  enabled         INTEGER NOT NULL DEFAULT 1,
  is_vault        INTEGER NOT NULL DEFAULT 0,
  sort_order      INTEGER NOT NULL DEFAULT 0,
  last_scanned_at INTEGER
);

-- A real folder or an archive. kind: folder, zip, 7z, rar.
-- Multi-part archives: path is the entry point, part_paths is a JSON array of every part present.
-- Nested zips (a zip inside a zip): path is "<outer>::<entry>", entry_path is the entry inside the parent.
CREATE TABLE containers (
  id            INTEGER PRIMARY KEY,
  library_id    INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
  parent_id     INTEGER REFERENCES containers(id) ON DELETE CASCADE,
  kind          TEXT    NOT NULL CHECK (kind IN ('folder', 'zip', '7z', 'rar')),
  path          TEXT    NOT NULL,
  part_paths    TEXT,
  entry_path    TEXT,
  name          TEXT    NOT NULL,
  size          INTEGER NOT NULL DEFAULT 0,
  mtime         INTEGER NOT NULL DEFAULT 0,
  signature     TEXT,
  is_pack       INTEGER NOT NULL DEFAULT 0,
  solid         INTEGER NOT NULL DEFAULT 0,
  encrypted     INTEGER NOT NULL DEFAULT 0,
  status        TEXT    NOT NULL DEFAULT 'ok'
                CHECK (status IN ('ok', 'missing_parts', 'corrupt', 'unsupported')),
  status_detail TEXT,
  missing       INTEGER NOT NULL DEFAULT 0,
  vaulted       INTEGER NOT NULL DEFAULT 0,
  original_path TEXT,
  rot_x REAL, rot_y REAL, rot_z REAL,
  model_count   INTEGER NOT NULL DEFAULT 0,
  issue_count   INTEGER NOT NULL DEFAULT 0,
  UNIQUE (library_id, path)
);
CREATE INDEX idx_containers_parent ON containers(parent_id);

CREATE TABLE models (
  id            INTEGER PRIMARY KEY,
  container_id  INTEGER NOT NULL REFERENCES containers(id) ON DELETE CASCADE,
  library_id    INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
  rel_path      TEXT    NOT NULL,
  name          TEXT    NOT NULL,
  ext           TEXT    NOT NULL,
  size          INTEGER NOT NULL DEFAULT 0,
  mtime         INTEGER NOT NULL DEFAULT 0,
  encrypted     INTEGER NOT NULL DEFAULT 0,
  rot_x REAL, rot_y REAL, rot_z REAL,
  missing       INTEGER NOT NULL DEFAULT 0,
  vaulted       INTEGER NOT NULL DEFAULT 0,
  original_path TEXT,
  UNIQUE (container_id, rel_path)
);
CREATE INDEX idx_models_library ON models(library_id);

CREATE TABLE tags (
  id     INTEGER PRIMARY KEY,
  name   TEXT NOT NULL UNIQUE COLLATE NOCASE,
  colour TEXT NOT NULL DEFAULT '#64748b'
);

CREATE TABLE model_tags (
  model_id INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
  tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (model_id, tag_id)
);

CREATE TABLE container_tags (
  container_id INTEGER NOT NULL REFERENCES containers(id) ON DELETE CASCADE,
  tag_id       INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (container_id, tag_id)
);

CREATE TABLE collections (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL
);

CREATE TABLE collection_items (
  collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
  container_id  INTEGER NOT NULL REFERENCES containers(id) ON DELETE CASCADE,
  PRIMARY KEY (collection_id, container_id)
);

CREATE TABLE slicers (
  id           INTEGER PRIMARY KEY,
  name         TEXT    NOT NULL,
  exe_path     TEXT    NOT NULL,
  args         TEXT    NOT NULL DEFAULT '[]',
  is_default   INTEGER NOT NULL DEFAULT 0,
  one_per_file INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- rowid = models.id. Kept in sync from Rust (db::queries::refresh_fts); deletes via trigger.
CREATE VIRTUAL TABLE models_fts USING fts5(
  name, container_name, path, tags,
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);

CREATE TRIGGER models_fts_delete AFTER DELETE ON models BEGIN
  DELETE FROM models_fts WHERE rowid = old.id;
END;
