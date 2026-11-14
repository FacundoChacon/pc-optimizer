-- Migracion 002 (C-08): hallazgos de basura y resumen por categoria.
-- NUNCA editar una migracion publicada: agregar 00N_*.sql.
--
-- `custom_rule` entra en el CHECK de categoria (reglas del usuario, v0.3) para no reconstruir
-- tablas STRICT mas adelante; el dominio de C-08 no tiene esa variante.

-- Un hallazgo es un archivo. `skip_reason` es el codigo estable del motivo de omision
-- (`ProtectedReason::code()` o `skipped_cloud`). La identidad (volumen + id de archivo) permite
-- a la limpieza detectar un archivo reemplazado: `volume_serial` es un u64 guardado como i64
-- (reinterpretacion de bits) y `file_id` un u128 en hex de 32 caracteres.
CREATE TABLE junk_finding (
  id             INTEGER PRIMARY KEY,
  scan_id        INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  category       TEXT    NOT NULL CHECK (category IN (
                   'temp_user','temp_windows','browser_cache_chrome','browser_cache_edge',
                   'browser_cache_firefox','browser_cache_brave','old_logs','wu_cache',
                   'thumbnail_cache','crash_dumps','recycle_bin','custom_rule')),
  path           TEXT    NOT NULL CHECK (length(path) > 0),
  size_bytes     INTEGER NOT NULL CHECK (size_bytes >= 0),
  modified_at    INTEGER,
  protected_flag INTEGER NOT NULL CHECK (protected_flag IN (0, 1)),
  skip_reason    TEXT,
  volume_serial  INTEGER,
  file_id        TEXT CHECK (file_id IS NULL OR length(file_id) = 32),
  CHECK ((protected_flag = 1) = (skip_reason IS NOT NULL)),
  CHECK (protected_flag = 1 OR (volume_serial IS NOT NULL AND file_id IS NOT NULL)),
  UNIQUE (scan_id, path)
) STRICT;
CREATE INDEX idx_junk_finding_scan_category ON junk_finding(scan_id, category, size_bytes DESC);

-- Una fila por escaneo y categoria (incluida la Papelera, que no tiene hallazgos): los estados
-- `blocked`/`unavailable`/`partial` y los contadores no se pueden reconstruir desde los hallazgos.
CREATE TABLE junk_category_result (
  scan_id            INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  category           TEXT    NOT NULL CHECK (category IN (
                       'temp_user','temp_windows','browser_cache_chrome','browser_cache_edge',
                       'browser_cache_firefox','browser_cache_brave','old_logs','wu_cache',
                       'thumbnail_cache','crash_dumps','recycle_bin','custom_rule')),
  scope              TEXT    NOT NULL CHECK (scope IN ('machine','per_user','mixed')),
  status             TEXT    NOT NULL CHECK (status IN ('done','partial','blocked','unavailable')),
  block_reason       TEXT    CHECK (block_reason IS NULL OR block_reason IN ('different_user','undetermined')),
  item_count         INTEGER NOT NULL CHECK (item_count >= 0),
  size_bytes         INTEGER NOT NULL CHECK (size_bytes >= 0),
  skipped_count      INTEGER NOT NULL CHECK (skipped_count >= 0),
  inaccessible_count INTEGER NOT NULL CHECK (inaccessible_count >= 0),
  CHECK ((status IN ('partial','blocked')) = (block_reason IS NOT NULL)),
  PRIMARY KEY (scan_id, category)
) STRICT;
