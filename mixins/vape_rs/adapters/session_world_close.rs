fn session_change(game: &mut crate::game::Game<'_>) {
    if game.serving.is_some() || game.options.dedicated.is_some() { return; }
    crate::game::remote::vape_packets::flush_inbound(game);
    let client = crate::game::vape_rs::core::client().clone();
    crate::game::vape_rs::core::with_client(&client, |runtime| runtime.session_change(crate::game::vape_rs::core::lifecycle::CleanupReason::CloseWorld));
}
