//! Demostracion de C-13 (tarea 8.2): un test que falla a proposito para comprobar
//! que el job `rust` del CI se pone en rojo. Esta rama nunca se fusiona.

#[test]
fn ci_gate_should_turn_red_on_a_failing_test() {
    assert_eq!(1 + 1, 3, "fallo intencional de demostracion (C-13, 8.2)");
}
