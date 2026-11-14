use std::collections::HashSet;

use super::*;

const EXPECTED: [(JunkCategory, &str); 11] = [
    (JunkCategory::TempUser, "temp_user"),
    (JunkCategory::TempWindows, "temp_windows"),
    (JunkCategory::BrowserCacheChrome, "browser_cache_chrome"),
    (JunkCategory::BrowserCacheEdge, "browser_cache_edge"),
    (JunkCategory::BrowserCacheFirefox, "browser_cache_firefox"),
    (JunkCategory::BrowserCacheBrave, "browser_cache_brave"),
    (JunkCategory::OldLogs, "old_logs"),
    (JunkCategory::WuCache, "wu_cache"),
    (JunkCategory::ThumbnailCache, "thumbnail_cache"),
    (JunkCategory::CrashDumps, "crash_dumps"),
    (JunkCategory::RecycleBin, "recycle_bin"),
];

#[test]
fn category_should_round_trip_through_the_exact_persisted_strings() {
    for (category, text) in EXPECTED {
        assert_eq!(category.as_str(), text);
        assert_eq!(JunkCategory::parse(text), Some(category));
    }
}

#[test]
fn parse_should_reject_categories_outside_the_whitelist() {
    for raw in [
        "custom_rule",
        "downloads",
        "program_files",
        "",
        "TEMP_USER",
        " temp_user",
        "temp_user ",
    ] {
        assert_eq!(JunkCategory::parse(raw), None, "{raw:?}");
    }
}

#[test]
fn all_should_list_each_category_once_in_the_stable_ui_order() {
    let expected: Vec<JunkCategory> = EXPECTED.iter().map(|(category, _)| *category).collect();
    assert_eq!(JunkCategory::ALL.to_vec(), expected);
    let unique: HashSet<JunkCategory> = JunkCategory::ALL.into_iter().collect();
    assert_eq!(unique.len(), 11);
}

#[test]
fn scope_should_classify_each_category_as_machine_per_user_or_mixed() {
    let machine = [
        JunkCategory::TempWindows,
        JunkCategory::WuCache,
        JunkCategory::OldLogs,
    ];
    let per_user = [
        JunkCategory::TempUser,
        JunkCategory::BrowserCacheChrome,
        JunkCategory::BrowserCacheEdge,
        JunkCategory::BrowserCacheFirefox,
        JunkCategory::BrowserCacheBrave,
        JunkCategory::ThumbnailCache,
        JunkCategory::RecycleBin,
    ];
    for category in machine {
        assert_eq!(category.scope(), CategoryScope::Machine, "{category:?}");
    }
    for category in per_user {
        assert_eq!(category.scope(), CategoryScope::PerUser, "{category:?}");
    }
    assert_eq!(JunkCategory::CrashDumps.scope(), CategoryScope::Mixed);
}

#[test]
fn category_scope_should_round_trip_through_the_persisted_strings() {
    for (scope, text) in [
        (CategoryScope::Machine, "machine"),
        (CategoryScope::PerUser, "per_user"),
        (CategoryScope::Mixed, "mixed"),
    ] {
        assert_eq!(scope.as_str(), text);
        assert_eq!(CategoryScope::parse(text), Some(scope));
    }
    assert_eq!(CategoryScope::parse("Machine"), None);
}
