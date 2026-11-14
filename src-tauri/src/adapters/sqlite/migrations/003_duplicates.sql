-- Migracion 003 (C-11): busqueda de duplicados.
-- NUNCA editar una migracion publicada: agregar 00N_*.sql.
--
-- `cleanup_log`, `scan` y `junk_*` no se tocan: el log admite la categoria `duplicates` sin
-- migrar (`category` es texto libre no vacio) y `scan.kind` ya incluye `duplicates`.

-- Un resultado por escaneo de duplicados terminado: tamano minimo usado y contadores. Sin esta
-- tabla el resumen no sobreviviria a un reinicio.
CREATE TABLE duplicate_scan_result (
  scan_id           INTEGER PRIMARY KEY REFERENCES scan(id) ON DELETE CASCADE,
  min_size_bytes    INTEGER NOT NULL CHECK (min_size_bytes IN (102400, 1048576, 10485760, 104857600)),
  files_seen        INTEGER NOT NULL CHECK (files_seen >= 0),
  below_min_size    INTEGER NOT NULL CHECK (below_min_size >= 0),
  protected_count   INTEGER NOT NULL CHECK (protected_count >= 0),
  cloud_count       INTEGER NOT NULL CHECK (cloud_count >= 0),
  hard_linked_count INTEGER NOT NULL CHECK (hard_linked_count >= 0),
  changed_count     INTEGER NOT NULL CHECK (changed_count >= 0),
  unreadable_count  INTEGER NOT NULL CHECK (unreadable_count >= 0),
  inaccessible_dirs INTEGER NOT NULL CHECK (inaccessible_dirs >= 0),
  excluded_dirs     INTEGER NOT NULL CHECK (excluded_dirs >= 0),
  hashed_bytes      INTEGER NOT NULL CHECK (hashed_bytes >= 0)
) STRICT;

-- Estado de cada carpeta de contenido en el escaneo. `reason` es el codigo estable del motivo y
-- solo existe si la carpeta no estuvo disponible.
CREATE TABLE duplicate_scan_root (
  scan_id    INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  root       TEXT    NOT NULL CHECK (root IN ('desktop','documents','downloads','music','pictures','videos')),
  status     TEXT    NOT NULL CHECK (status IN ('done','not_selected','nested','unavailable')),
  reason     TEXT,
  files_seen INTEGER NOT NULL CHECK (files_seen >= 0),
  CHECK ((status = 'unavailable') = (reason IS NOT NULL)),
  PRIMARY KEY (scan_id, root)
) STRICT;

-- `file_count` evita un COUNT por fila al ordenar por espacio recuperable
-- (size_bytes * (file_count - 1) DESC, id ASC); el store valida que coincida con las copias.
-- La unicidad (scan_id, size_bytes, blake3) cubre el indice (scan_id, size_bytes) de la KB 04.
CREATE TABLE duplicate_group (
  id          INTEGER PRIMARY KEY,
  scan_id     INTEGER NOT NULL REFERENCES scan(id) ON DELETE CASCADE,
  size_bytes  INTEGER NOT NULL CHECK (size_bytes > 0),
  blake3      TEXT    NOT NULL CHECK (length(blake3) = 64 AND blake3 NOT GLOB '*[^0-9a-f]*'),
  file_count  INTEGER NOT NULL CHECK (file_count >= 2),
  resolved_at INTEGER CHECK (resolved_at IS NULL OR resolved_at > 0),
  UNIQUE (scan_id, size_bytes, blake3)
) STRICT;
CREATE INDEX idx_duplicate_group_blake3 ON duplicate_group(blake3);

-- Una copia. `volume_serial` es un u64 guardado como i64 (reinterpretacion de bits) y `file_id`
-- un u128 en hex de 32 caracteres, igual que `junk_finding`. `kept` solo lo escribe la
-- resolucion de un grupo confirmado (por defecto 0: nunca hay una copia elegida sin decision).
CREATE TABLE duplicate_file (
  id            INTEGER PRIMARY KEY,
  group_id      INTEGER NOT NULL REFERENCES duplicate_group(id) ON DELETE CASCADE,
  root          TEXT    NOT NULL CHECK (root IN ('desktop','documents','downloads','music','pictures','videos')),
  path          TEXT    NOT NULL CHECK (length(path) > 0),
  mtime         INTEGER,
  volume_serial INTEGER NOT NULL,
  file_id       TEXT    NOT NULL CHECK (length(file_id) = 32),
  kept          INTEGER NOT NULL DEFAULT 0 CHECK (kept IN (0, 1)),
  UNIQUE (group_id, path),
  UNIQUE (group_id, volume_serial, file_id)
) STRICT;
CREATE INDEX idx_duplicate_file_root ON duplicate_file(root, group_id);
