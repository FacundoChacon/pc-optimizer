use std::path::Path;

use super::*;
use crate::domain::ports::PortError;
use crate::domain::ports::fakes::{FakeFileSystem, FakeSessionIdentity, user_sid};
use crate::domain::protection::context::{ProtectionContext, RawProtectionContext};
use crate::domain::protection::decision::{ProtectedReason, ProtectionDecision};
use crate::domain::protection::service::PathProtection;
use crate::domain::protection::test_support::raw_c_ana;

const ANA: u32 = 1001;
const ADMIN: u32 = 500;
const LUIS: u32 = 1002;

fn raw_sid(revision: u8, subs: &[u32]) -> Vec<u8> {
    let mut bytes = vec![revision, subs.len() as u8, 0, 0, 0, 0, 0, 5];
    for sub in subs {
        bytes.extend_from_slice(&sub.to_le_bytes());
    }
    bytes
}

// ---- 3.1 UserSid ----

#[test]
fn user_sid_should_accept_well_known_and_domain_user_sids() {
    let system = UserSid::from_bytes(raw_sid(1, &[18])).expect("S-1-5-18");
    assert_eq!(system.as_bytes().len(), 12);
    let user = UserSid::from_bytes(raw_sid(1, &[21, 1, 2, 3, 1001])).expect("S-1-5-21-...");
    assert_eq!(user.as_bytes().len(), 28);
    // 15 subautoridades es el maximo permitido.
    assert!(UserSid::from_bytes(raw_sid(1, &[1; 15])).is_some());
}

#[test]
fn user_sid_should_reject_malformed_bytes() {
    assert!(UserSid::from_bytes(Vec::new()).is_none());
    assert!(UserSid::from_bytes(vec![1, 0, 0, 0, 0, 0]).is_none());
    assert!(UserSid::from_bytes(raw_sid(2, &[18])).is_none());
    assert!(UserSid::from_bytes(raw_sid(0, &[18])).is_none());
    assert!(UserSid::from_bytes(raw_sid(1, &[1; 16])).is_none());
    // Longitud ≠ 8 + 4n: falta un byte, sobra un byte.
    let mut short = raw_sid(1, &[21, 1, 2, 3, 1001]);
    short.pop();
    assert!(UserSid::from_bytes(short).is_none());
    let mut long = raw_sid(1, &[18]);
    long.push(0);
    assert!(UserSid::from_bytes(long).is_none());
}

#[test]
fn user_sid_equality_should_compare_bytes() {
    assert_eq!(user_sid(ANA), user_sid(ANA));
    assert_ne!(user_sid(ANA), user_sid(LUIS));
}

// ---- 3.2 decide ----

fn evidence(process: Option<u32>, shell: Option<u32>, logon: Option<u32>) -> SessionEvidence {
    SessionEvidence {
        process_user: process.map(user_sid),
        shell_user: shell.map(user_sid),
        logon_user: logon.map(user_sid),
    }
}

const DIFFERENT: UserMatch = UserMatch::Mismatch(MismatchReason::DifferentUser);
const UNDETERMINED: UserMatch = UserMatch::Mismatch(MismatchReason::Undetermined);

#[test]
fn decide_should_follow_the_fail_closed_table() {
    let table = [
        (
            "ana/ana/ana",
            Some(ANA),
            Some(ANA),
            Some(ANA),
            UserMatch::Match,
        ),
        (
            "admin/ana/ana",
            Some(ADMIN),
            Some(ANA),
            Some(ANA),
            DIFFERENT,
        ),
        ("admin/sin/sin", Some(ADMIN), None, None, UNDETERMINED),
        (
            "sin token/ana/ana",
            None,
            Some(ANA),
            Some(ANA),
            UNDETERMINED,
        ),
        (
            "ana/ana/luis",
            Some(ANA),
            Some(ANA),
            Some(LUIS),
            UNDETERMINED,
        ),
        ("ana/sin/ana", Some(ANA), None, Some(ANA), UserMatch::Match),
        ("ana/ana/sin", Some(ANA), Some(ANA), None, UserMatch::Match),
        ("admin/ana/sin", Some(ADMIN), Some(ANA), None, DIFFERENT),
        ("admin/sin/ana", Some(ADMIN), None, Some(ANA), DIFFERENT),
        (
            "ana/luis/luis",
            Some(ANA),
            Some(LUIS),
            Some(LUIS),
            DIFFERENT,
        ),
        ("ana/sin/luis", Some(ANA), None, Some(LUIS), DIFFERENT),
        (
            "admin/ana/luis",
            Some(ADMIN),
            Some(ANA),
            Some(LUIS),
            UNDETERMINED,
        ),
        ("sin nada", None, None, None, UNDETERMINED),
    ];
    for (name, process, shell, logon, expected) in table {
        assert_eq!(decide(&evidence(process, shell, logon)), expected, "{name}");
    }
}

#[test]
fn mismatch_reason_should_use_stable_codes() {
    assert_eq!(MismatchReason::DifferentUser.as_str(), "different_user");
    assert_eq!(MismatchReason::Undetermined.as_str(), "undetermined");
}

// ---- 3.3 detect ----

fn session(process: u32, shell: Option<u32>, logon: Option<u32>) -> FakeSessionIdentity {
    FakeSessionIdentity::new(user_sid(process))
        .shell(shell.map(user_sid))
        .logon(logon.map(user_sid))
}

fn denied() -> PortError {
    PortError::PermissionDenied("acceso denegado".to_owned())
}

#[test]
fn detect_should_report_match_with_elevation() {
    let ctx = detect(&session(ANA, Some(ANA), Some(ANA)).elevated(true));
    assert_eq!(ctx.user_match(), UserMatch::Match);
    assert!(ctx.elevated());
    let ctx = detect(&session(ANA, Some(ANA), Some(ANA)));
    assert_eq!(ctx.user_match(), UserMatch::Match);
    assert!(!ctx.elevated());
}

#[test]
fn detect_should_flag_over_the_shoulder_elevation() {
    let ctx = detect(&session(ADMIN, Some(ANA), Some(ANA)).elevated(true));
    assert_eq!(ctx.user_match(), DIFFERENT);
    assert!(ctx.elevated());
}

#[test]
fn detect_should_be_undetermined_when_the_process_user_fails() {
    let mut identity = session(ANA, Some(ANA), Some(ANA)).elevated(true);
    identity.process_user = Err(PortError::Io("sin token".to_owned()));
    let ctx = detect(&identity);
    assert_eq!(ctx.user_match(), UNDETERMINED);
    // La elevacion es independiente de la coincidencia.
    assert!(ctx.elevated());
}

#[test]
fn detect_should_use_the_remaining_signal_when_the_shell_is_denied() {
    let mut identity = session(ANA, Some(ANA), Some(ANA));
    identity.shell_user = Err(denied());
    assert_eq!(detect(&identity).user_match(), UserMatch::Match);

    let mut other = session(ADMIN, None, Some(ANA));
    other.shell_user = Err(denied());
    assert_eq!(detect(&other).user_match(), DIFFERENT);
}

#[test]
fn detect_should_be_undetermined_when_both_signals_fail() {
    let mut identity = session(ANA, Some(ANA), Some(ANA));
    identity.shell_user = Err(denied());
    identity.logon_user = Err(PortError::NotFound("sesion".to_owned()));
    assert_eq!(detect(&identity).user_match(), UNDETERMINED);
}

#[test]
fn detect_should_report_not_elevated_when_elevation_cannot_be_read() {
    let mut identity = session(ANA, Some(ANA), Some(ANA));
    identity.process_elevated = Err(denied());
    let ctx = detect(&identity);
    assert!(!ctx.elevated());
    assert_eq!(ctx.user_match(), UserMatch::Match);

    let mut over_the_shoulder = session(ADMIN, Some(ANA), Some(ANA));
    over_the_shoulder.process_elevated = Err(denied());
    let ctx = detect(&over_the_shoulder);
    assert!(!ctx.elevated());
    assert_eq!(ctx.user_match(), DIFFERENT);
}

#[test]
fn undetermined_context_should_be_the_fail_closed_fallback() {
    let ctx = PrivilegeContext::undetermined();
    assert_eq!(ctx.user_match(), UNDETERMINED);
    assert!(!ctx.elevated());
}

// ---- 3.4 scope_access ----

fn context(user_match: UserMatch) -> PrivilegeContext {
    PrivilegeContext::new(user_match, true)
}

#[test]
fn scope_access_should_block_only_per_user_scope_on_mismatch() {
    let table = [
        (UserMatch::Match, UserScope::Machine, ScopeAccess::Allowed),
        (UserMatch::Match, UserScope::PerUser, ScopeAccess::Allowed),
        (DIFFERENT, UserScope::Machine, ScopeAccess::Allowed),
        (
            DIFFERENT,
            UserScope::PerUser,
            ScopeAccess::Blocked(MismatchReason::DifferentUser),
        ),
        (UNDETERMINED, UserScope::Machine, ScopeAccess::Allowed),
        (
            UNDETERMINED,
            UserScope::PerUser,
            ScopeAccess::Blocked(MismatchReason::Undetermined),
        ),
    ];
    for (user_match, scope, expected) in table {
        assert_eq!(
            scope_access(&context(user_match), scope),
            expected,
            "{user_match:?} x {scope:?}"
        );
    }
}

#[test]
fn scope_access_should_ignore_elevation() {
    for elevated in [true, false] {
        let ctx = PrivilegeContext::new(DIFFERENT, elevated);
        assert_eq!(
            scope_access(&ctx, UserScope::PerUser),
            ScopeAccess::Blocked(MismatchReason::DifferentUser)
        );
    }
}

// ---- 3.5 restrict_protection_context ----

fn check(raw: RawProtectionContext, path: &str) -> ProtectionDecision {
    let fs = FakeFileSystem::new()
        .canonical_as_is()
        .with_dir(r"C:\Users")
        .with_dir(r"C:\Users\ana")
        .with_dir(r"C:\Users\ana\AppData")
        .with_dir(r"C:\Users\ana\AppData\Local")
        .with_dir(r"C:\Users\ana\AppData\Local\Temp")
        .with_file(r"C:\Users\ana\AppData\Local\Temp\a.tmp", 7, None)
        .with_dir(r"C:\Users\admin")
        .with_dir(r"C:\Users\admin\AppData")
        .with_dir(r"C:\Users\admin\AppData\Local")
        .with_dir(r"C:\Users\admin\AppData\Local\Temp")
        .with_file(r"C:\Users\admin\AppData\Local\Temp\a.tmp", 7, None)
        .with_dir(r"C:\Windows")
        .with_dir(r"C:\Windows\Temp")
        .with_file(r"C:\Windows\Temp\a.tmp", 7, None);
    PathProtection::new(&fs, ProtectionContext::from_raw(raw)).check(Path::new(path))
}

#[test]
fn restrict_should_keep_the_context_identical_on_match() {
    let raw = raw_c_ana();
    let restricted = restrict_protection_context(raw.clone(), &context(UserMatch::Match));
    assert_eq!(restricted, raw);
}

#[test]
fn restrict_should_drop_only_the_user_on_mismatch() {
    for user_match in [DIFFERENT, UNDETERMINED] {
        let raw = raw_c_ana();
        let restricted = restrict_protection_context(raw.clone(), &context(user_match));
        assert!(restricted.user.is_none());
        let mut expected = raw;
        expected.user = None;
        assert_eq!(restricted, expected, "{user_match:?}");
    }
}

#[test]
fn restrict_should_protect_the_token_profile_on_mismatch() {
    let path = r"C:\Users\ana\AppData\Local\Temp\a.tmp";
    // Con coincidencia la ruta del propio usuario es elegible...
    assert!(check(raw_c_ana(), path).is_allowed());
    // ...y en mismatch pasa a ser perfil ajeno.
    let restricted = restrict_protection_context(raw_c_ana(), &context(DIFFERENT));
    assert_eq!(
        check(restricted.clone(), path),
        ProtectionDecision::Protected(ProtectedReason::OtherUserProfile)
    );
    assert_eq!(
        check(restricted, r"C:\Users\admin\AppData\Local\Temp\a.tmp"),
        ProtectionDecision::Protected(ProtectedReason::OtherUserProfile)
    );
}

#[test]
fn restrict_should_not_change_system_locations_on_mismatch() {
    let path = r"C:\Windows\Temp\a.tmp";
    let on_match = check(raw_c_ana(), path);
    let restricted = restrict_protection_context(raw_c_ana(), &context(UNDETERMINED));
    assert_eq!(check(restricted, path), on_match);
}
