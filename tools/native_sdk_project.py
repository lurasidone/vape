#!/usr/bin/env python3
"""Prepare a disposable current-SDK native project from the source tree.

The output lives under target/ and can be built by the matching Horizon
launcher. The SDK JSON must come from the signed SDK release bundle.
"""
import argparse
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target/current-native-mod"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("sdk_json", type=Path, help="sdk.json from the signed SDK release")
    args = parser.parse_args()

    try:
        sdk = json.loads(args.sdk_json.read_text())
        build = sdk["build"]
        commit = build["commit"]
        game = build["game"]
        mixins = sdk["mixins"]["rev"]
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
        raise SystemExit(f"invalid SDK metadata: {exc}") from exc
    if len(commit) != 40 or any(c not in "0123456789abcdef" for c in commit):
        raise SystemExit("SDK metadata has no full Horizon commit")
    if len(mixins) != 40 or any(c not in "0123456789abcdef" for c in mixins):
        raise SystemExit("SDK metadata has no full mixin-tool commit")

    runtime = ROOT / "target/bundle/mixins/vape_rs/runtime.rs"
    helpers = {
        name: ROOT / "mixins/vape_rs/adapters" / f"current_{name}.rs"
        for name in ("tick", "keys", "hud", "control_keys", "agent_keys")
    }
    if not runtime.is_file():
        raise SystemExit("runtime payload missing; run `cargo run --offline -p xtask -- pack` first")
    for helper in helpers.values():
        if not helper.is_file():
            raise SystemExit(f"native adapter missing: {helper.relative_to(ROOT)}")
    if OUTPUT.exists():
        shutil.rmtree(OUTPUT)
    (OUTPUT / "src/adapters").mkdir(parents=True)
    shutil.copyfile(runtime, OUTPUT / "src/runtime.rs")
    for name, helper in helpers.items():
        shutil.copyfile(helper, OUTPUT / f"src/adapters/current_{name}.rs")
    (OUTPUT / "Cargo.toml").write_text(
        '[package]\nname = "mod-vape-rs"\nversion = "0.1.0"\nedition = "2024"\n\n'
        '[dependencies]\nhorizon-mod-api = { path = ".horizon/sdk/api/crates/horizon-mod-api" }\n\n'
        '[workspace]\n'
    )
    (OUTPUT / "src/lib.rs").write_text(
        "use mod_api::Hooks;\n\n#[derive(Default)]\nstruct VapeRs;\n\n"
        "#[mod_api::horizon_mod]\nimpl Hooks for VapeRs {}\n"
    )
    (OUTPUT / "horizon-mod.toml").write_text(
        'id = "vape_rs"\nname = "vape-rs"\nversion = "0.1.0-dev"\n'
        'description = "Local Rust recreation of the Vape client for Horizon"\n'
        'authors = []\napi = "0.2"\nside = "client"\ncapabilities = ["registries"]\n\n'
        '[native]\n'
        f'game = "{game}"\nhorizon = "{commit}"\nmixins = "{mixins}"\n\n'
        '[[native.mixin]]\nid = "client-tick"\n'
        'target = "horizon_client::game::Game<\'_>::tick"\nkind = "HEAD"\n'
        'helper = "src/adapters/current_tick.rs"\n'
        'bindings = [{ name = "game", ty = "&mut crate::game::Game<\'_>", value = "&mut *self" }]\n\n'
        '[[native.mixin]]\nid = "control-key-names"\n'
        'target = "horizon_client::game::control::key_code"\nkind = "HEAD"\n'
        'cancellable = true\nhelper = "src/adapters/current_control_keys.rs"\n'
        'bindings = [{ name = "name", ty = "&str", value = "name" }]\n\n'
        '[[native.mixin]]\nid = "game-key-capture"\n'
        'target = "horizon_client::game::play_input::Game::mapped_key"\nkind = "HEAD"\n'
        'helper = "src/adapters/current_keys.rs"\n'
        'bindings = [\n'
        '  { name = "game", ty = "&mut crate::game::Game<\'_>", value = "&mut *self" },\n'
        '  { name = "key", ty = "winit::keyboard::KeyCode", value = "key" },\n'
        '  { name = "pressed", ty = "bool", value = "pressed" },\n'
        '  { name = "repeat", ty = "bool", value = "repeat" },\n'
        ']\n\n'
        '[[native.mixin]]\nid = "agent-key-capture"\n'
        'target = "horizon_client::game::control::Game<\'_>::control_key_down"\nkind = "HEAD"\n'
        'helper = "src/adapters/current_agent_keys.rs"\n'
        'bindings = [\n'
        '  { name = "game", ty = "&mut crate::game::Game<\'_>", value = "&mut *self" },\n'
        '  { name = "key", ty = "winit::keyboard::KeyCode", value = "key" },\n'
        ']\n\n'
        '[[native.mixin]]\nid = "client-hud"\n'
        'target = "horizon_client::game::extras::Game<\'_>::draw_extras"\nkind = "HEAD"\n'
        'helper = "src/adapters/current_hud.rs"\n'
        'bindings = [\n'
        '  { name = "game", ty = "&mut crate::game::Game<\'_>", value = "&mut *self" },\n'
        '  { name = "batch", ty = "&mut horizon_client::hud::HudBatch", value = "&mut *batch" },\n'
        ']\n\n'
        '[[native.module]]\nid = "vape-runtime"\n'
        'file = "crates/horizon-client/src/game.rs"\nname = "vape_rs"\n'
        'source = "src/runtime.rs"\n'
    )
    print(OUTPUT)


if __name__ == "__main__":
    main()
