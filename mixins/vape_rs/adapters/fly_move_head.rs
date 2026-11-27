fn fly_move_head(game: &mut crate::game::Game<'_>) {
    match game.player.as_ref() {
        Some(player) => crate::game::vape_current_support::vape_rs_set_fly_before(
            [player.position.x, player.position.y, player.position.z],
            player.fall_distance,
            player.events.len(),
        ),
        None => crate::game::vape_current_support::vape_rs_set_fly_before(
            [f64::NAN; 3],
            f64::NAN,
            0,
        ),
    }
}
