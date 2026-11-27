fn fly_move_body(
    game: &mut crate::game::Game<'_>,
    entity_collisions: &[horizon_core::geometry::Aabb],
) {
    use crate::game::vape_current_support::PlayerMotionExt;
    let Some((position, fall_distance, events)) =
        crate::game::vape_current_support::vape_rs_take_fly_before()
    else {
        return;
    };
    let Some(player) = game.player.as_mut() else {
        return;
    };
    let Some(movement) = player.vape_rs_take_fly_move(player.velocity) else {
        return;
    };
    // Put back the vanilla travel's displacement, fall and motion events for
    // this tick, then apply the module's move through the same collision path.
    player.position = horizon_core::geometry::Vec3::new(position[0], position[1], position[2]);
    player.fall_distance = fall_distance;
    player.events.truncate(events);
    let mut gate = horizon_world::move_gate::MoveGate::new();
    let _ = crate::motion::MotionWorld {
        entities: entity_collisions,
        player,
        scene: &mut game.world,
        assets: &game.pack.assets,
        collision_binding: game.collision,
        resources: game.motion,
        health: &game.health_resources,
        loop_context: &game.loop_context,
        damage_hooks: &game.damage_hooks,
        tag_items: None,
        block_registries: None,
    }
    .move_self(&mut gate, movement);
    // The seats were placed from the vanilla path this hook discarded.
    game.seat_own_stands();
}
