use super::*;

const T0: i64 = 1_800_000_000_000;
const TTL: i64 = 2 * 60 * 1000;

fn c_root() -> VolumeRoot {
    VolumeRoot::from_drive_letter('C').unwrap()
}

fn prepared(id: u64) -> PreparedEmpty {
    PreparedEmpty::new(
        id,
        c_root(),
        RecycleBinUsage {
            items: 12,
            bytes: 340_000_000,
        },
        EpochMillis(T0),
    )
}

// --- PreparedEmpty::acknowledge ---

#[test]
fn prepared_should_expose_what_was_measured_and_expire_two_minutes_after_creation() {
    let request = prepared(4);
    assert_eq!(request.id(), 4);
    assert_eq!(request.root(), c_root());
    assert_eq!(request.usage().items, 12);
    assert_eq!(request.usage().bytes, 340_000_000);
    assert_eq!(request.created_at(), EpochMillis(T0));
    assert_eq!(request.expires_at(), EpochMillis(T0 + TTL));
}

#[test]
fn acknowledge_should_succeed_within_the_window_and_carry_the_token() {
    let ack = prepared(1)
        .acknowledge(EpochMillis(T0 + 5_000), 77)
        .unwrap();
    assert_eq!(ack.id(), 1);
    assert_eq!(ack.token(), 77);
    assert_eq!(ack.usage().items, 12);
}

#[test]
fn acknowledge_should_be_valid_at_exactly_two_minutes_and_expired_one_millisecond_later() {
    assert!(prepared(1).acknowledge(EpochMillis(T0 + TTL), 1).is_ok());
    assert_eq!(
        prepared(1).acknowledge(EpochMillis(T0 + TTL + 1), 1).err(),
        Some(EmptyRequestError::Expired)
    );
}

// --- AcknowledgedEmpty::check ---

fn acknowledged() -> AcknowledgedEmpty {
    prepared(1).acknowledge(EpochMillis(T0), 9).unwrap()
}

#[test]
fn check_should_accept_the_right_token_within_the_window() {
    assert_eq!(acknowledged().check(9, EpochMillis(T0 + TTL)), Ok(()));
}

#[test]
fn check_should_reject_a_different_token_even_when_expired_as_not_found() {
    assert_eq!(
        acknowledged().check(8, EpochMillis(T0 + 1)),
        Err(EmptyRequestError::NotFound)
    );
    assert_eq!(
        acknowledged().check(8, EpochMillis(T0 + TTL + 1)),
        Err(EmptyRequestError::NotFound)
    );
}

#[test]
fn check_should_report_expired_one_millisecond_after_the_window() {
    assert_eq!(
        acknowledged().check(9, EpochMillis(T0 + TTL + 1)),
        Err(EmptyRequestError::Expired)
    );
}

// --- EmptySlot ---

#[test]
fn slot_acknowledge_should_move_prepared_to_acknowledged_and_return_a_receipt() {
    let mut slot = EmptySlot::default();
    slot.store(prepared(1));
    let receipt = slot.acknowledge(1, EpochMillis(T0 + 1), 50).unwrap();
    assert_eq!(receipt.request_id, 1);
    assert_eq!(receipt.token, 50);
    assert_eq!(receipt.usage.items, 12);
    assert_eq!(receipt.expires_at, EpochMillis(T0 + TTL));
}

#[test]
fn slot_acknowledge_should_fail_not_found_for_an_unknown_id_and_keep_the_request() {
    let mut slot = EmptySlot::default();
    assert_eq!(
        slot.acknowledge(1, EpochMillis(T0), 50).err(),
        Some(EmptyRequestError::NotFound)
    );
    slot.store(prepared(2));
    assert_eq!(
        slot.acknowledge(1, EpochMillis(T0), 50).err(),
        Some(EmptyRequestError::NotFound)
    );
    assert!(slot.acknowledge(2, EpochMillis(T0), 50).is_ok());
}

#[test]
fn slot_acknowledge_twice_should_fail_not_found_and_keep_the_first_acknowledgement() {
    let mut slot = EmptySlot::default();
    slot.store(prepared(1));
    slot.acknowledge(1, EpochMillis(T0), 50).unwrap();
    assert_eq!(
        slot.acknowledge(1, EpochMillis(T0), 51).err(),
        Some(EmptyRequestError::NotFound)
    );
    let ack = slot.take_for_empty(1).unwrap();
    assert_eq!(ack.token(), 50);
}

#[test]
fn slot_acknowledge_should_fail_expired_and_consume_the_request() {
    let mut slot = EmptySlot::default();
    slot.store(prepared(1));
    assert_eq!(
        slot.acknowledge(1, EpochMillis(T0 + TTL + 1), 50).err(),
        Some(EmptyRequestError::Expired)
    );
    assert_eq!(
        slot.take_for_empty(1).err(),
        Some(EmptyRequestError::NotFound)
    );
}

#[test]
fn slot_store_should_replace_the_previous_request_at_any_stage() {
    let mut slot = EmptySlot::default();
    slot.store(prepared(1));
    slot.acknowledge(1, EpochMillis(T0), 50).unwrap();
    slot.store(prepared(2));
    assert_eq!(
        slot.take_for_empty(1).err(),
        Some(EmptyRequestError::NotFound)
    );
    slot.store(prepared(3));
    assert_eq!(
        slot.acknowledge(2, EpochMillis(T0), 1).err(),
        Some(EmptyRequestError::NotFound)
    );
}

#[test]
fn take_for_empty_should_demand_the_acknowledgement_and_always_empty_the_slot() {
    let mut slot = EmptySlot::default();
    slot.store(prepared(1));
    assert_eq!(
        slot.take_for_empty(1).err(),
        Some(EmptyRequestError::NotAcknowledged)
    );
    // La solicitud se consumio aunque fallo.
    assert_eq!(
        slot.take_for_empty(1).err(),
        Some(EmptyRequestError::NotFound)
    );
}

#[test]
fn take_for_empty_should_fail_not_found_for_another_id_and_consume_the_slot() {
    let mut slot = EmptySlot::default();
    slot.store(prepared(1));
    slot.acknowledge(1, EpochMillis(T0), 50).unwrap();
    assert_eq!(
        slot.take_for_empty(2).err(),
        Some(EmptyRequestError::NotFound)
    );
    assert_eq!(
        slot.take_for_empty(1).err(),
        Some(EmptyRequestError::NotFound)
    );
}

#[test]
fn take_for_empty_should_hand_over_the_acknowledged_request_once() {
    let mut slot = EmptySlot::default();
    slot.store(prepared(1));
    slot.acknowledge(1, EpochMillis(T0), 50).unwrap();
    assert!(slot.take_for_empty(1).is_ok());
    assert_eq!(
        slot.take_for_empty(1).err(),
        Some(EmptyRequestError::NotFound)
    );
}
