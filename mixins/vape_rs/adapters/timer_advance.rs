fn timer_advance(
    clock: &mut crate::clock::FixedStep,
    elapsed: std::time::Duration,
    target_millis: f32,
) -> core::ops::ControlFlow<u32> {
    use crate::game::vape_current_support::FixedStepExt;
    match clock.vape_rs_timer_speed() {
        Some(speed) => core::ops::ControlFlow::Break(clock.vape_rs_advance_scaled(
            elapsed,
            target_millis,
            speed,
        )),
        None => core::ops::ControlFlow::Continue(()),
    }
}
