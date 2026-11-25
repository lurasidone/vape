fn entity_pick(
    game: &crate::game::Game<'_>,
) -> core::ops::ControlFlow<Option<(usize, f64, [f64; 3])>> {
    crate::game::vape_combat::pick(game)
}
