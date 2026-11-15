-- Migracion 004 (C-07): analisis de espacio en disco.
-- NUNCA editar una migracion publicada: agregar 00N_*.sql.
--
-- `scan`, `cleanup_log`, `junk_*` y `duplicate_*` no se tocan: `scan.kind` ya admite `folders`.

-- Un resultado por analisis terminado: raiz, espacio del volumen y contadores. Sin esta tabla el
-- resumen no sobreviviria a un reinicio.
CREATE TABLE folder_scan_result (
  scan_id            INTEGER PRIMARY KEY REFERENCES scan(id) ON DELETE CASCADE,
  root_kind          TEXT    NOT NULL CHECK (root_kind IN ('system_drive','picked')),
  root_path          TEXT    NOT NULL CHECK (length(root_path) > 0),
  volume_total_bytes INTEGER CHECK (volume_total_bytes IS NULL OR volume_total_bytes > 0),
  volume_free_bytes  INTEGER CHECK (volume_free_bytes IS NULL OR volume_free_bytes >= 0),
  total_bytes        INTEGER NOT NULL CHECK (total_bytes >= 0),
  file_count         INTEGER NOT NULL CHECK (file_count >= 0),
  dir_count          INTEGER NOT NULL CHECK (dir_count >= 0),
  inaccessible_dirs  INTEGER NOT NULL CHECK (inaccessible_dirs >= 0),
  reparse_skipped    INTEGER NOT NULL CHECK (reparse_skipped >= 0),
  cloud_files        INTEGER NOT NULL CHECK (cloud_files >= 0),
  opaque_dirs        INTEGER NOT NULL CHECK (opaque_dirs >= 0),
  folders_stored     INTEGER NOT NULL CHECK (folders_stored >= 0),
  folders_truncated  INTEGER NOT NULL CHECK (folders_truncated IN (0, 1)),
  CHECK ((volume_total_bytes IS NULL) = (volume_free_bytes IS NULL))
) STRICT;

-- Carpetas guardadas (hasta 20 000, cerradas por ancestros). La raiz es la unica sin padre.
CREATE TABLE folder_snapshot (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  scan_id      INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  parent_id    INTEGER REFERENCES folder_snapshot(id) ON DELETE CASCADE,
  path         TEXT    NOT NULL CHECK (length(path) > 0),
  depth        INTEGER NOT NULL CHECK (depth >= 0),
  size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
  direct_bytes INTEGER NOT NULL CHECK (direct_bytes >= 0 AND direct_bytes <= size_bytes),
  file_count   INTEGER NOT NULL CHECK (file_count >= 0),
  dir_count    INTEGER NOT NULL CHECK (dir_count >= 0),
  access       TEXT    NOT NULL CHECK (access IN ('complete','incomplete','inaccessible')),
  opaque_reason TEXT   CHECK (opaque_reason IS NULL OR opaque_reason IN
                         ('other_user_profile','recycle_bin','system_volume_information')),
  system       INTEGER NOT NULL CHECK (system IN (0, 1)),
  junk_hint    INTEGER NOT NULL CHECK (junk_hint IN (0, 1)),
  CHECK ((depth = 0) = (parent_id IS NULL)),
  UNIQUE (scan_id, path)
) STRICT;
-- Tendencias por carpeta (KB 04, C-25).
CREATE INDEX idx_folder_snapshot_path ON folder_snapshot(path, scan_id);
-- Hijos de una carpeta por tamano.
CREATE INDEX idx_folder_snapshot_children ON folder_snapshot(scan_id, parent_id, size_bytes DESC);

-- Los 500 archivos mas grandes, fuera de carpetas opacas.
CREATE TABLE large_file (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  scan_id     INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  folder_id   INTEGER NOT NULL REFERENCES folder_snapshot(id) ON DELETE CASCADE,
  path        TEXT    NOT NULL CHECK (length(path) > 0),
  size_bytes  INTEGER NOT NULL CHECK (size_bytes > 0),
  modified_at INTEGER CHECK (modified_at IS NULL OR modified_at > 0),
  UNIQUE (scan_id, path)
) STRICT;
CREATE INDEX idx_large_file_size ON large_file(scan_id, size_bytes DESC);
CREATE INDEX idx_large_file_modified ON large_file(scan_id, modified_at);

-- Muestra (hasta 50) de carpetas que no se pudieron listar. Nunca de carpetas opacas.
CREATE TABLE folder_scan_issue (
  scan_id INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  path    TEXT    NOT NULL CHECK (length(path) > 0),
  reason  TEXT    NOT NULL CHECK (reason IN ('access_denied','not_found','io_error')),
  PRIMARY KEY (scan_id, path)
) STRICT;
