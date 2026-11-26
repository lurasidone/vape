use crate::game::vape_current_support::GameExt;
use crate::game::vape_current_support::{MotionWorldExt, PlayerMotionExt};
// Live entity snapshots for combat modules and the AimAssist frame adapter.
// Snapshots are owned values for one frame: positions interpolated at the
// render partial tick, ids paired with the session epoch.
use crate::game::vape_rs::core::{ModuleId, lifecycle::EntityHandle, with_client};
use crate::game::vape_rs::inventory::item_limit::HeldItem;
use crate::game::vape_rs::modules::combat::aim_assist::Frame;
use crate::game::vape_rs::modules::combat::anti_fireball::{Aabb, Body, Fireball, Me, Tick};
use horizon_client::projectiles::ProjectileKind;
use crate::game::vape_rs::targeting::entities::{
    Bounds, EntityClass, EntityView, PlayerView, threat_score,
};
use crate::game::vape_rs::targeting::filter::People;
use crate::game::{Game, Screen};

fn lerp(previous: [f64; 3], current: [f64; 3], partial: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| previous[i] + (current[i] - previous[i]) * partial)
}

pub(super) fn item_score(game: &Game<'_>, item: Option<&str>, armor: bool) -> f64 {
    let Some(info) = item.and_then(|id| game.res.items().get(id)) else {
        return 0.0;
    };
    if !armor {
        return f64::from(info.attack_damage);
    }
    // Vanilla armor modifiers name the piece's slot group; see vape_transactions::score.
    info.modifiers
        .iter()
        .filter(|modifier| {
            modifier.slot == "armor"
                || modifier.slot == "any"
                || info.equip_slot.as_deref() == Some(modifier.slot.as_str())
        })
        .map(|modifier| match modifier.attribute.as_str() {
            "minecraft:armor" | "armor" => modifier.amount,
            "minecraft:armor_toughness" | "armor_toughness" => modifier.amount * 0.25,
            _ => 0.0,
        })
        .sum()
}

fn eye(game: &Game<'_>) -> Option<horizon_core::geometry::Vec3> {
    let p = game.player.as_ref()?;
    Some(horizon_core::geometry::Vec3::new(
        p.position.x,
        p.position.y + f64::from(p.dimensions.eye_height),
        p.position.z,
    ))
}

/// Living entities other than the local player: mobs (including remote
/// players) and, when hosting LAN, the guests. `sight` limits the
/// line-of-sight raycasts to entities within that distance.
pub(super) fn snapshot(
    game: &Game<'_>,
    epoch: crate::game::vape_rs::core::lifecycle::Epoch,
    sight: Option<f64>,
) -> Vec<EntityView> {
    let partial = game.clock.fraction();
    let names = game.remote_player_names();
    let eye = eye(game);
    let world = sight.map(|_| game.mob_world_base(&[]));
    let visible = |at: [f64; 3], eye_height: f64| -> bool {
        let (Some(world), Some(from), Some(range)) = (world.as_ref(), eye, sight) else {
            return true;
        };
        let to = horizon_core::geometry::Vec3::new(at[0], at[1] + eye_height, at[2]);
        let d =
            ((to.x - from.x).powi(2) + (to.y - from.y).powi(2) + (to.z - from.z).powi(2)).sqrt();
        d > range || world.line_of_sight(from, to)
    };
    let mut out = Vec::new();
    for mob in game.mobs.iter().filter(|m| m.alive()) {
        let class = EntityClass::classify(&mob.kind.id, &mob.kind.category);
        let current = [mob.position.x, mob.position.y, mob.position.z];
        let position = lerp(
            [mob.previous.x, mob.previous.y, mob.previous.z],
            current,
            partial,
        );
        let b = mob.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let e = &mob.extra;
        let worn = [
            e.head_id(),
            e.slot_id(horizon_items::equipment::CHEST),
            e.slot_id(horizon_items::equipment::LEGS),
            e.slot_id(horizon_items::equipment::FEET),
        ];
        let name = if class == EntityClass::Player {
            names
                .iter()
                .find(|(id, _)| *id == mob.id)
                .map(|(_, n)| n.clone())
                .unwrap_or_default()
        } else {
            mob.custom_name.clone().unwrap_or_default()
        };
        out.push(EntityView {
            handle: EntityHandle {
                epoch,
                id: mob.id as i32,
            },
            class,
            name,
            position,
            tick_position: current,
            bounds: Bounds {
                min: [lo.x, lo.y, lo.z],
                max: [hi.x, hi.y, hi.z],
            },
            health: mob.health,
            invisible: mob.invisible,
            holding: e.main_hand_id().is_some(),
            armored: worn.iter().any(Option::is_some),
            armor: worn.iter().map(|w| item_score(game, *w, true)).sum(),
            // A remote player's effects never reach a client; a mob's do.
            threat: threat_score(
                item_score(game, e.main_hand_id(), false),
                e.effects
                    .iter()
                    .find(|effect| effect.id.trim_start_matches("minecraft:") == "strength")
                    .map(|effect| (effect.duration, effect.amplifier)),
            ),
            visible: visible(current, mob.kind.eye_height),
            vehicle: Some(mob.id) == game.riding,
            blocking: e.using_item > 0
                && e.use_animation == horizon_core::use_animation::UseAnimation::Block,
            look: Some((mob.head_yaw, mob.head_pitch)),
            motion: [mob.velocity.x, mob.velocity.y, mob.velocity.z],
        });
    }
    if let Some(host) = &game.lan {
        for guest in host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead)
        {
            let worn: Vec<_> = guest
                .armor
                .iter()
                .map(|a| a.as_ref().map(|s| &*s.item))
                .collect();
            let held = guest.inventory.get(guest.selected).and_then(|s| s.as_ref());
            let min = [
                guest.position[0] - 0.3,
                guest.position[1],
                guest.position[2] - 0.3,
            ];
            out.push(EntityView {
                handle: EntityHandle {
                    epoch,
                    id: guest.entity_id,
                },
                class: EntityClass::Player,
                name: guest.name.clone(),
                position: lerp(guest.previous_position, guest.position, partial),
                tick_position: guest.position,
                bounds: Bounds {
                    min,
                    max: [min[0] + 0.6, min[1] + 1.8, min[2] + 0.6],
                },
                health: guest.health,
                invisible: false,
                holding: held.is_some(),
                armored: worn.iter().any(Option::is_some),
                armor: worn.iter().map(|w| item_score(game, *w, true)).sum(),
                threat: threat_score(
                    item_score(game, held.map(|s| &*s.item), false),
                    guest
                        .effects
                        .get("strength")
                        .map(|e| (e.duration, e.amplifier)),
                ),
                visible: visible(guest.position, 1.62),
                vehicle: false,
                // The guest's model carries its raised shield (lan_update_models).
                blocking: guest.living_flags & 1 != 0 && {
                    let held = if guest.living_flags & 2 != 0 {
                        guest.offhand.as_ref()
                    } else {
                        guest.inventory.get(guest.selected).and_then(Option::as_ref)
                    };
                    held.is_some_and(|s| s.item.trim_start_matches("minecraft:") == "shield")
                },
                look: Some((guest.head_yaw, guest.pitch)),
                // The host keeps no guest velocity; a remote player's motion
                // only follows velocity packets in the reference too.
                motion: [0.0; 3],
            });
        }
    }
    out
}

pub(super) fn held(game: &Game<'_>) -> HeldItem {
    item_at(game, game.selected)
}

/// The inventory slot's item as the item limits read it.
pub(super) fn item_at(game: &Game<'_>, slot: usize) -> HeldItem {
    item(
        game,
        game.inventory.get(slot).and_then(|s| s.as_ref()),
        slot,
    )
}

pub(super) fn item(
    game: &Game<'_>,
    stack: Option<&horizon_client::items::ItemStack>,
    slot: usize,
) -> HeldItem {
    let info = stack.and_then(|s| game.res.items().get(&s.item));
    HeldItem {
        id: stack.map(|s| s.item.trim_start_matches("minecraft:").to_owned()),
        display_name: stack
            .map(|s| horizon_client::stations::hover_name(&game.pack.names, s))
            .unwrap_or_default(),
        block: info.is_some_and(|i| i.block.is_some()),
        food: info.is_some_and(|i| i.food.is_some()),
        potion: stack.is_some_and(|s| s.item.ends_with("potion")),
        slot,
    }
}

fn playing(game: &Game<'_>) -> bool {
    game.focused
        && game.screen == Screen::Playing
        && game.title.is_none()
        && game.sleeping.is_none()
        && !game.dialog_open()
        && game.server_packs.prompt.is_none()
        && !game.spectator()
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health() > 0.0)
}

/// After the host's own mouse look and before rendering: AimAssist turns the
/// view through the same `Entity.turn` the mouse uses.
pub(super) fn aim_assist(game: &mut Game<'_>) {
    if game.serving.is_some() || game.options.dedicated.is_some() {
        return;
    }
    let client = crate::game::vape_rs::core::client().clone();
    let (active, epoch, walls, range) = with_client(&client, |runtime| {
        let walls = runtime
            .settings()
            .get("combat.aim-assist.targets.ignore-walls")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let range = runtime
            .settings()
            .get("combat.aim-assist.distance")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(8.0);
        (
            runtime.aim_assist_active(),
            runtime.session().epoch(),
            walls,
            range,
        )
    });
    let Some(player) = game.player.as_ref() else {
        return;
    };
    if !active || game.title.is_some() {
        return;
    }
    let partial = game.clock.fraction();
    let current = [player.position.x, player.position.y, player.position.z];
    let previous = [
        game.previous_position.x,
        game.previous_position.y,
        game.previous_position.z,
    ];
    let motion = [player.velocity.x, player.velocity.y, player.velocity.z];
    let (on_ground, eye_height) = (
        player.ground.on_ground,
        f64::from(player.dimensions.eye_height),
    );
    let strafe = f32::from(u8::from(game.keys.raw(&winit::keyboard::KeyCode::KeyA)))
        - f32::from(u8::from(game.keys.raw(&winit::keyboard::KeyCode::KeyD)));
    // The crosshair is on a block when the block hit is nearer than any mob.
    let aiming_block = game.pick().is_some_and(|hit| {
        let eye = game.eye_position().unwrap_or_default();
        let d = (0..3)
            .map(|i| (hit.point[i] - eye[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        game.mob_hit().is_none_or(|(_, m, _)| d < m)
    });
    let frame = Frame {
        player: PlayerView {
            position: lerp(previous, current, partial),
            tick_position: current,
            yaw: game.yaw,
            pitch: game.pitch,
            strafe,
        },
        entities: snapshot(game, epoch, walls.then_some(range + 1.0)),
        held: held(game),
        attack_down: game.left,
        aiming_block,
        playing: playing(game),
        movement_locked: with_client(&client, |runtime| runtime.movement_input_locked()),
        sensitivity: game.settings.sensitivity,
        elapsed_ms: 0,
        motion,
        on_ground,
        eye_height,
    };
    let (x, y) = with_client(&client, |runtime| {
        if runtime.enabled(ModuleId::AimAssist) {
            runtime.aim_assist_frame(frame)
        } else {
            (0.0, 0.0)
        }
    });
    if x == 0.0 && y == 0.0 {
        return;
    }
    let mut rotation = horizon_client::mouse::Rotation {
        x_rot: game.pitch,
        y_rot: game.yaw,
        x_rot_o: game.pitch,
        y_rot_o: game.yaw,
    };
    rotation.turn(x, y);
    game.yaw = rotation.y_rot;
    game.pitch = rotation.x_rot;
}

/// The entity under the crosshair before any block: a mob (remote players
/// included) or, when hosting LAN, a guest. `miss` is true when the view hits
/// neither an entity nor a block.
fn crosshair(game: &Game<'_>, entities: &[EntityView]) -> (Option<EntityView>, bool) {
    let mob = game
        .mob_hit()
        .filter(|(_, d, _)| game.before_block(*d))
        .map(|(i, d, _)| (game.mobs[i].id as i32, d));
    let guest = game
        .guest_hit()
        .filter(|(_, d)| game.before_block(*d))
        .and_then(|(conn, d)| {
            let host = game.lan.as_ref()?;
            host.hub
                .guests
                .iter()
                .find(|g| g.conn == conn)
                .map(|g| (g.entity_id, d))
        });
    let best = match (mob, guest) {
        (Some(m), Some(g)) => Some(if g.1 < m.1 { g } else { m }),
        (m, g) => m.or(g),
    };
    let entity = best.and_then(|(id, _)| entities.iter().find(|e| e.handle.id == id).cloned());
    let miss = entity.is_none() && game.pick().is_none();
    (entity, miss)
}

pub(super) fn trigger_tick(
    game: &Game<'_>,
    now_ms: u64,
    epoch: crate::game::vape_rs::core::lifecycle::Epoch,
    walls: bool,
) -> crate::game::vape_rs::modules::combat::trigger_bot::Tick {
    use crate::game::vape_rs::modules::combat::trigger_bot::{Cooldown, Tick};
    let entities = snapshot(game, epoch, walls.then_some(6.0));
    let (crosshair, crosshair_miss) = crosshair(game, &entities);
    let player = game.player.as_ref();
    let (cooldown_bypass, shield_exempt) =
        crate::game::vape_modern_combat::trigger_assist(game, epoch, now_ms);
    Tick {
        now_ms,
        hurt_time: player.map_or(0, |p| p.health.hurt_animation_time),
        attack_down: game.left,
        playing: playing(game),
        held: held(game),
        on_ground: player.is_some_and(|p| p.ground.on_ground),
        vertical_motion: player.map_or(0.0, |p| p.position.y - game.previous_position.y),
        crosshair,
        crosshair_miss,
        // The client's LivingEntity.attackStrengthTicker, which vanilla's
        // getCooledAttackStrength reads (the integrated server keeps its own).
        cooldown: Cooldown {
            ticks: game.hand_strength.ticker as f32,
            delay: horizon_client::attack_strength::delay(game.main_hand_attack_speed()),
        },
        cooldown_bypass,
        shield_exempt,
    }
}

/// Client tick HEAD: one TriggerBot step and its synthetic attack click.
pub(super) fn trigger_bot(game: &mut Game<'_>) {
    let client = crate::game::vape_rs::core::client().clone();
    let (active, epoch, now, walls) = with_client(&client, |runtime| {
        let walls = runtime
            .settings()
            .get("combat.trigger-bot.targets.ignore-walls")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        (
            runtime.trigger_bot_active(),
            runtime.session().epoch(),
            runtime.now_ms(),
            walls,
        )
    });
    if !active || game.player.is_none() || game.title.is_some() {
        return;
    }
    let tick = trigger_tick(game, now, epoch, walls);
    let effects = with_client(&client, |runtime| runtime.trigger_bot_tick(&tick));
    if effects.attack && tick.playing {
        use crate::game::vape_rs::modules::combat::weapon_swap::Source;
        if crate::game::vape_modern_combat::request(game, Source::TriggerBot) {
            crate::game::remote::vape_macros::attack_once(game);
        } else {
            with_client(&client, |runtime| runtime.trigger_bot_attack_cancelled());
        }
    }
}

/// The physical attack button changed. True swallows the activation click.
pub(super) fn trigger_bot_input(game: &Game<'_>, pressed: bool) -> bool {
    let client = crate::game::vape_rs::core::client().clone();
    let (enabled, epoch, now) = with_client(&client, |runtime| {
        (
            runtime.enabled(ModuleId::TriggerBot),
            runtime.session().epoch(),
            runtime.now_ms(),
        )
    });
    if !enabled || game.player.is_none() {
        return false;
    }
    let tick = trigger_tick(game, now, epoch, false);
    with_client(&client, |runtime| runtime.trigger_bot_input(&tick, pressed))
}

/// `RotationUtil.H/F` for Velocity: other non-friend players within 6 blocks
/// looking at us, and us looking at one of them (integer yaw under 60).
pub(crate) fn facing(
    game: &Game<'_>,
    friends: &[String],
) -> crate::game::vape_rs::modules::combat::velocity::Facing {
    use crate::game::vape_rs::targeting::entities::{distance, int_yaw_difference};
    let mut out = crate::game::vape_rs::modules::combat::velocity::Facing::default();
    let Some(player) = game.player.as_ref() else {
        return out;
    };
    let me = [player.position.x, player.position.y, player.position.z];
    let me_view = PlayerView {
        position: me,
        tick_position: me,
        yaw: game.yaw,
        pitch: game.pitch,
        strafe: 0.0,
    };
    let names = game.remote_player_names();
    for mob in game
        .mobs
        .iter()
        .filter(|m| m.alive() && &*m.kind.id == "minecraft:player")
    {
        let name = names
            .iter()
            .find(|(id, _)| *id == mob.id)
            .map_or("", |(_, n)| n.as_str());
        if friends.iter().any(|f| f.eq_ignore_ascii_case(name)) {
            continue;
        }
        let at = [mob.position.x, mob.position.y, mob.position.z];
        if distance(me, at) >= 6.0 {
            continue;
        }
        let them = PlayerView {
            position: at,
            tick_position: at,
            yaw: mob.yaw,
            pitch: 0.0,
            strafe: 0.0,
        };
        out.player_facing_target |= int_yaw_difference(&them, me) < 60;
        out.target_facing_player |= int_yaw_difference(&me_view, at) < 60;
    }
    out
}

/// A Velocity replacement for the local player's last motion packet. The
/// inbound hook only observes (the host's packet handler holds macros), so
/// the replacement lands right after the host's own assignment: at the next
/// packet or tick, and only if the motion is still exactly what the packet set.
fn apply_motion_override(game: &mut Game<'_>) {
    let client = crate::game::vape_rs::core::client().clone();
    let Some((replacement, received)) =
        with_client(&client, |runtime| runtime.take_motion_override())
    else {
        return;
    };
    if let Some(player) = game.player.as_mut()
        && [player.velocity.x, player.velocity.y, player.velocity.z] == received
    {
        player.velocity =
            horizon_core::geometry::Vec3::new(replacement[0], replacement[1], replacement[2]);
    }
}

/// Inbound packet boundary (`handle_packet` HEAD, observe-only). Velocity
/// decides on the local player's motion packets here.
pub(super) fn inbound(game: &mut Game<'_>, packet: &horizon_protocol::packets::play::Clientbound) {
    use crate::game::vape_rs::modules::combat::velocity::Decision;
    use horizon_protocol::packets::play::Clientbound as C;
    apply_motion_override(game);
    if let C::PlayerPosition(_) = packet {
        // AntiFall's onPacketReceive (S08): the lagback landed.
        let client = crate::game::vape_rs::core::client().clone();
        if with_client(&client, |runtime| {
            runtime.server_teleport();
            runtime.enabled(ModuleId::AntiFall)
        }) && let Some(player) = game.player.as_mut()
        {
            player.fall_distance = 0.0;
            player.velocity.x = 0.0;
            player.velocity.z = 0.0;
        }
        return;
    }
    let C::SetEntityMotion(p) = packet else {
        return;
    };
    if !game.remote.as_ref().is_some_and(|r| r.entity_id == p.id) {
        return;
    }
    let client = crate::game::vape_rs::core::client().clone();
    with_client(&client, |runtime| runtime.no_slowdown_knockback());
    let (active, friends) = with_client(&client, |runtime| {
        (runtime.velocity_active(), runtime.friend_keys())
    });
    if !active || game.player.is_none() {
        return;
    }
    let facing = facing(game, &friends);
    let water = game
        .player
        .as_ref()
        .is_some_and(|pl| pl.fluids.was_touching_water);
    with_client(&client, |runtime| {
        if let Decision::Replace(m) = runtime.velocity_motion(facing, water, p.velocity) {
            runtime.set_motion_override(m, p.velocity);
        }
    });
}

/// `remote_explode` HEAD (cancellable): Velocity scales the explosion push by
/// running the host's handler on a copy with the replaced push.
pub(super) fn explosion(
    game: &mut Game<'_>,
    p: &horizon_protocol::packets::play::Explode,
) -> core::ops::ControlFlow<()> {
    use crate::game::vape_rs::modules::combat::velocity::Decision;
    use core::ops::ControlFlow;
    let Some(push) = p.knockback else {
        return ControlFlow::Continue(());
    };
    let client = crate::game::vape_rs::core::client().clone();
    let (active, reentry, friends) = with_client(&client, |runtime| {
        if !runtime.explosion_reentry_pending() {
            runtime.no_slowdown_knockback();
        }
        (
            runtime.velocity_active(),
            runtime.take_explosion_reentry(),
            runtime.friend_keys(),
        )
    });
    if reentry || !active || game.player.is_none() {
        return ControlFlow::Continue(());
    }
    let facing = facing(game, &friends);
    let water = game
        .player
        .as_ref()
        .is_some_and(|pl| pl.fluids.was_touching_water);
    let decision = with_client(&client, |runtime| {
        runtime.velocity_explosion(facing, water, push)
    });
    let Decision::Replace(m) = decision else {
        return ControlFlow::Continue(());
    };
    let mut changed = p.clone();
    changed.knockback = Some(m);
    with_client(&client, |runtime| runtime.set_explosion_reentry());
    game.remote_explode(&changed);
    ControlFlow::Break(())
}

/// Client tick HEAD: Velocity's delayed scaling of the current motion.
pub(super) fn velocity_tick(game: &mut Game<'_>) {
    apply_motion_override(game);
    let client = crate::game::vape_rs::core::client().clone();
    let Some(player) = game.player.as_ref() else {
        return;
    };
    let water = player.fluids.was_touching_water;
    let v = [player.velocity.x, player.velocity.y, player.velocity.z];
    if let Some(next) = with_client(&client, |runtime| runtime.velocity_tick(water, v))
        && let Some(player) = game.player.as_mut()
    {
        player.velocity = horizon_core::geometry::Vec3::new(next[0], next[1], next[2]);
    }
}

/// `isReachAllowed`'s local player state.
fn reach_player(game: &Game<'_>) -> crate::game::vape_rs::modules::combat::reach::Player {
    let player = game.player.as_ref();
    crate::game::vape_rs::modules::combat::reach::Player {
        sprinting: player.is_some_and(|p| p.sprinting),
        in_liquid: player.is_some_and(|p| p.fluids.was_touching_water || p.is_in_lava()),
    }
}

/// The host's `mob_hit` with another range and every box grown by `expand`
/// (the entity's pick border plus HitBoxes' expansion).
fn mob_hit_with(game: &Game<'_>, reach: f64, expand: f64) -> Option<(usize, f64, [f64; 3])> {
    let (eye, dir) = game.aim()?;
    let grow = horizon_core::geometry::Vec3::new(expand, expand, expand);
    let mut best: Option<(usize, f64, [f64; 3])> = None;
    for (i, m) in game.mobs.iter().enumerate() {
        if !m.alive() || Some(m.id) == game.riding {
            continue;
        }
        let bounds = if expand > 0.0 {
            m.bounds().inflate(grow)
        } else {
            m.bounds()
        };
        if let Some(t) = super::ray_box(eye, dir, reach, bounds)
            && best.is_none_or(|b| t < b.1)
        {
            best = Some((
                i,
                t,
                [
                    eye[0] + dir[0] * t,
                    eye[1] + dir[1] * t,
                    eye[2] + dir[2] * t,
                ],
            ));
        }
    }
    best
}

/// `mob_hit` HEAD (cancellable): Reach's range and HitBoxes' expansion for
/// every entity pick (crosshair, attacks, interactions). Creative keeps the
/// host's own 5-block reach.
pub(super) fn pick(game: &Game<'_>) -> core::ops::ControlFlow<Option<(usize, f64, [f64; 3])>> {
    use core::ops::ControlFlow;
    if game.serving.is_some() || game.options.dedicated.is_some() || game.creative {
        return ControlFlow::Continue(());
    }
    let client = crate::game::vape_rs::core::client().clone();
    if crate::game::vape_rs::core::try_with_client(&client, |runtime| {
        runtime.bed_breaker_target()
    })
    .flatten()
    .is_some()
    {
        return ControlFlow::Break(None);
    }
    let player = reach_player(game);
    match crate::game::vape_rs::core::try_with_client(&client, |runtime| runtime.pick_range(player))
        .flatten()
    {
        Some((reach, expand)) => ControlFlow::Break(mob_hit_with(game, reach, expand)),
        None => ControlFlow::Continue(()),
    }
}

/// Client tick HEAD: Reach's Advanced-mode target trace at the maximum range.
pub(super) fn reach_tick(game: &Game<'_>) {
    let client = crate::game::vape_rs::core::client().clone();
    let max = with_client(&client, |runtime| {
        runtime.enabled(ModuleId::Reach).then(|| {
            crate::game::vape_rs::modules::combat::reach::Options::read(runtime.settings())
                .range
                .1
        })
    });
    let traced = max
        .and_then(|max| mob_hit_with(game, max, 0.0))
        .map(|(i, _, _)| game.mobs[i].id as i32);
    let player = reach_player(game);
    with_client(&client, |runtime| runtime.reach_tick(player, traced));
}

/// An attack started (`EventPostAttack`).
pub(super) fn post_attack(game: &mut Game<'_>) {
    let client = crate::game::vape_rs::core::client().clone();
    with_client(&client, |runtime| runtime.reach_post_attack());
    // WTap's EventPreAttack: the crosshair mob this attack will hit.
    let names = game.remote_player_names();
    let target = game
        .mob_hit()
        .filter(|(_, d, _)| game.before_block(*d))
        .and_then(|(i, _, _)| game.mobs.get(i))
        .map(|mob| (names.iter().any(|(id, _)| *id == mob.id), mob.hurt_time));
    if let Some((player, hurt_time)) = target
        && with_client(&client, |runtime| {
            runtime.wtap_attack(player, hurt_time);
            runtime.enabled(ModuleId::WTap)
        })
    {
        crate::game::vape_input_delays::movement_again(game);
    }
}

/// Frame HEAD: AutoClicker and RightClicker through the host's attack/use
/// mappings (`setKeyBindState` + `KeyMapping.click`).
pub(super) fn clickers(game: &mut Game<'_>) {
    use crate::game::vape_rs::modules::combat::clicker::Input;
    if game.serving.is_some() || game.options.dedicated.is_some() {
        return;
    }
    let client = crate::game::vape_rs::core::client().clone();
    let (now, left_on, right_on) = with_client(&client, |runtime| {
        (
            runtime.now_ms(),
            runtime.enabled(ModuleId::AutoClicker),
            runtime.enabled(ModuleId::RightClicker),
        )
    });
    let playing = playing(game) && game.player.is_some();
    let held = held(game);
    // Trigger mode and Break blocks read the crosshair.
    let entity_hit = left_on && game.mob_hit().is_some_and(|(_, d, _)| game.before_block(d));
    let block_hit = left_on && !entity_hit && game.pick().is_some();
    let sneaking = game.keys.raw(&winit::keyboard::KeyCode::ShiftLeft);
    for (right, on) in [(false, left_on), (true, right_on)] {
        let input = Input {
            now_ms: now,
            playing,
            attack_held: on && game.toggles.is_down(if right { 3 } else { 2 }),
            attack_mapped: if right { game.right } else { game.left },
            held: held.clone(),
            entity_hit,
            block_hit,
            sneaking,
            hit_select: !right && entity_hit && crate::game::vape_hit_select::defers(game),
            use_claimed: false,
        };
        let actions = with_client(&client, |runtime| runtime.clicker_frame(right, &input));
        if actions.release {
            if right {
                game.right = false
            } else {
                game.left = false;
                crate::game::vape_block_hit::clicker_release(game);
            }
        }
        if actions.press {
            if right {
                crate::game::remote::vape_macros::use_once(game);
                game.right = true;
            } else if crate::game::vape_modern_combat::request(
                game,
                crate::game::vape_rs::modules::combat::weapon_swap::Source::AutoClicker,
            ) {
                crate::game::remote::vape_macros::attack_once(game);
                game.left = true;
                crate::game::vape_block_hit::clicker_press(game);
            }
        }
        if actions.turn != (0, 0) {
            let mut rotation = horizon_client::mouse::Rotation {
                x_rot: game.pitch,
                y_rot: game.yaw,
                x_rot_o: game.pitch,
                y_rot_o: game.yaw,
            };
            rotation.turn(f64::from(actions.turn.0), f64::from(actions.turn.1));
            game.yaw = rotation.y_rot;
            game.pitch = rotation.x_rot;
        }
    }
}

fn vec3(v: horizon_core::geometry::Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

fn body_aabb(bounds: horizon_core::geometry::Aabb) -> Aabb {
    let (min, max) = (bounds.min_position(), bounds.max_position());
    Aabb {
        min: [min.x, min.y, min.z],
        max: [max.x, max.y, max.z],
    }
}

/// The redirectable projectile under the view ray, unless a block is nearer.
fn crosshair_projectile(game: &Game<'_>, reach: f64) -> Option<i32> {
    let (eye, dir) = game.aim()?;
    let block = game.pick().map(|hit| {
        let d = [hit.point[0] - eye[0], hit.point[1] - eye[1], hit.point[2] - eye[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    });
    // The reference's extended entity ray also stops at a nearer living body.
    let mob = game.mob_hit_within(reach).map(|(_, distance, _)| distance);
    let mut best: Option<(i32, f64)> = None;
    for projectile in &game.creatures.projectiles {
        if !matches!(
            projectile.kind,
            ProjectileKind::LargeFireball { .. }
                | ProjectileKind::SmallFireball
                | ProjectileKind::DragonFireball
                | ProjectileKind::WindCharge
                | ProjectileKind::BreezeWindCharge
        ) {
            continue;
        }
        if let Some(t) = crate::game::ray_box(eye, dir, reach, projectile.bounds())
            && block.is_none_or(|b| t < b)
            && mob.is_none_or(|m| t < m)
            && best.is_none_or(|(_, bt)| t < bt)
        {
            best = Some((projectile.id as i32, t));
        }
    }
    best.map(|(id, _)| id)
}

/// Client tick HEAD: AntiFireball's rotation and attack, before the movement
/// marks and the motion packet.
fn anti_fireball(game: &mut Game<'_>) {
    let client = crate::game::vape_rs::core::client().clone();
    if !with_client(&client, |runtime| runtime.anti_fireball_wanted()) {
        return;
    }
    let Some(player) = game.player.as_ref() else {
        return;
    };
    let epoch = with_client(&client, |runtime| runtime.session().epoch());
    let people = with_client(&client, |runtime| runtime.target_people());
    let me = Me {
        id: player.entity_id,
        position: vec3(player.position),
        previous: vec3(game.previous_position),
        eye_height: f64::from(player.dimensions.eye_height),
        bounds: body_aabb(player.bounds()),
        yaw: game.yaw,
        pitch: game.pitch,
    };
    let bodies = snapshot(game, epoch, None)
        .into_iter()
        .map(|entity| {
            let player = entity.class == EntityClass::Player;
            let valid_owner =
                !player || (!people.friend(&entity.name) && !people.teammate(&entity));
            Body {
                id: entity.handle.id,
                position: entity.position,
                bounds: Aabb {
                    min: entity.bounds.min,
                    max: entity.bounds.max,
                },
                player,
                valid_owner,
            }
        })
        .collect();
    let fireballs = game
        .creatures
        .projectiles
        .iter()
        .filter_map(|projectile| {
            if !matches!(projectile.kind, ProjectileKind::LargeFireball { .. }) {
                return None;
            }
            let size = projectile.kind.size();
            Some(Fireball {
                id: projectile.id as i32,
                position: vec3(projectile.position),
                previous: vec3(projectile.previous),
                velocity: vec3(projectile.velocity),
                acceleration: projectile.acceleration,
                age: projectile.age,
                width: size,
                height: size,
            })
        })
        .collect();
    let reach = with_client(&client, |runtime| {
        runtime
            .pick_range(reach_player(game))
            .map(|(reach, _)| reach)
            .unwrap_or(game.entity_reach)
    });
    let crosshair = crosshair_projectile(game, reach);
    let mut tick = Tick {
        me,
        bodies,
        fireballs,
        focused: game.focused,
        blocked_by_screen: game.screen != Screen::Playing,
        screen_open: game.screen != Screen::Playing,
        attack_down: game.left,
        crosshair,
        sensitivity: f64::from(game.settings.sensitivity),
        managed: None,
        managed_returning: false,
    };
    let effects = {
        let mut clip = |from: [f64; 3], to: [f64; 3]| {
            let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
            let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if length <= 1.0e-9 {
                return None;
            }
            game.clip_ray(from, [d[0] / length, d[1] / length, d[2] / length], length)
                .map(|hit| hit.point)
        };
        with_client(&client, |runtime| {
            runtime.anti_fireball_tick(&mut tick, &mut clip)
        })
    };
    if let Some((yaw, pitch)) = effects.view {
        game.yaw = yaw;
        game.pitch = pitch;
    }
    if let Some(sent) = effects.sent {
        crate::game::remote::vape_combat_remote::set_rotation(sent);
    }
    if effects.release_attack {
        game.left = false;
    }
    if effects.press_attack {
        game.left = true;
    }
}

/// Client tick HEAD: the clickers' jitter step, Fly's (else LongJump's or
/// Speed's) move, AntiFall's lagback and NoSlowdown's mark for this tick's
/// player move.
pub(super) fn movement_tick(game: &mut Game<'_>) {
    use crate::game::vape_rs::modules::blatant::{anti_fall, fly, no_slowdown, speed};
    use winit::keyboard::KeyCode;
    let client = crate::game::vape_rs::core::client().clone();
    with_client(&client, |runtime| runtime.clicker_tick());
    if game.player.is_none() {
        return;
    }
    anti_fireball(game);
    let active = playing(game);
    let key = |k: KeyCode| game.keys.raw(&k);
    let axis = |plus: KeyCode, minus: KeyCode| {
        f64::from(u8::from(key(plus))) - f64::from(u8::from(key(minus)))
    };
    let (forward, strafe) = if active {
        (
            axis(KeyCode::KeyW, KeyCode::KeyS),
            axis(KeyCode::KeyA, KeyCode::KeyD),
        )
    } else {
        (0.0, 0.0)
    };
    let effect = |id: &str| {
        game.potions
            .effects
            .list
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.amplifier as i32)
    };
    let speed_amplifier = effect("speed");
    let jump_amplifier = effect("jump_boost");
    let sneaking = active && (key(KeyCode::ShiftLeft) || game.toggles.is_down(0));
    let input = fly::Input {
        forward,
        strafe,
        yaw: game.yaw,
        jump: active && key(KeyCode::Space),
        sneak: active && key(KeyCode::ShiftLeft),
        speed_amplifier,
    };
    let yaw = game.yaw;
    let (blocked_vertically, (on_vine, on_ice)) = {
        let entities = game.player_entity_collisions();
        let player = game.player.as_mut().unwrap();
        let bounds = player.bounds().move_by(horizon_core::geometry::Vec3::new(
            0.0,
            player.velocity.y,
            0.0,
        ));
        let mut motion = horizon_client::motion::MotionWorld {
            entities: &entities,
            player,
            scene: &mut game.world,
            assets: &game.pack.assets,
            collision_binding: game.collision,
            resources: game.motion,
            health: &game.health_resources,
            tag_items: Some(game.res.items()),
            loop_context: &game.loop_context,
            damage_hooks: &game.damage_hooks,
            block_registries: Some(&game.pack.block_registries),
        };
        (
            !motion.no_player_collision(bounds).unwrap_or(true),
            motion.vape_rs_surface(),
        )
    };
    let player = game.player.as_ref().unwrap();
    let tick = no_slowdown::Tick {
        on_ground: player.ground.on_ground,
        in_water: player.fluids.was_touching_water,
        slowed: game.using.is_some(),
        held: held(game),
    };
    let speed_tick = speed::Tick {
        forward,
        strafe,
        yaw,
        on_ground: player.ground.on_ground,
        collided_vertically: player.collision.vertical,
        blocked_vertically,
        in_liquid: player.fluids.was_touching_water || player.is_in_lava(),
        sneaking,
        food: player.health.food.food,
        speed_amplifier,
        jump_amplifier,
        position: [player.position.x, player.position.z],
        on_vine,
        on_ice,
    };
    // AntiFall's column scan: any non-air block below the feet.
    let (x, z) = (
        player.position.x.floor() as i32,
        player.position.z.floor() as i32,
    );
    let feet = (player.position.y - 1.0).floor() as i32;
    let air = game.world.air;
    let over_air = game.world.block(x, feet, z) == air;
    let block_below = (horizon_client_render::chunk::MIN_Y..=feet)
        .rev()
        .any(|y| game.world.block(x, y, z) != air);
    let fall_tick = anti_fall::Tick {
        now_ms: 0,
        skip: player.health.health() <= 0.0 || game.creative || player.flying || game.spectator(),
        block_below,
        over_air,
        on_ground: player.ground.on_ground,
        fall_distance: player.fall_distance,
    };
    let (fly_move, speed_move, lagback, no_slow) = with_client(&client, |runtime| {
        let fly_move = runtime.fly_move(&input);
        let speed_move = if fly_move.is_none() {
            runtime.speed_move(&speed_tick)
        } else {
            Default::default()
        };
        let lagback = runtime.anti_fall_motion(&anti_fall::Tick {
            now_ms: runtime.now_ms(),
            ..fall_tick
        });
        (
            fly_move,
            speed_move,
            lagback,
            runtime.no_slowdown_tick(&tick),
        )
    });
    let Some(player) = game.player.as_mut() else {
        return;
    };
    if let Some(height) = lagback {
        // `setPosition(x, y + fallDistance, z)`, horizontal motion, input and
        // fall distance zeroed.
        player.position.y += height;
        player.velocity.x = 0.0;
        player.velocity.z = 0.0;
        player.fall_distance = 0.0;
    }
    let replacement = match fly_move {
        Some(m) => {
            // `player.motionY = verticalMotion` alongside the replaced move.
            player.velocity.y = m[1];
            Some(m)
        }
        None if speed_move != speed::Move::default() => {
            if let Some(y) = speed_move.y {
                player.velocity.y = y;
            }
            let [mx, mz] = speed_move.horizontal.unwrap_or([f64::NAN; 2]);
            Some([mx, speed_move.y.unwrap_or(f64::NAN), mz])
        }
        None => None,
    };
    player.vape_rs_set_fly_move(replacement);
    player.vape_rs_set_no_slowdown(no_slow);
    let keep_sprint = no_slow && with_client(&client, |runtime| runtime.enabled(ModuleId::Sprint));
    player.vape_rs_set_no_slowdown_sprint(keep_sprint);
}
