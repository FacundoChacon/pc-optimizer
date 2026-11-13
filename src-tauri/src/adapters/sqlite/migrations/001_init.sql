-- Migracion 001: ajustes, escaneos y bitacora de limpieza.
-- NUNCA editar una migracion publicada: agregar 00N_*.sql.

CREATE TABLE settings (
  key        TEXT PRIMARY KEY NOT NULL CHECK (length(key) > 0),
  value_json TEXT NOT NULL CHECK (json_valid(value_json))
) STRICT;

CREATE TABLE scan (
  id           INTEGER PRIMARY KEY,
  kind         TEXT    NOT NULL CHECK (kind IN ('full','junk','duplicates','folders')),
  triggered_by TEXT    NOT NULL CHECK (triggered_by IN ('manual','scheduled')),
  status       TEXT    NOT NULL CHECK (status IN ('running','done','cancelled','error')),
  started_at   INTEGER NOT NULL CHECK (started_at > 0),
  finished_at  INTEGER CHECK (finished_at IS NULL OR finished_at >= started_at),
  CHECK ((status = 'running') = (finished_at IS NULL))
) STRICT;
CREATE INDEX idx_scan_started_at ON scan(started_at);

-- scan_id es una referencia logica historica, SIN FOREIGN KEY a proposito: la retencion de
-- escaneos no debe tocar la auditoria (los triggers de abajo abortan UPDATE y DELETE).
CREATE TABLE cleanup_log (
  id           INTEGER PRIMARY KEY,
  ts           INTEGER NOT NULL CHECK (ts > 0),
  category     TEXT    NOT NULL CHECK (length(category) > 0),
  path         TEXT    NOT NULL CHECK (length(path) > 0),
  size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
  outcome      TEXT    NOT NULL CHECK (outcome IN ('moved_to_trash','skipped_locked','skipped_protected',
                                                   'skipped_cloud','error','recycle_bin_emptied')),
  scan_id      INTEGER,
  error_detail TEXT
) STRICT;
CREATE INDEX idx_cleanup_log_ts ON cleanup_log(ts);

CREATE TRIGGER cleanup_log_no_update BEFORE UPDATE ON cleanup_log
BEGIN SELECT RAISE(ABORT, 'cleanup_log is append-only'); END;
CREATE TRIGGER cleanup_log_no_delete BEFORE DELETE ON cleanup_log
BEGIN SELECT RAISE(ABORT, 'cleanup_log is append-only'); END;

INSERT OR IGNORE INTO settings(key, value_json) VALUES
  ('ui_mode', '"simple"'), ('autostart_tray', 'false'), ('language', '"es"');
