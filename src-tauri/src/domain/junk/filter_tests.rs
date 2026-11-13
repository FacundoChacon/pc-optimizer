use super::*;

const NOW: EpochMillis = EpochMillis(1_800_000_000_000);
const HOUR: i64 = 3_600_000;
const MIN: i64 = 60_000;

fn ago(ms: i64) -> Option<EpochMillis> {
    Some(EpochMillis(NOW.0 - ms))
}

const THUMBS: Filter = Filter {
    min_age_ms: None,
    name: NameFilter::ThumbCache,
};
const TEMP: Filter = Filter {
    min_age_ms: Some(TEMP_MIN_AGE_MS),
    name: NameFilter::Any,
};
const LOGS: Filter = Filter {
    min_age_ms: Some(OLD_LOGS_MIN_AGE_MS),
    name: NameFilter::Any,
};

#[test]
fn thumbcache_filter_should_accept_only_thumbcache_db_files_ignoring_case() {
    for name in ["thumbcache_256.db", "THUMBCACHE_IDX.DB", "ThumbCache_sr.db"] {
        assert!(THUMBS.accepts(name, None, NOW), "{name}");
    }
    for name in [
        "iconcache_32.db",
        "thumbcache_256.db.bak",
        "thumbcache_.dbx",
        "xthumbcache_256.db",
        "thumbcache.db",
        "",
    ] {
        assert!(!THUMBS.accepts(name, None, NOW), "{name:?}");
    }
}

#[test]
fn temp_age_filter_should_require_at_least_24_hours_since_last_write() {
    assert_eq!(TEMP_MIN_AGE_MS, 24 * HOUR);
    assert!(
        !TEMP.accepts("a.tmp", ago(24 * HOUR - MIN), NOW),
        "23 h 59 min"
    );
    assert!(TEMP.accepts("a.tmp", ago(24 * HOUR), NOW), "24 h");
    assert!(TEMP.accepts("a.tmp", ago(3 * 24 * HOUR), NOW), "3 days");
}

#[test]
fn age_filter_should_reject_files_without_date_or_modified_in_the_future() {
    assert!(!TEMP.accepts("a.tmp", None, NOW));
    assert!(!TEMP.accepts("a.tmp", ago(-HOUR), NOW));
}

#[test]
fn old_logs_age_filter_should_distinguish_29_from_30_days() {
    let day = 24 * HOUR;
    assert!(!LOGS.accepts("a.log", ago(29 * day), NOW));
    assert!(LOGS.accepts("a.log", ago(30 * day), NOW));
}

#[test]
fn no_filter_should_accept_any_name_and_missing_date() {
    assert!(Filter::NONE.accepts("x.bin", None, NOW));
    assert!(Filter::NONE.accepts("x.bin", ago(1), NOW));
}
