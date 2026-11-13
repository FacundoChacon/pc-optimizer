use super::*;
use crate::domain::junk::filter::{NameFilter, TEMP_MIN_AGE_MS};
use crate::domain::protection::context::ProtectionContext;
use crate::domain::protection::decision::RuleVerdict;
use crate::domain::protection::path::normalize_lexical;
use crate::domain::protection::rules::evaluate_rules;
use crate::domain::protection::test_support::{ctx_c_ana, ctx_c_no_user, ctx_d_ana, raw_c_ana};

const CHROMIUM_CACHES: &[&str] = &["Cache", "Code Cache", "GPUCache"];

fn of(ctx: &ProtectionContext, category: JunkCategory) -> Vec<JunkSource> {
    sources(ctx)
        .into_iter()
        .filter(|source| source.category == category)
        .collect()
}

fn roots(sources: &[JunkSource]) -> Vec<String> {
    sources.iter().map(|s| s.root.to_string()).collect()
}

fn only(ctx: &ProtectionContext, category: JunkCategory) -> JunkSource {
    let mut found = of(ctx, category);
    assert_eq!(found.len(), 1, "{category:?}: {found:?}");
    found.remove(0)
}

#[test]
fn temp_user_should_use_the_user_temp_with_the_24_hour_filter() {
    let source = only(&ctx_c_ana(), JunkCategory::TempUser);
    assert_eq!(source.root.to_string(), r"C:\Users\ana\AppData\Local\Temp");
    assert_eq!(source.scope, UserScope::PerUser);
    assert_eq!(source.filter.min_age_ms, Some(TEMP_MIN_AGE_MS));
    assert_eq!(source.depth, Depth::Recursive);
    assert_eq!(source.layout, Layout::Direct);
}

#[test]
fn temp_windows_and_wu_cache_should_be_machine_locations_under_windows() {
    let ctx = ctx_c_ana();
    let temp = only(&ctx, JunkCategory::TempWindows);
    assert_eq!(temp.root.to_string(), r"C:\Windows\Temp");
    assert_eq!(temp.scope, UserScope::Machine);
    assert_eq!(temp.filter.min_age_ms, Some(TEMP_MIN_AGE_MS));
    let wu = only(&ctx, JunkCategory::WuCache);
    assert_eq!(
        wu.root.to_string(),
        r"C:\Windows\SoftwareDistribution\Download"
    );
    assert_eq!(wu.scope, UserScope::Machine);
    assert_eq!(wu.filter, Filter::NONE);
}

#[test]
fn thumbnail_cache_should_read_only_the_first_level_filtering_thumbcache_names() {
    let source = only(&ctx_c_ana(), JunkCategory::ThumbnailCache);
    assert_eq!(
        source.root.to_string(),
        r"C:\Users\ana\AppData\Local\Microsoft\Windows\Explorer"
    );
    assert_eq!(source.depth, Depth::FirstLevel);
    assert_eq!(source.filter.name, NameFilter::ThumbCache);
    assert_eq!(source.scope, UserScope::PerUser);
}

#[test]
fn crash_dumps_should_have_four_locations_with_mixed_scopes() {
    let found = of(&ctx_c_ana(), JunkCategory::CrashDumps);
    let table: Vec<(String, UserScope)> = found
        .iter()
        .map(|s| (s.root.to_string(), s.scope))
        .collect();
    assert_eq!(
        table,
        vec![
            (
                r"C:\Users\ana\AppData\Local\CrashDumps".to_string(),
                UserScope::PerUser
            ),
            (r"C:\Windows\Minidump".to_string(), UserScope::Machine),
            (
                r"C:\ProgramData\Microsoft\Windows\WER\ReportArchive".to_string(),
                UserScope::Machine
            ),
            (
                r"C:\ProgramData\Microsoft\Windows\WER\ReportQueue".to_string(),
                UserScope::Machine
            ),
        ]
    );
}

#[test]
fn chromium_browsers_should_walk_exact_cache_subfolders_of_each_profile() {
    let ctx = ctx_c_ana();
    let expected = [
        (
            JunkCategory::BrowserCacheChrome,
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data",
        ),
        (
            JunkCategory::BrowserCacheEdge,
            r"C:\Users\ana\AppData\Local\Microsoft\Edge\User Data",
        ),
        (
            JunkCategory::BrowserCacheBrave,
            r"C:\Users\ana\AppData\Local\BraveSoftware\Brave-Browser\User Data",
        ),
    ];
    for (category, root) in expected {
        let source = only(&ctx, category);
        assert_eq!(source.root.to_string(), root, "{category:?}");
        assert_eq!(
            source.layout,
            Layout::Profiles {
                subdirs: CHROMIUM_CACHES
            }
        );
        assert_eq!(source.scope, UserScope::PerUser);
        assert_eq!(source.depth, Depth::Recursive);
    }
}

#[test]
fn firefox_should_walk_only_cache2_under_local_app_data_never_roaming() {
    let source = only(&ctx_c_ana(), JunkCategory::BrowserCacheFirefox);
    assert_eq!(
        source.root.to_string(),
        r"C:\Users\ana\AppData\Local\Mozilla\Firefox\Profiles"
    );
    assert_eq!(
        source.layout,
        Layout::Profiles {
            subdirs: &["cache2"]
        }
    );
}

#[test]
fn old_logs_and_recycle_bin_should_have_no_file_locations() {
    let ctx = ctx_c_ana();
    assert!(of(&ctx, JunkCategory::OldLogs).is_empty());
    assert!(of(&ctx, JunkCategory::RecycleBin).is_empty());
}

#[test]
fn sources_should_follow_a_windows_installed_on_another_drive() {
    let ctx = ctx_d_ana();
    let found = sources(&ctx);
    assert!(!found.is_empty());
    assert!(
        found.iter().all(|s| s.root.drive() == 'D'),
        "{:?}",
        roots(&found)
    );
    let temp = only(&ctx, JunkCategory::TempWindows);
    assert_eq!(temp.root.to_string(), r"D:\Windows\Temp");
}

#[test]
fn context_without_user_should_yield_only_machine_locations() {
    let found = sources(&ctx_c_no_user());
    assert!(found.iter().all(|s| s.scope == UserScope::Machine));
    let categories: Vec<JunkCategory> = found.iter().map(|s| s.category).collect();
    assert_eq!(
        categories,
        vec![
            JunkCategory::TempWindows,
            JunkCategory::WuCache,
            JunkCategory::CrashDumps,
            JunkCategory::CrashDumps,
            JunkCategory::CrashDumps,
        ]
    );
}

#[test]
fn context_without_temp_should_leave_temp_user_without_location() {
    let mut raw = raw_c_ana();
    if let Some(user) = raw.user.as_mut() {
        user.temp = None;
    }
    let ctx = ProtectionContext::from_raw(raw).expect("contexto valido");
    assert!(of(&ctx, JunkCategory::TempUser).is_empty());
    assert_eq!(of(&ctx, JunkCategory::ThumbnailCache).len(), 1);
}

#[test]
fn sources_should_follow_the_stable_category_order() {
    let order: Vec<JunkCategory> = sources(&ctx_c_ana()).iter().map(|s| s.category).collect();
    assert!(!order.is_empty());
    let mut sorted = order.clone();
    sorted.sort_by_key(|c| JunkCategory::ALL.iter().position(|a| a == c));
    assert_eq!(order, sorted);
}

/// Carpetas que el recorrido llega a listar: la raiz, o `<raiz>\Default\<subcarpeta>`.
fn walked_dirs(source: &JunkSource) -> Vec<NormalizedPath> {
    match source.layout {
        Layout::Direct => vec![source.root.clone()],
        Layout::Profiles { subdirs } => subdirs
            .iter()
            .map(|sub| source.root.joined(&["Default", sub]))
            .collect(),
    }
}

#[test]
fn every_walked_location_should_be_allowed_by_the_protection_rules_and_never_roaming() {
    let ctx = ctx_c_ana();
    let roaming = normalize_lexical(r"C:\Users\ana\AppData\Roaming").expect("ruta valida");
    let found = sources(&ctx);
    assert!(!found.is_empty());
    for source in &found {
        for dir in walked_dirs(source) {
            let child = dir.joined(&["x.tmp"]);
            let verdict = evaluate_rules(&ctx, &child);
            assert!(
                verdict.is_allowed(),
                "{:?}: {child} => {verdict:?}",
                source.category
            );
            assert!(!dir.starts_with(&roaming), "{dir}");
        }
    }
}

#[test]
fn a_location_outside_the_exceptions_should_be_protected_by_the_same_rules() {
    // Contraste del test anterior: la whitelist no basta si no esta en las excepciones de C-03.
    let ctx = ctx_c_ana();
    let logs = normalize_lexical(r"C:\Windows\Logs\CBS\x.log").expect("ruta valida");
    assert!(matches!(
        evaluate_rules(&ctx, &logs),
        RuleVerdict::Protected(_)
    ));
}
