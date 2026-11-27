fn inv_walk_keys(
    game: &crate::game::Game<'_>,
    keys: crate::sprint::Keys,
) -> crate::sprint::Keys {
    // InvWalk: the movement keys physically held behind a screen join the
    // tick's screen-gated (so released) keys. The SDK's controlled tick is in
    // the library crate, so the keys are merged here, in the game crate, on
    // the call's argument instead of through a library-side hook.
    match crate::game::remote::vape_world_actions::inv_walk_keys(game) {
        Some(held) => crate::sprint::Keys {
            forward: keys.forward || held.forward,
            backward: keys.backward || held.backward,
            left: keys.left || held.left,
            right: keys.right || held.right,
            jump: keys.jump || held.jump,
            shift: keys.shift || held.shift,
            ..keys
        },
        None => keys,
    }
}