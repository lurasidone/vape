fn hud(game: &mut crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch) {
    if game.serving.is_some()
        || game.options.dedicated.is_some()
        || game.screen != crate::game::Screen::Playing
        || game.title.is_some()
        || game.ui.hide_gui
    {
        return;
    }
    let Some(player) = game.player.as_ref() else {
        return;
    };
    let snapshot = crate::game::vape_rs::ui::Snapshot {
        position: [player.position.x, player.position.y, player.position.z],
        fps: game.frame_reading().0 as f32,
        yaw: game.yaw,
        in_world: true,
        show_hud: true,
        ..Default::default()
    };
    let client = crate::game::vape_rs::core::client();
    let lines = crate::game::vape_rs::core::with_client(&client, |runtime| {
        runtime.hud(snapshot)
    });
    let atlas = &game.gui.atlas;
    for (row, line) in lines.iter().enumerate() {
        let text = format!("{}: {}", line.label, line.value);
        batch.text(atlas, &text, 8.0, 8.0 + row as f32 * 14.0, u32::MAX, true);
    }
}
