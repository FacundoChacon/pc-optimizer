//! Tests de integracion de dominio (tarea 11.1): contexto `ctx_c_ana()` + `FakeFileSystem`.
//!
//! Un test por escenario de `specs/path-protection/spec.md`, nombrado `scenario_<escenario>`.
//! Los comentarios `///` citan el requisito y el escenario del spec.

use std::collections::HashSet;
use std::path::Path;

use super::context::{ContextError, ProtectionContext};
use super::decision::{
    AllowRule, AnchorKind, Browser, CheckedPath, ProtectedReason, ProtectionDecision,
    ResolveFailure,
};
use super::path::PathRejection;
use super::service::PathProtection;
use super::test_support::{ctx_c_ana, ctx_c_no_user, ctx_c_user, ctx_d_ana, raw_c_ana};
use crate::domain::model::CleanupOutcome;
use crate::domain::ports::fakes::FakeFileSystem;
use crate::domain::ports::{EntryKind, PortError};

use ProtectedReason as R;

const TEMP: &str = r"C:\Users\ana\AppData\Local\Temp";

/// Arma un fake con cada ruta de `files`, de `dirs` y todos sus ancestros (sin la raiz).
fn tree(files: &[&str], dirs: &[&str]) -> FakeFileSystem {
    let mut fs = FakeFileSystem::new().canonical_as_is();
    let mut seen: HashSet<String> = HashSet::new();
    let leaves = files
        .iter()
        .map(|path| (*path, false))
        .chain(dirs.iter().map(|path| (*path, true)));
    for (leaf, is_dir) in leaves {
        for ancestor in Path::new(leaf).ancestors().skip(1) {
            if ancestor.parent().is_none() {
                continue;
            }
            let text = ancestor.display().to_string();
            if seen.insert(text.clone()) {
                fs = fs.with_dir(&text);
            }
        }
        fs = if is_dir {
            fs.with_dir(leaf)
        } else {
            fs.with_file(leaf, 7, None)
        };
    }
    fs
}

fn decide_with(ctx: ProtectionContext, fs: &FakeFileSystem, raw: &str) -> ProtectionDecision {
    PathProtection::new(fs, Ok(ctx)).check(Path::new(raw))
}

fn decide(fs: &FakeFileSystem, raw: &str) -> ProtectionDecision {
    decide_with(ctx_c_ana(), fs, raw)
}

fn protected(reason: ProtectedReason) -> ProtectionDecision {
    ProtectionDecision::Protected(reason)
}

fn allowed(decision: ProtectionDecision) -> CheckedPath {
    match decision {
        ProtectionDecision::Allowed(checked) => checked,
        other => panic!("se esperaba Allowed, dio {other:?}"),
    }
}

fn reason_code(decision: &ProtectionDecision) -> &'static str {
    match decision {
        ProtectionDecision::Protected(reason) => reason.code(),
        other => panic!("se esperaba Protected, dio {other:?}"),
    }
}

fn same_identity() -> crate::domain::ports::FileIdentity {
    crate::domain::ports::FileIdentity {
        volume_serial: 1,
        file_id: 4242,
    }
}

/// Maquina estandar de los escenarios.
fn machine() -> FakeFileSystem {
    tree(
        &[
            r"C:\Users\ana\AppData\Local\Temp\a.tmp",
            r"C:\Windows\System32\kernel32.dll",
            r"C:\Windows\System32\x.dll",
            r"C:\Windows\Temp\x.tmp",
            r"C:\Windows\Temp\a.tmp",
            r"C:\Windows\SoftwareDistribution\Download\abc\x.cab",
            r"C:\Windows\SoftwareDistribution\DataStore\x.edb",
            r"C:\Windows\Logs\CBS\CBS.log",
            r"C:\Users\ana\Downloads\setup-viejo.exe",
            r"C:\Program Files (x86)\App\cache\a.tmp",
            r"C:\Users\ana\AppData\Local\pc-optimizer\pc-optimizer.db",
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\Network\Cookies",
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Profile 2\Cache\Cache_Data\f_000001",
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Cache\x",
            r"C:\Users\ana\AppData\Local\Mozilla\Firefox\Profiles\ab12.default\cache2\entries\X",
            r"C:\Users\ana\AppData\Roaming\Mozilla\Firefox\Profiles\ab12.default\logins.json",
            r"C:\Users\bob\AppData\Local\Temp\a.tmp",
            r"C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Cache\x",
            r"C:\Users\bob\Downloads\a.txt",
            r"C:\Users\Public\Documents\a.txt",
            r"C:\pagefile.sys",
            r"C:\System Volume Information\x",
            r"C:\$Recycle.Bin\S-1-5-21-1\$R1.tmp",
            r"D:\Temp\a.tmp",
        ],
        &[
            r"C:\Users\ana\AppData\Roaming",
            r"C:\ProgramData",
            r"C:\Program Files",
        ],
    )
}

// --- Requisito: Decision con motivo para cada ruta ---

/// Escenario: Archivo temporal elegible.
#[test]
fn scenario_archivo_temporal_elegible() {
    let fs = machine();
    let checked = allowed(decide(&fs, r"C:\Users\ana\AppData\Local\Temp\a.tmp"));
    assert_eq!(
        checked.path.to_string(),
        r"C:\Users\ana\AppData\Local\Temp\a.tmp"
    );
    assert_eq!(checked.kind, EntryKind::File);
    assert_eq!(checked.size_bytes, 7);
    assert_eq!(checked.rule, AllowRule::Default);
    // La identidad certificada es la que informa el sistema de archivos.
    let meta = crate::domain::ports::FileSystem::metadata(
        &fs,
        Path::new(r"C:\Users\ana\AppData\Local\Temp\a.tmp"),
    )
    .unwrap();
    assert_eq!(Some(checked.identity), meta.identity);
}

/// Escenario: Ruta protegida con motivo.
#[test]
fn scenario_ruta_protegida_con_motivo() {
    let decision = decide(&machine(), r"C:\Windows\System32\kernel32.dll");
    assert_eq!(decision, protected(R::WindowsDir));
    assert_eq!(reason_code(&decision), "windows_dir");
}

/// Escenario: Correspondencia con los resultados del log.
#[test]
fn scenario_correspondencia_con_los_resultados_del_log() {
    let fs = machine().with_cloud_placeholder(r"C:\Users\ana\Downloads\setup-viejo.exe", 5);
    let protected_decision = decide(&fs, r"C:\Windows\System32\kernel32.dll");
    assert_eq!(
        protected_decision.skip_outcome(),
        Some(CleanupOutcome::SkippedProtected)
    );
    let cloud = decide(&fs, r"C:\Users\ana\Downloads\setup-viejo.exe");
    assert!(matches!(cloud, ProtectionDecision::SkippedCloud { .. }));
    assert_eq!(cloud.skip_outcome(), Some(CleanupOutcome::SkippedCloud));
    let eligible = decide(&fs, r"C:\Users\ana\AppData\Local\Temp\a.tmp");
    assert!(eligible.is_allowed());
    assert_eq!(eligible.skip_outcome(), None);
}

// --- Requisito: Fail-closed ante cualquier duda ---

/// Escenario: Metadatos ilegibles.
#[test]
fn scenario_metadatos_ilegibles() {
    let denied = || PortError::PermissionDenied("denegado".into());
    let ancestor = machine().fail_on(r"C:\Users\ana\AppData", denied());
    assert_eq!(
        decide(&ancestor, r"C:\Users\ana\AppData\Local\Temp\a.tmp"),
        protected(R::Unresolvable(ResolveFailure::PermissionDenied))
    );
    let target = machine().fail_on(r"C:\Users\ana\AppData\Local\Temp\a.tmp", denied());
    assert_eq!(
        decide(&target, r"C:\Users\ana\AppData\Local\Temp\a.tmp"),
        protected(R::Unresolvable(ResolveFailure::PermissionDenied))
    );
    // El tipo de falla se conserva en el motivo.
    let in_use = machine().fail_on(
        r"C:\Users\ana\AppData\Local\Temp\a.tmp",
        PortError::InUse("en uso".into()),
    );
    assert_eq!(
        decide(&in_use, r"C:\Users\ana\AppData\Local\Temp\a.tmp"),
        protected(R::Unresolvable(ResolveFailure::InUse))
    );
}

/// Escenario: Ruta inexistente.
#[test]
fn scenario_ruta_inexistente() {
    let decision = decide(&machine(), r"C:\Users\ana\AppData\Local\Temp\no-existe.tmp");
    assert_eq!(
        decision,
        protected(R::Unresolvable(ResolveFailure::NotFound))
    );
    assert_eq!(reason_code(&decision), "unresolvable_not_found");
}

/// Escenario: Identidad de archivo no disponible.
#[test]
fn scenario_identidad_de_archivo_no_disponible() {
    let fs = machine().without_identity(r"C:\Users\ana\AppData\Local\Temp\a.tmp");
    assert_eq!(
        decide(&fs, r"C:\Users\ana\AppData\Local\Temp\a.tmp"),
        protected(R::Unresolvable(ResolveFailure::MissingIdentity))
    );
}

// --- Requisito: Normalizacion de la forma de la ruta ---

/// Escenario: Segmentos relativos que escapan hacia Windows.
#[test]
fn scenario_segmentos_relativos_que_escapan_hacia_windows() {
    let raw = r"C:\Users\ana\AppData\Local\Temp\..\..\..\..\..\Windows\System32\x.dll";
    assert_eq!(decide(&machine(), raw), protected(R::WindowsDir));
}

/// Escenario: Prefijo verbatim.
#[test]
fn scenario_prefijo_verbatim() {
    let fs = machine();
    let plain = decide(&fs, r"C:\Windows\Temp\x.tmp");
    assert!(plain.is_allowed());
    assert_eq!(decide(&fs, r"\\?\C:\Windows\Temp\x.tmp"), plain);
}

/// Escenario: Subida por encima de la raiz.
#[test]
fn scenario_subida_por_encima_de_la_raiz() {
    assert_eq!(
        decide(&machine(), r"C:\..\Windows\x.dll"),
        protected(R::Unparseable(PathRejection::EscapesRoot))
    );
}

/// Escenario: Forma verbatim con segmentos relativos.
#[test]
fn scenario_forma_verbatim_con_segmentos_relativos() {
    assert_eq!(
        decide(&machine(), r"\\?\C:\Users\ana\..\bob\x.txt"),
        protected(R::Unparseable(PathRejection::VerbatimNotNormalized))
    );
}

// --- Requisito: Rechazo de formas de ruta no locales o ambiguas ---

/// Escenario: Flujo de datos alternativo.
#[test]
fn scenario_flujo_de_datos_alternativo() {
    let decision = decide(&machine(), r"C:\Users\ana\AppData\Local\Temp\a.txt:oculto");
    assert_eq!(reason_code(&decision), "alternate_data_stream");
}

/// Escenario: Ruta de red.
#[test]
fn scenario_ruta_de_red() {
    for raw in [
        r"\\servidor\compartido\a.tmp",
        r"\\?\UNC\servidor\compartido\a.tmp",
    ] {
        assert_eq!(
            reason_code(&decide(&machine(), raw)),
            "network_path",
            "{raw}"
        );
    }
}

/// Escenario: Ruta de dispositivo.
#[test]
fn scenario_ruta_de_dispositivo() {
    let decision = decide(&machine(), r"\\.\C:\Users\ana\a.tmp");
    assert_eq!(reason_code(&decision), "device_path");
}

/// Escenario: Punto final que Windows recortaria.
#[test]
fn scenario_punto_final_que_windows_recortaria() {
    for raw in [
        r"C:\Windows.\System32\x.dll",
        "C:\\Users\\ana\\AppData\\Local\\Temp\\a.tmp ",
    ] {
        assert_eq!(
            reason_code(&decide(&machine(), raw)),
            "trailing_dot_or_space",
            "{raw:?}"
        );
    }
}

/// Escenario: Nombre de dispositivo reservado.
#[test]
fn scenario_nombre_de_dispositivo_reservado() {
    let decision = decide(&machine(), r"C:\Users\ana\AppData\Local\Temp\nul.txt");
    assert_eq!(reason_code(&decision), "reserved_device_name");
}

/// Escenario: Ruta relativa.
#[test]
fn scenario_ruta_relativa() {
    for raw in [r"Temp\a.tmp", r"C:Temp\a.tmp"] {
        assert_eq!(
            reason_code(&decide(&machine(), raw)),
            "not_absolute",
            "{raw}"
        );
    }
}

// --- Requisito: Comparacion de nombres sin distinguir mayusculas como NTFS ---

/// Escenario: Mayusculas mezcladas.
#[test]
fn scenario_mayusculas_mezcladas() {
    let raw = r"c:\wInDoWs\sYsTeM32\x.dll";
    let fs = machine().with_canonical(raw, r"C:\Windows\System32\x.dll");
    assert_eq!(decide(&fs, raw), protected(R::WindowsDir));
}

/// Escenario: Mayusculas no ASCII en el nombre de usuario.
#[test]
fn scenario_mayusculas_no_ascii_en_el_nombre_de_usuario() {
    let real = r"C:\Users\Jürgen\AppData\Local\Temp\a.tmp";
    let raw = r"C:\Users\JÜRGEN\AppData\Local\Temp\a.tmp";
    // La entrada existe con ambas grafias (el fake distingue mayusculas; NTFS no).
    let fs = tree(&[real, raw], &[])
        .with_identity(raw, same_identity())
        .with_identity(real, same_identity())
        .with_canonical(raw, real);
    let checked = allowed(decide_with(ctx_c_user("Jürgen"), &fs, raw));
    assert_eq!(checked.path.to_string(), real);
    // Otro usuario no gana acceso por parecerse.
    let other = tree(&[r"C:\Users\Jurgen\AppData\Local\Temp\a.tmp"], &[]);
    assert_eq!(
        decide_with(
            ctx_c_user("Jürgen"),
            &other,
            r"C:\Users\Jurgen\AppData\Local\Temp\a.tmp"
        ),
        protected(R::OtherUserProfile)
    );
}

/// Escenario: Sin equivalencias por expansion.
#[test]
fn scenario_sin_equivalencias_por_expansion() {
    let raw = r"C:\Users\STRASSE\AppData\Local\Temp\a.tmp";
    let fs = tree(&[raw], &[]);
    assert_eq!(
        decide_with(ctx_c_user("Straße"), &fs, raw),
        protected(R::OtherUserProfile)
    );
}

// --- Requisito: Nombres cortos 8.3 resueltos por la ruta real ---

fn short_name_fs() -> FakeFileSystem {
    tree(
        &[
            r"C:\PROGRA~1\App\a.log",
            r"C:\Program Files\App\a.log",
            r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp",
            r"C:\Users\ana maria\AppData\Local\Temp\a.tmp",
        ],
        &[],
    )
    .with_canonical(r"C:\PROGRA~1\App\a.log", r"C:\Program Files\App\a.log")
    .with_canonical(
        r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp",
        r"C:\Users\ana maria\AppData\Local\Temp\a.tmp",
    )
}

/// Escenario: Nombre corto de Program Files.
#[test]
fn scenario_nombre_corto_de_program_files() {
    assert_eq!(
        decide(&short_name_fs(), r"C:\PROGRA~1\App\a.log"),
        protected(R::ProgramFiles)
    );
}

/// Escenario: Nombre corto del perfil.
#[test]
fn scenario_nombre_corto_del_perfil() {
    let checked = allowed(decide_with(
        ctx_c_user("ana maria"),
        &short_name_fs(),
        r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp",
    ));
    assert_eq!(
        checked.path.to_string(),
        r"C:\Users\ana maria\AppData\Local\Temp\a.tmp"
    );
}

// --- Requisito: Contexto de proteccion inyectado ---

/// Escenario: Windows instalado en otra unidad.
#[test]
fn scenario_windows_instalado_en_otra_unidad() {
    let fs = tree(
        &[r"D:\Windows\System32\x.dll", r"D:\Windows\Temp\a.tmp"],
        &[],
    );
    assert_eq!(
        decide_with(ctx_d_ana(), &fs, r"D:\Windows\System32\x.dll"),
        protected(R::WindowsDir)
    );
    assert!(decide_with(ctx_d_ana(), &fs, r"D:\Windows\Temp\a.tmp").is_allowed());
}

/// Escenario: Usuario actual del contexto.
#[test]
fn scenario_usuario_actual_del_contexto() {
    let fs = machine();
    assert!(decide(&fs, r"C:\Users\ana\Downloads\setup-viejo.exe").is_allowed());
    assert_eq!(
        decide(&fs, r"C:\Users\bob\Downloads\a.txt"),
        protected(R::OtherUserProfile)
    );
    assert_eq!(
        decide(&fs, r"C:\Users\Public\Documents\a.txt"),
        protected(R::OtherUserProfile)
    );
}

// --- Requisito: Contexto invalido o incompleto protege todo ---

fn unavailable(error: ContextError) -> PathProtection<'static, FakeFileSystem> {
    // El fake vive durante todo el programa de test: se fuga a proposito (solo tests).
    let fs: &'static FakeFileSystem = Box::leak(Box::new(machine()));
    PathProtection::new(fs, Err(error))
}

/// Escenario: ProgramData no resoluble.
#[test]
fn scenario_programdata_no_resoluble() {
    let mut raw = raw_c_ana();
    raw.program_data = None;
    let error = ProtectionContext::from_raw(raw).unwrap_err();
    let service = unavailable(error);
    for path in [
        r"C:\Users\ana\AppData\Local\Temp\a.tmp",
        r"C:\Windows\Temp\x.tmp",
        r"C:\Users\ana\Downloads\setup-viejo.exe",
    ] {
        assert_eq!(
            service.check(Path::new(path)),
            protected(R::ContextUnavailable),
            "{path}"
        );
    }
}

/// Escenario: Ubicacion que es raiz de volumen.
#[test]
fn scenario_ubicacion_que_es_raiz_de_volumen() {
    let mut raw = raw_c_ana();
    raw.program_data = Some(r"C:\".to_owned());
    let error = ProtectionContext::from_raw(raw).unwrap_err();
    let decision = unavailable(error).check(Path::new(r"C:\Users\ana\AppData\Local\Temp\a.tmp"));
    assert_eq!(decision, protected(R::ContextUnavailable));
    assert_eq!(reason_code(&decision), "context_unavailable");
}

/// Escenario: Usuario actual desconocido.
#[test]
fn scenario_usuario_actual_desconocido() {
    let fs = machine();
    for path in [
        r"C:\Users\ana\AppData\Local\Temp\a.tmp",
        r"C:\Users\ana\Downloads\setup-viejo.exe",
    ] {
        assert_eq!(
            decide_with(ctx_c_no_user(), &fs, path),
            protected(R::OtherUserProfile),
            "{path}"
        );
    }
}

// --- Requisito: Raices de volumen y entradas de sistema de la raiz protegidas ---

/// Escenario: Raiz del volumen.
#[test]
fn scenario_raiz_del_volumen() {
    assert_eq!(decide(&machine(), r"C:\"), protected(R::VolumeRoot));
}

/// Escenario: Papelera de reciclaje.
#[test]
fn scenario_papelera_de_reciclaje() {
    assert_eq!(
        decide(&machine(), r"C:\$Recycle.Bin\S-1-5-21-1\$R1.tmp"),
        protected(R::VolumeSystemEntry)
    );
}

/// Escenario: Archivo de paginacion.
#[test]
fn scenario_archivo_de_paginacion() {
    for path in [r"C:\pagefile.sys", r"C:\System Volume Information\x"] {
        assert_eq!(
            decide(&machine(), path),
            protected(R::VolumeSystemEntry),
            "{path}"
        );
    }
}

// --- Requisito: Anclas del sistema y del usuario no eliminables ---

/// Escenario: Carpeta temporal en si.
#[test]
fn scenario_carpeta_temporal_en_si() {
    let decision = decide(&machine(), TEMP);
    assert_eq!(decision, protected(R::Anchor(AnchorKind::Temp)));
    assert_eq!(reason_code(&decision), "anchor");
}

/// Escenario: Raiz de una excepcion en si.
#[test]
fn scenario_raiz_de_una_excepcion_en_si() {
    let fs = machine();
    assert_eq!(
        decide(&fs, r"C:\Windows\Temp"),
        protected(R::Anchor(AnchorKind::ExceptionRoot))
    );
    assert!(decide(&fs, r"C:\Windows\Temp\a.tmp").is_allowed());
}

/// Escenario: Carpeta Descargas en si.
#[test]
fn scenario_carpeta_descargas_en_si() {
    assert_eq!(
        decide(&machine(), r"C:\Users\ana\Downloads"),
        protected(R::Anchor(AnchorKind::KnownFolder))
    );
}

// --- Requisito: Perfiles de otros usuarios protegidos ---

/// Escenario: Temporales de otro usuario.
#[test]
fn scenario_temporales_de_otro_usuario() {
    assert_eq!(
        decide(&machine(), r"C:\Users\bob\AppData\Local\Temp\a.tmp"),
        protected(R::OtherUserProfile)
    );
}

/// Escenario: Perfil publico.
#[test]
fn scenario_perfil_publico() {
    assert_eq!(
        decide(&machine(), r"C:\Users\Public\Documents\a.txt"),
        protected(R::OtherUserProfile)
    );
}

/// Escenario: Nombre de usuario con espacios y Unicode.
#[test]
fn scenario_nombre_de_usuario_con_espacios_y_unicode() {
    let own = r"C:\Users\José María\AppData\Local\Temp\a.tmp";
    let foreign = r"C:\Users\José\AppData\Local\Temp\a.tmp";
    let fs = tree(&[own, foreign], &[]);
    assert!(decide_with(ctx_c_user("José María"), &fs, own).is_allowed());
    assert_eq!(
        decide_with(ctx_c_user("José María"), &fs, foreign),
        protected(R::OtherUserProfile)
    );
}

// --- Requisito: Solo volumenes soportados ---

/// Escenario: Archivo en otro disco.
#[test]
fn scenario_archivo_en_otro_disco() {
    assert_eq!(
        decide(&machine(), r"D:\Temp\a.tmp"),
        protected(R::UnsupportedVolume)
    );
}

// --- Requisito: Raices protegidas ---

/// Escenario: Program Files (x86).
#[test]
fn scenario_program_files_x86() {
    assert_eq!(
        decide(&machine(), r"C:\Program Files (x86)\App\cache\a.tmp"),
        protected(R::ProgramFiles)
    );
}

/// Escenario: Cookies del navegador.
#[test]
fn scenario_cookies_del_navegador() {
    let decision = decide(
        &machine(),
        r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\Network\Cookies",
    );
    assert_eq!(decision, protected(R::BrowserProfile(Browser::Chrome)));
    assert_eq!(reason_code(&decision), "browser_profile");
}

/// Escenario: Base de datos de la app.
#[test]
fn scenario_base_de_datos_de_la_app() {
    assert_eq!(
        decide(
            &machine(),
            r"C:\Users\ana\AppData\Local\pc-optimizer\pc-optimizer.db"
        ),
        protected(R::AppDir)
    );
}

/// Escenario: Ruta fuera de toda raiz protegida.
#[test]
fn scenario_ruta_fuera_de_toda_raiz_protegida() {
    let checked = allowed(decide(
        &machine(),
        r"C:\Users\ana\Downloads\setup-viejo.exe",
    ));
    assert_eq!(checked.rule, AllowRule::Default);
}

// --- Requisito: Lista cerrada de excepciones ---

/// Escenario: Temporales de Windows.
#[test]
fn scenario_temporales_de_windows() {
    assert!(decide(&machine(), r"C:\Windows\Temp\x.tmp").is_allowed());
}

/// Escenario: Cache de Windows Update.
#[test]
fn scenario_cache_de_windows_update() {
    let fs = machine();
    assert!(decide(&fs, r"C:\Windows\SoftwareDistribution\Download\abc\x.cab").is_allowed());
    assert_eq!(
        decide(&fs, r"C:\Windows\SoftwareDistribution\DataStore\x.edb"),
        protected(R::WindowsDir)
    );
}

/// Escenario: Cache de Chrome en cualquier perfil.
#[test]
fn scenario_cache_de_chrome_en_cualquier_perfil() {
    assert!(
        decide(
            &machine(),
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Profile 2\Cache\Cache_Data\f_000001"
        )
        .is_allowed()
    );
}

/// Escenario: Comodin de un solo componente.
#[test]
fn scenario_comodin_de_un_solo_componente() {
    assert_eq!(
        decide(
            &machine(),
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Cache\x"
        ),
        protected(R::BrowserProfile(Browser::Chrome))
    );
}

/// Escenario: Cache de Firefox.
#[test]
fn scenario_cache_de_firefox() {
    let fs = machine();
    assert!(
        decide(
            &fs,
            r"C:\Users\ana\AppData\Local\Mozilla\Firefox\Profiles\ab12.default\cache2\entries\X"
        )
        .is_allowed()
    );
    assert_eq!(
        decide(
            &fs,
            r"C:\Users\ana\AppData\Roaming\Mozilla\Firefox\Profiles\ab12.default\logins.json"
        ),
        protected(R::BrowserProfile(Browser::Firefox))
    );
}

// --- Requisito: Precedencia de reglas ---

/// Escenario: Excepcion no anula perfil ajeno.
#[test]
fn scenario_excepcion_no_anula_perfil_ajeno() {
    assert_eq!(
        decide(
            &machine(),
            r"C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Cache\x"
        ),
        protected(R::OtherUserProfile)
    );
}

/// Escenario: Primera regla protegida que coincide.
#[test]
fn scenario_primera_regla_protegida_que_coincide() {
    let mut raw = raw_c_ana();
    raw.app_dirs = vec![r"C:\Program Files\pc-optimizer".to_owned()];
    let ctx = ProtectionContext::from_raw(raw).unwrap();
    let inside = r"C:\Program Files\pc-optimizer\data\pc-optimizer.db";
    let fs = tree(&[inside], &[]);
    // Program Files precede a app_dir en el orden documentado.
    assert_eq!(decide_with(ctx, &fs, inside), protected(R::ProgramFiles));
}

// --- Requisito: Reparse points nunca atravesados ---

/// Escenario: Junction en TEMP hacia Windows.
#[test]
fn scenario_junction_en_temp_hacia_windows() {
    let fs = tree(&[r"C:\Users\ana\AppData\Local\Temp\j\System32\x.dll"], &[])
        .with_reparse_point(r"C:\Users\ana\AppData\Local\Temp\j");
    assert_eq!(
        decide(&fs, r"C:\Users\ana\AppData\Local\Temp\j\System32\x.dll"),
        protected(R::ReparsePointInPath)
    );
}

/// Escenario: Junction hacia una ruta que seria elegible.
#[test]
fn scenario_junction_hacia_una_ruta_que_seria_elegible() {
    let fs = tree(&[r"C:\Users\ana\AppData\Local\Temp\j\a.exe"], &[])
        .with_reparse_point(r"C:\Users\ana\AppData\Local\Temp\j");
    assert_eq!(
        decide(&fs, r"C:\Users\ana\AppData\Local\Temp\j\a.exe"),
        protected(R::ReparsePointInPath)
    );
}

/// Variante nube del mismo requisito: un ancestro de nube se omite en vez de protegerse.
#[test]
fn scenario_junction_en_temp_hacia_windows_con_ancestro_de_nube() {
    let fs = tree(&[r"C:\Users\ana\AppData\Local\Temp\j\a.exe"], &[])
        .with_cloud_reparse_point(r"C:\Users\ana\AppData\Local\Temp\j");
    assert!(matches!(
        decide(&fs, r"C:\Users\ana\AppData\Local\Temp\j\a.exe"),
        ProtectionDecision::SkippedCloud { .. }
    ));
}

// --- Requisito: Un reparse point como objetivo no es elegible ---

/// Escenario: Symlink dentro de TEMP.
#[test]
fn scenario_symlink_dentro_de_temp() {
    let fs = tree(&[r"C:\Windows\System32\x.dll"], &[TEMP])
        .with_reparse_point(r"C:\Users\ana\AppData\Local\Temp\link.dll");
    assert_eq!(
        decide(&fs, r"C:\Users\ana\AppData\Local\Temp\link.dll"),
        protected(R::ReparsePoint)
    );
}

/// Escenario: Junction dentro de TEMP.
#[test]
fn scenario_junction_dentro_de_temp() {
    let fs = tree(&[], &[TEMP, r"C:\Users\ana\AppData\Local\Temp\vacia"])
        .with_reparse_point(r"C:\Users\ana\AppData\Local\Temp\j");
    assert_eq!(
        decide(&fs, r"C:\Users\ana\AppData\Local\Temp\j"),
        protected(R::ReparsePoint)
    );
}

// --- Requisito: Ruta lexica y ruta real deben coincidir ---

/// Escenario: Redireccion no detectada por metadatos.
#[test]
fn scenario_redireccion_no_detectada_por_metadatos() {
    let raw = r"C:\Users\ana\AppData\Local\Temp\x\a.tmp";
    let fs = tree(&[raw, r"C:\Windows\a.tmp"], &[]).with_canonical(raw, r"C:\Windows\a.tmp");
    let decision = decide(&fs, raw);
    assert!(
        matches!(decision, ProtectionDecision::Protected(_)),
        "{decision:?}"
    );
    assert_eq!(decision, protected(R::ResolvedPathMismatch));
}

/// Escenario: Unidad sustituida.
///
/// `ProtectionContext::from_raw` solo soporta la unidad del sistema, asi que una unidad `subst`
/// ni siquiera llega a la resolucion (`UnsupportedVolume`, mas estricto que el spec); y si la
/// ruta real de un archivo de `C:` cae en otra unidad, es `ResolvedPathMismatch`.
#[test]
fn scenario_unidad_sustituida() {
    let fs = tree(&[r"X:\System32\x.dll", r"C:\Windows\System32\x.dll"], &[])
        .with_canonical(r"X:\System32\x.dll", r"C:\Windows\System32\x.dll");
    assert_eq!(
        decide(&fs, r"X:\System32\x.dll"),
        protected(R::UnsupportedVolume)
    );
    let raw = r"C:\Users\ana\AppData\Local\Temp\a.tmp";
    let subst = machine().with_canonical(raw, r"X:\Users\ana\AppData\Local\Temp\a.tmp");
    assert_eq!(decide(&subst, raw), protected(R::ResolvedPathMismatch));
}

// --- Requisito: Contenido en la nube omitido ---

/// Escenario: Archivo de OneDrive solo en la nube.
#[test]
fn scenario_archivo_de_onedrive_solo_en_la_nube() {
    let fs = tree(&[], &[r"C:\Users\ana\OneDrive"])
        .with_cloud_placeholder(r"C:\Users\ana\OneDrive\video.mp4", 0);
    assert!(matches!(
        decide(&fs, r"C:\Users\ana\OneDrive\video.mp4"),
        ProtectionDecision::SkippedCloud { .. }
    ));
}

/// Escenario: Marcador en ruta protegida.
#[test]
fn scenario_marcador_en_ruta_protegida() {
    let fs = tree(&[], &[r"C:\Users\bob\OneDrive"])
        .with_cloud_placeholder(r"C:\Users\bob\OneDrive\video.mp4", 0);
    assert_eq!(
        decide(&fs, r"C:\Users\bob\OneDrive\video.mp4"),
        protected(R::OtherUserProfile)
    );
}

// --- Requisito: Archivos con varios hard links no elegibles ---

/// Escenario: Hard link en TEMP a un archivo de sistema.
#[test]
fn scenario_hard_link_en_temp_a_un_archivo_de_sistema() {
    let fs = tree(&[r"C:\Windows\System32\x.dll"], &[TEMP])
        .with_file(r"C:\Users\ana\AppData\Local\Temp\h.dll", 7, None)
        .with_hard_links(r"C:\Users\ana\AppData\Local\Temp\h.dll", 2);
    assert_eq!(
        decide(&fs, r"C:\Users\ana\AppData\Local\Temp\h.dll"),
        protected(R::MultipleHardLinks)
    );
}

// --- Requisito: La decision sobre un directorio no cubre su contenido ---

/// Escenario: Directorio elegible con contenido protegido.
#[test]
fn scenario_directorio_elegible_con_contenido_protegido() {
    let fs = tree(&[], &[r"C:\Users\ana\AppData\Local\Temp\d"])
        .with_reparse_point(r"C:\Users\ana\AppData\Local\Temp\d\j");
    let directory = allowed(decide(&fs, r"C:\Users\ana\AppData\Local\Temp\d"));
    assert_eq!(directory.kind, EntryKind::Directory);
    assert_eq!(
        decide(&fs, r"C:\Users\ana\AppData\Local\Temp\d\j"),
        protected(R::ReparsePoint)
    );
}

// --- Requisito: Re-verificacion anti-TOCTOU antes de borrar ---

const SCANNED: &str = r"C:\Users\ana\AppData\Local\Temp\d\a.tmp";

fn scanned_fs() -> FakeFileSystem {
    tree(&[SCANNED], &[])
}

fn scan(fs: &FakeFileSystem) -> CheckedPath {
    allowed(decide(fs, SCANNED))
}

fn assert_not_delete_target(
    result: Result<super::service::DeleteTarget, ProtectionDecision>,
) -> ProtectionDecision {
    match result {
        Ok(_) => panic!("prepare_delete no debia entregar un objetivo"),
        Err(decision) => decision,
    }
}

/// Escenario: Archivo reemplazado por un symlink.
#[test]
fn scenario_archivo_reemplazado_por_un_symlink() {
    let fs = scanned_fs();
    let scanned = scan(&fs);
    fs.replace_with_reparse_point(Path::new(SCANNED));
    let service = PathProtection::new(&fs, Ok(ctx_c_ana()));
    assert_eq!(service.reverify(&scanned), protected(R::ReparsePoint));
    let refused = assert_not_delete_target(service.prepare_delete(&scanned));
    assert_eq!(refused, protected(R::ReparsePoint));
    // Al fallar la re-verificacion no queda nada fijado.
    assert!(fs.pinned().is_empty());
}

/// Escenario: Archivo reemplazado por otro con el mismo nombre.
#[test]
fn scenario_archivo_reemplazado_por_otro_con_el_mismo_nombre() {
    let fs = scanned_fs();
    let scanned = scan(&fs);
    fs.recreate_file(
        Path::new(SCANNED),
        crate::domain::ports::FileIdentity {
            volume_serial: 1,
            file_id: 999_999,
        },
    );
    let service = PathProtection::new(&fs, Ok(ctx_c_ana()));
    assert_eq!(service.reverify(&scanned), protected(R::IdentityChanged));
    assert_eq!(
        assert_not_delete_target(service.prepare_delete(&scanned)),
        protected(R::IdentityChanged)
    );
}

/// Escenario: Carpeta ancestro reemplazada por una junction.
#[test]
fn scenario_carpeta_ancestro_reemplazada_por_una_junction() {
    let fs = scanned_fs();
    let scanned = scan(&fs);
    fs.replace_with_reparse_point(Path::new(r"C:\Users\ana\AppData\Local\Temp\d"));
    let service = PathProtection::new(&fs, Ok(ctx_c_ana()));
    assert!(matches!(
        service.reverify(&scanned),
        ProtectionDecision::Protected(_)
    ));
    assert!(matches!(
        assert_not_delete_target(service.prepare_delete(&scanned)),
        ProtectionDecision::Protected(_)
    ));
}

/// Escenario: Ancestro fijado durante el movimiento.
#[test]
fn scenario_ancestro_fijado_durante_el_movimiento() {
    let fs = scanned_fs();
    let scanned = scan(&fs);
    let service = PathProtection::new(&fs, Ok(ctx_c_ana()));
    let target = match service.prepare_delete(&scanned) {
        Ok(target) => target,
        Err(decision) => panic!("se esperaba un objetivo, dio {decision:?}"),
    };
    let pinned = fs.pinned();
    assert!(pinned.contains(&Path::new(r"C:\Users\ana\AppData\Local\Temp\d").to_path_buf()));
    assert!(pinned.contains(&Path::new(TEMP).to_path_buf()));
    // Vigente mientras el objetivo vive...
    assert_eq!(fs.pinned(), pinned);
    drop(target);
    // ...y liberada al soltarlo.
    assert!(fs.pinned().is_empty());
}

/// Escenario: Ruta a mover.
#[test]
fn scenario_ruta_a_mover() {
    // El alias corto y la ruta real son el mismo archivo: misma identidad.
    let same = crate::domain::ports::FileIdentity {
        volume_serial: 1,
        file_id: 4242,
    };
    let fs = short_name_fs()
        .with_identity(r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp", same)
        .with_identity(r"C:\Users\ana maria\AppData\Local\Temp\a.tmp", same);
    let service = PathProtection::new(&fs, Ok(ctx_c_user("ana maria")));
    let scanned = allowed(service.check(Path::new(r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp")));
    let target = match service.prepare_delete(&scanned) {
        Ok(target) => target,
        Err(decision) => panic!("se esperaba un objetivo, dio {decision:?}"),
    };
    assert_eq!(
        target.path(),
        Path::new(r"C:\Users\ana maria\AppData\Local\Temp\a.tmp")
    );
    assert_ne!(
        target.path(),
        Path::new(r"C:\Users\ANAMAR~1\AppData\Local\Temp\a.tmp")
    );
}

// --- Requisito: Validacion de raices de reglas personalizadas ---

fn rule_root_fs() -> FakeFileSystem {
    tree(
        &[],
        &[
            r"C:\Users\ana\AppData\Local",
            r"C:\Users\ana\Downloads",
            r"C:\Users\bob\Downloads",
            r"C:\Windows",
        ],
    )
}

fn rule_root(fs: &FakeFileSystem, raw: &str) -> Result<String, ProtectedReason> {
    PathProtection::new(fs, Ok(ctx_c_ana()))
        .check_rule_root(Path::new(raw))
        .map(|path| path.to_string())
}

/// Escenario: Reglas sobre ubicaciones criticas.
#[test]
fn scenario_reglas_sobre_ubicaciones_criticas() {
    let fs = rule_root_fs();
    let expected = [
        (r"C:\", R::VolumeRoot),
        (r"C:\Users", R::Anchor(AnchorKind::UsersRoot)),
        (r"C:\Windows", R::Anchor(AnchorKind::WindowsDir)),
        (r"C:\Users\ana", R::Anchor(AnchorKind::UserProfile)),
        (
            r"C:\Users\ana\AppData\Local",
            R::Anchor(AnchorKind::LocalAppData),
        ),
    ];
    for (root, reason) in expected {
        assert_eq!(rule_root(&fs, root), Err(reason), "{root}");
    }
}

/// Escenario: Regla sobre Descargas.
#[test]
fn scenario_regla_sobre_descargas() {
    assert_eq!(
        rule_root(&rule_root_fs(), r"C:\Users\ana\Downloads"),
        Ok(r"C:\Users\ana\Downloads".to_owned())
    );
}

/// Escenario: Regla sobre otro usuario.
#[test]
fn scenario_regla_sobre_otro_usuario() {
    assert_eq!(
        rule_root(&rule_root_fs(), r"C:\Users\bob\Downloads"),
        Err(R::OtherUserProfile)
    );
}

// --- Requisito: Proteccion no configurable por el usuario ---

/// Escenario: Categoria que intenta limpiar fuera de la lista.
#[test]
fn scenario_categoria_que_intenta_limpiar_fuera_de_la_lista() {
    assert_eq!(
        decide(&machine(), r"C:\Windows\Logs\CBS\CBS.log"),
        protected(R::WindowsDir)
    );
}
