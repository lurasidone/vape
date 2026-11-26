fn no_slowdown_body(game: &mut crate::game::Game<'_>) {
    if game.serving.is_some() || game.options.dedicated.is_some() {
        return;
    }
    let client = crate::game::vape_rs::core::client().clone();
    let (no_slow, sprint) = crate::game::vape_rs::core::with_client(&client, |runtime| {
        (
            runtime.enabled(crate::game::vape_rs::core::ModuleId::NoSlowdown),
            runtime.enabled(crate::game::vape_rs::core::ModuleId::Sprint),
        )
    });
    if !no_slow {
        return;
    }
    if let Some(player) = game.player.as_mut() {
        player.item_use_speed = None;
        if sprint {
            player.item_use_slows_sprint = false;
        }
    }
}
