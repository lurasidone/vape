fn agent_key(game: &mut crate::game::Game<'_>, key: winit::keyboard::KeyCode) {
    if game.serving.is_some() || game.options.dedicated.is_some() {
        return;
    }
    let module = match key {
        winit::keyboard::KeyCode::F8 => crate::game::vape_rs::core::ModuleId::Coordinates,
        winit::keyboard::KeyCode::F9 => crate::game::vape_rs::core::ModuleId::Fps,
        winit::keyboard::KeyCode::F10 => crate::game::vape_rs::core::ModuleId::Panic,
        _ => return,
    };
    let accepts_input = game.screen == crate::game::Screen::Playing
        && game.title.is_none()
        && !game.dialog_open()
        && game.server_packs.prompt.is_none();
    let client = crate::game::vape_rs::core::client();
    crate::game::vape_rs::core::with_client(&client, |runtime| {
        runtime.key(
            module,
            crate::game::vape_rs::input::KeyAction::Press,
            accepts_input,
        );
    });
}
