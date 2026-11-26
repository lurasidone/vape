fn target_info_pot_use(game: &crate::game::Game<'_>, stack: &horizon_client::items::ItemStack) {
    // `use_potion_item` both drinks and throws; the thrown branch is the old
    // `launch_thrown` count now that the SDK has no separate launch function.
    if matches!(&*stack.item, "minecraft:splash_potion" | "minecraft:lingering_potion") {
        crate::game::vape_hud::target_heal_joined(game, stack);
    } else {
        crate::game::vape_hud::target_heal_used(game, Some(stack));
    }
}