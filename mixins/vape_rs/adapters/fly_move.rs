fn fly_move(
    world: &mut crate::motion::MotionWorld<'_, '_, '_>,
    movement: horizon_core::geometry::Vec3,
) -> core::ops::ControlFlow<Result<()>> {
    use crate::game::vape_current_support::PlayerMotionExt;
    match world.player.vape_rs_take_fly_move(movement) {
        Some(movement) => core::ops::ControlFlow::Break(world.move_self(movement)),
        None => core::ops::ControlFlow::Continue(()),
    }
}
