fn current_timer_steps(game: &crate::game::Game<'_>, steps: u32) -> u32 {
    <crate::clock::FixedStep as crate::game::vape_current_support::FixedStepExt>::vape_rs_timer_steps(
        &game.clock,
        steps,
    )
}
