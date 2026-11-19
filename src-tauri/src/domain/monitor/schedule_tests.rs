use super::*;

#[test]
fn on_time_should_schedule_the_previous_deadline_plus_the_interval() {
    assert_eq!(next_deadline(10_000, 11_000, 1_000), 11_000);
    assert_eq!(next_deadline(10_000, 10_600, 1_000), 11_000);
}

#[test]
fn late_by_less_than_an_interval_should_keep_the_cadence() {
    // 400 ms de atraso: el plazo sigue siendo prev + intervalo (ya vencio, se muestrea de inmediato).
    assert_eq!(next_deadline(10_000, 11_400, 1_000), 11_000);
    assert_eq!(next_deadline(20_000, 22_000, 1_000), 21_000);
}

#[test]
fn late_by_more_than_an_interval_should_restart_from_now_without_a_burst() {
    assert_eq!(next_deadline(10_000, 14_500, 1_000), 15_500);
    assert_eq!(next_deadline(0, 2_001, 500), 2_501);
}
