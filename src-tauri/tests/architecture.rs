//! Reglas de arquitectura ejecutables: el dominio es puro (sin I/O ni dependencias de SO,
//! base de datos o framework).

use std::path::{Path, PathBuf};

/// Rutas de `std` con I/O que el dominio no puede importar.
const FORBIDDEN_STD_MODULES: [&str; 3] = ["fs", "process", "net"];
/// Crates cuyo uso esta prohibido en el dominio (el acceso va por traits en `adapters/`).
const FORBIDDEN_CRATES: [&str; 4] = ["windows", "windows_sys", "rusqlite", "tauri"];

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Quita comentarios `//` y `/* */` y el espacio en blanco, salvo el que separa dos identificadores.
fn compact_code(source: &str) -> String {
    let mut out = String::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('/', Some('/')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for next in chars.by_ref() {
                    if prev == '*' && next == '/' {
                        break;
                    }
                    prev = next;
                }
            }
            (c, _) => out.push(if c.is_whitespace() { ' ' } else { c }),
        }
    }
    squeeze_whitespace(&out)
}

/// Colapsa espacios: se conserva uno solo entre dos caracteres de identificador.
fn squeeze_whitespace(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c != ' ' {
            out.push(c);
            continue;
        }
        let prev = out.chars().next_back();
        let next = chars[i + 1..].iter().find(|n| **n != ' ');
        if prev.is_some_and(is_ident_char) && next.is_some_and(|n| is_ident_char(*n)) {
            out.push(' ');
        }
    }
    out
}

/// Posiciones donde `needle` aparece como ruta completa (sin continuar un identificador).
fn contains_path(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(start, _)| {
        let before_ok = haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_ident_char(c));
        let ends_with_ident = needle.chars().next_back().is_some_and(is_ident_char);
        let after_ok = !ends_with_ident
            || haystack[start + needle.len()..]
                .chars()
                .next()
                .is_none_or(|c| !is_ident_char(c));
        before_ok && after_ok
    })
}

/// Contenido de cada grupo `std::{ ... }` (hasta su llave de cierre).
fn std_groups(compact: &str) -> Vec<&str> {
    compact
        .match_indices("std::{")
        .filter_map(|(start, marker)| {
            let body = &compact[start + marker.len()..];
            let mut depth = 1;
            for (i, c) in body.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(&body[..i]);
                        }
                    }
                    _ => {}
                }
            }
            None
        })
        .collect()
}

/// Imports/usos prohibidos en una fuente de dominio (los comentarios no cuentan).
fn forbidden_imports(source: &str) -> Vec<String> {
    let compact = compact_code(source);
    let mut found = Vec::new();

    for module in FORBIDDEN_STD_MODULES {
        let direct = contains_path(&compact, &format!("std::{module}"));
        let grouped = std_groups(&compact).iter().any(|group| {
            group
                .split(',')
                .any(|item| item == module || item.starts_with(&format!("{module}::")))
        });
        if direct || grouped {
            found.push(format!("std::{module}"));
        }
    }
    for krate in FORBIDDEN_CRATES {
        let used = contains_path(&compact, &format!("{krate}::"))
            || contains_path(&compact, &format!("use {krate};"))
            || compact.contains(&format!("use {krate}::"))
            || compact.contains(&format!("use {krate}{{"))
            || compact.contains(&format!("extern crate {krate};"));
        if used {
            found.push(krate.to_string());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_imports_should_be_empty_for_pure_source() {
        let src = "use std::collections::HashMap;\npub fn add(a: u32, b: u32) -> u32 { a + b }\n";
        assert_eq!(forbidden_imports(src), Vec::<String>::new());
    }

    #[test]
    fn forbidden_imports_should_detect_each_forbidden_import() {
        let cases = [
            ("use std::fs;", "std::fs"),
            ("use std::fs::File;", "std::fs"),
            ("let _ = std::process::Command::new(\"x\");", "std::process"),
            ("use std::net::TcpStream;", "std::net"),
            ("use std::{collections::HashMap, fs::File};", "std::fs"),
            ("use std::{\n    io,\n    process,\n};", "std::process"),
            ("use windows::Win32::Foundation::HANDLE;", "windows"),
            ("use windows_sys::Win32::Foundation::HANDLE;", "windows_sys"),
            ("use rusqlite::Connection;", "rusqlite"),
            ("use tauri::State;", "tauri"),
            ("fn f(_: tauri::AppHandle) {}", "tauri"),
        ];
        for (source, expected) in cases {
            let found = forbidden_imports(source);
            assert!(
                found.iter().any(|f| f == expected),
                "{source:?} -> {found:?}"
            );
        }
    }

    #[test]
    fn forbidden_imports_should_ignore_mentions_in_comments() {
        let src = "// use std::fs;\n/* use tauri::State; */\n/// usa rusqlite::Connection en el adapter\nfn pure() {}\n";
        assert_eq!(forbidden_imports(src), Vec::<String>::new());
    }

    #[test]
    fn forbidden_imports_should_not_flag_lookalike_identifiers() {
        let src = "use std::fsx::Thing;\nuse my_tauri::State;\nlet windows = 3;\nlet rusqlite_like = 1;\n";
        assert_eq!(forbidden_imports(src), Vec::<String>::new());
    }
}

fn rust_files_under(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries =
            std::fs::read_dir(&current).map_err(|e| format!("{}: {e}", current.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

#[test]
fn domain_layer_should_not_import_io_os_database_or_framework() {
    let domain_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("domain");
    let files = rust_files_under(&domain_dir).unwrap();
    assert!(
        !files.is_empty(),
        "no se encontraron archivos en {}",
        domain_dir.display()
    );
    let mut violations = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap();
        for import in forbidden_imports(&source) {
            violations.push(format!("{}: import prohibido {import}", file.display()));
        }
    }
    assert!(violations.is_empty(), "dominio impuro: {violations:#?}");
}

// ---------------------------------------------------------------------------------------------
// C-09: el codigo de produccion no tiene via de borrado permanente (design D-12, RN-LIM-01).
// ---------------------------------------------------------------------------------------------

/// Llamadas de borrado que el codigo de produccion no puede contener: todo borrado pasa por
/// `Trash::move_to_trash`. La unica excepcion declarada (vaciar la Papelera, RN-LIM-06, C-10) esta
/// en [`DECLARED_PERMANENT_EXCEPTIONS`].
const FORBIDDEN_DELETION_TOKENS: [&str; 8] = [
    "remove_file",
    "remove_dir",
    "DeleteFileW",
    "SHEmptyRecycleBin",
    "SHERB_",
    "SHFileOperation",
    "FOF_",
    "delete_all",
];

/// El archivo de la unica excepcion: el vaciado de la Papelera (C-10, RN-LIM-06).
const EMPTY_ADAPTER: &str = "src/adapters/windows/recycle_bin_empty.rs";

/// Excepciones declaradas al borrado permanente: `(token, archivo relativo al crate, maximo de
/// ocurrencias en produccion)`. La llamada que vacia aparece exactamente una vez; las banderas
/// `SHERB_*` son tres (sin confirmacion, sin progreso, sin sonido). Ampliar esta lista exige un
/// change aprobado por el dueno.
const DECLARED_PERMANENT_EXCEPTIONS: [(&str, &str, usize); 2] = [
    ("SHEmptyRecycleBin", EMPTY_ADAPTER, 1),
    ("SHERB_", EMPTY_ADAPTER, 3),
];

/// Archivos que son solo de test aunque no esten bajo `#[cfg(test)]` en su propio contenido.
fn is_test_only_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with("_tests.rs")
        || name == "fakes.rs"
        || name == "test_util.rs"
        || name == "test_support.rs"
        || name.ends_with("_contract.rs")
}

/// Quita del texto (ya sin comentarios) cada item marcado con `#[cfg(test)]`: hasta su `;`
/// (p. ej. `mod x;`) o hasta el cierre de su bloque `{ ... }`, lo que ocurra primero.
fn strip_cfg_test_items(code: &str) -> String {
    const MARKER: &str = "#[cfg(test)]";
    let mut out = String::new();
    let mut rest = code;
    while let Some(start) = rest.find(MARKER) {
        out.push_str(&rest[..start]);
        let item = &rest[start + MARKER.len()..];
        let mut depth = 0usize;
        let mut end = item.len();
        for (i, c) in item.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                ';' if depth == 0 => {
                    end = i + 1;
                    break;
                }
                _ => {}
            }
        }
        rest = &item[end..];
    }
    out.push_str(rest);
    out
}

/// Tokens de borrado prohibidos presentes en una fuente de produccion (comentarios y bloques
/// `#[cfg(test)]` no cuentan). `rel_path` es la ruta del archivo relativa al crate (con `/`): una
/// excepcion declarada se tolera solo en su archivo y hasta su maximo de ocurrencias.
fn forbidden_deletion_calls(source: &str, rel_path: &str) -> Vec<&'static str> {
    let production = strip_cfg_test_items(&compact_code(source));
    FORBIDDEN_DELETION_TOKENS
        .into_iter()
        .filter(|token| {
            let allowed = DECLARED_PERMANENT_EXCEPTIONS
                .iter()
                .find(|(t, file, _)| t == token && *file == rel_path)
                .map_or(0, |(_, _, max)| *max);
            production.matches(token).count() > allowed
        })
        .collect()
}

/// Identificadores del contrato de vaciado que solo puede usar el flujo de vaciado (D-12).
const EMPTY_CONTRACT_IDENTIFIERS: [&str; 3] = [
    "RecycleBinEmptier",
    "WindowsRecycleBinEmptier",
    "run_empty_recycle_bin",
];

/// Lista CERRADA de archivos (o carpetas, con `/` final) de produccion que pueden nombrar el
/// contrato de vaciado. Un scheduler, las mejoras o la bandeja que lo usen rompen el test.
const EMPTY_CONTRACT_ALLOWED: [&str; 7] = [
    "src/domain/ports/recycle_bin_empty.rs",
    "src/domain/ports/mod.rs",
    "src/domain/recycle_bin/",
    "src/adapters/windows/recycle_bin_empty.rs",
    "src/adapters/windows/mod.rs",
    "src/commands/recycle_bin.rs",
    "src/lib.rs",
];

/// Identificadores del contrato de vaciado usados en una fuente de produccion fuera de la lista
/// cerrada (comentarios y `#[cfg(test)]` no cuentan).
fn empty_contract_uses(source: &str, rel_path: &str) -> Vec<&'static str> {
    let allowed = EMPTY_CONTRACT_ALLOWED
        .iter()
        .any(|entry| rel_path == *entry || (entry.ends_with('/') && rel_path.starts_with(entry)));
    if allowed {
        return Vec::new();
    }
    let production = strip_cfg_test_items(&compact_code(source));
    EMPTY_CONTRACT_IDENTIFIERS
        .into_iter()
        .filter(|identifier| contains_path(&production, identifier))
        .collect()
}

#[cfg(test)]
mod deletion_scanner_tests {
    use super::*;

    #[test]
    fn forbidden_deletion_calls_should_detect_each_forbidden_token_in_production_code() {
        let cases = [
            "fn f() { std::fs::remove_file(p).ok(); }",
            "fn f() { let _ = fs::remove_dir_all(p); }",
            "fn f() { DeleteFileW(p); }",
            "fn f() { SHEmptyRecycleBinW(h, r, 0); }",
            "fn f() { SHFileOperationW(&mut op); }",
            "const F: u32 = FOF_ALLOWUNDO;",
            "fn f() { trash::delete_all(items) }",
        ];
        for source in cases {
            assert!(
                !forbidden_deletion_calls(source, "src/other.rs").is_empty(),
                "no detecto: {source}"
            );
        }
    }

    #[test]
    fn forbidden_deletion_calls_should_ignore_comments_and_cfg_test_items() {
        let src = "// std::fs::remove_file(p)\n/* delete_all */\nfn ok() { trash::delete(p) }\n\
                   #[cfg(test)]\nmod tests { fn t() { std::fs::remove_file(p).unwrap(); } }\n\
                   #[cfg(test)]\n#[path = \"x_tests.rs\"]\nmod more;\n\
                   fn after() {}\n";
        assert_eq!(
            forbidden_deletion_calls(src, "src/other.rs"),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn forbidden_deletion_calls_should_still_scan_code_after_a_cfg_test_block() {
        let src = "#[cfg(test)]\nmod tests { fn t() {} }\nfn bad() { std::fs::remove_file(p); }\n";
        assert_eq!(
            forbidden_deletion_calls(src, "src/other.rs"),
            vec!["remove_file"]
        );
    }

    const CALL: &str = "fn empty() { windows::Win32::UI::Shell::SHEmptyRecycleBinW(None, r, SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND); }";

    #[test]
    fn forbidden_deletion_calls_should_tolerate_the_declared_exception_in_its_own_file() {
        assert_eq!(
            forbidden_deletion_calls(CALL, EMPTY_ADAPTER),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn forbidden_deletion_calls_should_detect_the_exception_in_any_other_file() {
        for other in [
            "src/commands/scan.rs",
            "src/adapters/windows/trash.rs",
            "src/domain/recycle_bin/execute.rs",
        ] {
            assert_eq!(
                forbidden_deletion_calls(CALL, other),
                vec!["SHEmptyRecycleBin", "SHERB_"],
                "{other}"
            );
        }
    }

    #[test]
    fn forbidden_deletion_calls_should_detect_a_second_call_in_the_declared_file() {
        let twice = format!("{CALL}\n{CALL}");
        assert_eq!(
            forbidden_deletion_calls(&twice, EMPTY_ADAPTER),
            vec!["SHEmptyRecycleBin", "SHERB_"]
        );
    }

    #[test]
    fn forbidden_deletion_calls_should_ignore_the_exception_in_comments_and_test_items() {
        let src = "// SHEmptyRecycleBinW(None, r, SHERB_NOSOUND)\n\
                   #[cfg(test)]\nmod tests { fn t() { SHEmptyRecycleBinW(None, r, SHERB_NOSOUND); } }\n";
        assert_eq!(
            forbidden_deletion_calls(src, "src/other.rs"),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn empty_contract_uses_should_tolerate_the_closed_list_and_detect_everything_else() {
        let src = "fn f(e: &impl RecycleBinEmptier) { run_empty_recycle_bin(a); WindowsRecycleBinEmptier::new(); }";
        for allowed in [
            "src/commands/recycle_bin.rs",
            "src/domain/recycle_bin/execute.rs",
            "src/adapters/windows/recycle_bin_empty.rs",
            "src/lib.rs",
        ] {
            assert_eq!(
                empty_contract_uses(src, allowed),
                Vec::<&str>::new(),
                "{allowed}"
            );
        }
        for other in [
            "src/commands/scan.rs",
            "src/scheduler.rs",
            "src/commands/cleanup.rs",
            "src/domain/improvements/apply.rs",
        ] {
            assert_eq!(
                empty_contract_uses(src, other),
                vec![
                    "RecycleBinEmptier",
                    "WindowsRecycleBinEmptier",
                    "run_empty_recycle_bin"
                ],
                "{other}"
            );
        }
    }

    #[test]
    fn empty_contract_uses_should_not_flag_lookalikes_comments_or_test_items() {
        let src = "// RecycleBinEmptier\nfn f() { let my_run_empty_recycle_bin_x = 1; }\n\
                   #[cfg(test)]\nmod tests { fn t() { run_empty_recycle_bin(a); } }\n";
        assert_eq!(
            empty_contract_uses(src, "src/commands/scan.rs"),
            Vec::<&str>::new()
        );
    }
}

#[test]
fn production_code_should_not_contain_permanent_deletion_calls() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = rust_files_under(&src_dir).unwrap();
    assert!(
        !files.is_empty(),
        "no se encontraron fuentes en {}",
        src_dir.display()
    );
    let mut violations = Vec::new();
    for file in files.iter().filter(|f| !is_test_only_file(f)) {
        let source = std::fs::read_to_string(file).unwrap();
        for token in forbidden_deletion_calls(&source, &relative_to_crate(file)) {
            violations.push(format!("{}: {token}", file.display()));
        }
    }
    assert!(
        violations.is_empty(),
        "borrado permanente en produccion: {violations:#?}"
    );
}

/// Ruta de `file` relativa al crate y con `/` (`src/adapters/...`).
fn relative_to_crate(file: &Path) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn permanent_empty_contract_should_only_be_used_by_the_empty_flow() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = rust_files_under(&src_dir).unwrap();
    let mut violations = Vec::new();
    for file in files.iter().filter(|f| !is_test_only_file(f)) {
        let source = std::fs::read_to_string(file).unwrap();
        for identifier in empty_contract_uses(&source, &relative_to_crate(file)) {
            violations.push(format!("{}: {identifier}", file.display()));
        }
    }
    assert!(
        violations.is_empty(),
        "el contrato de vaciado solo lo usa el flujo de vaciado: {violations:#?}"
    );
}

#[test]
fn the_declared_exception_file_should_contain_exactly_the_single_call() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join(EMPTY_ADAPTER);
    let source = std::fs::read_to_string(file).unwrap();
    let production = strip_cfg_test_items(&compact_code(&source));
    assert_eq!(production.matches("SHEmptyRecycleBin").count(), 1);
}

/// Archivo donde, y solo donde, se asigna la marca `kept` de una copia (D-10): la resolucion de
/// un grupo confirmado.
const KEPT_WRITER: &str = "src/adapters/sqlite/duplicates.rs";

/// Cantidad de asignaciones `kept = ...` (no comparaciones `kept == ...`) en una fuente de
/// produccion; comentarios y bloques `#[cfg(test)]` no cuentan.
fn kept_assignments(source: &str) -> usize {
    let production = strip_cfg_test_items(&compact_code(source));
    production
        .match_indices("kept=")
        .filter(|(start, _)| !production[start + "kept=".len()..].starts_with('='))
        .count()
}

#[test]
fn kept_should_only_be_assigned_by_the_group_resolution() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = rust_files_under(&src_dir).unwrap();
    let mut violations = Vec::new();
    let mut writer_assignments = 0;
    for file in files.iter().filter(|f| !is_test_only_file(f)) {
        let source = std::fs::read_to_string(file).unwrap();
        let rel = relative_to_crate(file);
        let count = kept_assignments(&source);
        if rel == KEPT_WRITER {
            writer_assignments = count;
        } else if count > 0 {
            violations.push(format!("{rel}: {count} asignacion(es) de `kept`"));
        }
    }
    assert!(
        violations.is_empty(),
        "`kept` solo se escribe al resolver un grupo: {violations:#?}"
    );
    assert_eq!(
        writer_assignments, 1,
        "la resolucion debe contener exactamente una sentencia que asigne `kept`"
    );
}

#[test]
fn kept_assignments_should_count_sql_assignments_and_ignore_comparisons_comments_and_tests() {
    let sql = r#"const A: &str = "UPDATE duplicate_file SET kept = 1 WHERE id = ?1";"#;
    assert_eq!(kept_assignments(sql), 1);
    let two = r#"const A: &str = "UPDATE f SET kept=1"; const B: &str = "UPDATE f SET kept = 0";"#;
    assert_eq!(kept_assignments(two), 2);
    assert_eq!(kept_assignments("let same = file.kept == 1;"), 0);
    assert_eq!(kept_assignments("// UPDATE f SET kept = 1"), 0);
    assert_eq!(
        kept_assignments(
            "#[cfg(test)]
mod tests { const A: &str = \"SET kept = 1\"; }"
        ),
        0
    );
    assert_eq!(kept_assignments("struct F { kept: bool }"), 0);
}
