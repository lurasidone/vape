use crate::game::vape_current_support::GameExt;
use crate::game::vape_current_support::PlayerMotionExt;
// Host side of the movement/world modules (the shared rotation manager,
// Scaffold and its bridge modes, InvWalk, WindCharge, NoFall's jump release). It sits under game/remote.rs so the
// remote use path, the open menu id and the physical keys are reachable.
use crate::game::Game;
use crate::game::vape_rs::core::movement_input::{Axis, HostView, Owner, Request};
use crate::game::vape_rs::core::movement_runtime::BridgeTick;
use crate::game::vape_rs::core::{ModuleId, with_client};
use crate::game::vape_rs::modules::movement::movement_task::{Body, World};
use crate::game::vape_rs::modules::movement::scaffold_bridge::{Hit, Inventory, Stack};
use crate::game::vape_rs::modules::movement::{inv_walk, scaffold, wind_charge};
use crate::game::vape_rs::modules::combat::crystal_aura;
use crate::game::vape_rs::modules::combat::rotation;
use crate::game::vape_rs::modules::utility::auto_anchor;
use crate::game::vape_rs::modules::world::block_in;
use horizon_core::geometry::Vec3;
use winit::keyboard::KeyCode;

thread_local! {
    /// WindCharge's jump for the writer this tick.
    static JUMP: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    /// AutoTotem's `suppressInput` this tick.
    static TOTEM_HOLD: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    /// BedBreaker's scheduled action: the reference scans every 50 ticks.
    static BED_SCAN_TICKS: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
    /// AutoAnchor's On place right-click edge.
    static AUTO_ANCHOR_RIGHT: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    /// Block-In holds the sneak key while it places.
    static BLOCK_IN_SNEAK: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Keys physically held, through the key mapping (`KeyMapping.isDown` of
/// the physical state, whatever screen is open).
fn physical(game: &Game<'_>, key: KeyCode) -> bool {
    game.physical_keys.iter().any(|&held| {
        game.settings
            .keys
            .translate_all(held)
            .into_iter()
            .any(|m| m == Ok(key))
    })
}

/// Scaffold Legit's edge test: the box shrunk by 0.2 horizontally, moved by
/// this tick's horizontal motion and one block down, touches nothing.
fn scaffold_edge(game: &mut Game<'_>) -> bool {
    let entities = game.player_entity_collisions();
    let Some(player) = game.player.as_mut() else {
        return false;
    };
    let (vx, vz) = (player.velocity.x, player.velocity.z);
    let bounds = player
        .bounds()
        .deflate(horizon_core::geometry::Vec3::new(0.2, 0.0, 0.2))
        .move_by(horizon_core::geometry::Vec3::new(vx, -1.0, vz));
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
    motion.no_player_collision(bounds).unwrap_or(false)
}

/// The bridge modes' world queries.
struct HostWorld<'g, 'a>(&'g mut Game<'a>);

impl World for HostWorld<'_, '_> {
    fn air(&mut self, pos: [i32; 3]) -> bool {
        let name = &self
            .0
            .res
            .facts()
            .get(self.0.world.block(pos[0], pos[1], pos[2]))
            .name;
        matches!(
            &**name,
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        )
    }
    fn clear_below(&mut self, at: [f64; 2], shrink: f64) -> bool {
        let game = &mut *self.0;
        let entities = game.player_entity_collisions();
        let Some(player) = game.player.as_mut() else {
            return false;
        };
        let (vx, vz) = (player.velocity.x, player.velocity.z);
        let bounds = player
            .bounds()
            .move_by(Vec3::new(
                at[0] - player.position.x,
                0.0,
                at[1] - player.position.z,
            ))
            .deflate(Vec3::new(shrink, 0.0, shrink))
            .move_by(Vec3::new(vx, -1.0, vz));
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
        motion.no_player_collision(bounds).unwrap_or(false)
    }
}

/// The local player for the bridge modes and the movement task.
fn body(game: &mut Game<'_>) -> Option<Body> {
    let on_ladder = on_ladder(game);
    let previous = game.previous_position;
    let player = game.player.as_ref()?;
    Some(Body {
        pos: [player.position.x, player.position.y, player.position.z],
        prev: [previous.x, previous.y, previous.z],
        motion: [player.velocity.x, player.velocity.y, player.velocity.z],
        yaw: game.yaw,
        pitch: game.pitch,
        on_ground: player.ground.on_ground,
        sneaking: player.shift_key_down || player.pose == horizon_client::pose::Pose::Crouching,
        sprinting: player.sprinting,
        flying: player.flying,
        eye_height: f64::from(player.dimensions.eye_height),
        on_ladder,
    })
}

/// The inventory as the bridge modes read it.
fn inventory(game: &Game<'_>) -> Inventory {
    let stack = |s: Option<&horizon_client::items::ItemStack>, slot| {
        s.map(|s| Stack {
            item: crate::game::vape_combat::item(game, Some(s), slot),
            count: s.count,
        })
    };
    Inventory {
        slots: (0..36)
            .map(|slot| stack(game.inventory.get(slot).and_then(Option::as_ref), slot))
            .collect(),
        offhand: stack(game.offhand.as_ref(), 40),
        selected: game.selected,
        creative: game.creative,
    }
}

/// `normalReachRayTrace` (`rayTraceUsingManagedRotation(3.0, 0, false)`, the
/// 1.20.6+ mouse-over of RayTraceUtil), as `RotationManager.onTick` leaves
/// it once it has turned the view, before the player moves. The block along
/// the block reach the host's own pick uses (4.5, 5 in creative); the 3.0
/// only bounds entities.
/// An entity on the ray before the block leaves no block hit (an entity hit
/// within 3, a miss past it). LAN guests count within the host's guest pick.
fn crosshair_block(game: &Game<'_>) -> Option<Hit> {
    let (eye, dir) = game.aim()?;
    let client = crate::game::vape_rs::core::client().clone();
    if let Some(target) = crate::game::vape_rs::core::try_with_client(&client, |runtime| {
        runtime.bed_breaker_target()
    })
    .flatten()
    {
        let center = [
            f64::from(target.block[0]) + 0.5,
            f64::from(target.block[1]) + 0.5,
            f64::from(target.block[2]) + 0.5,
        ];
        let distance = (0..3)
            .map(|i| (center[i] - eye[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        if distance < 4.5 {
            return Some(Hit {
                pos: target.block,
                side: 1,
            });
        }
    }
    let reach = game.reach();
    let hit = game.clip_ray(eye, dir, reach)?;
    let distance = (0..3)
        .map(|i| (hit.point[i] - eye[i]).powi(2))
        .sum::<f64>()
        .sqrt();
    let entity = game
        .mobs
        .iter()
        .filter(|m| m.alive() && Some(m.id) != game.riding)
        .filter_map(|m| crate::game::ray_box(eye, dir, reach, m.bounds()))
        .chain(game.guest_hit().map(|(_, d)| d))
        .any(|d| d < distance);
    if entity {
        return None;
    }
    let side = match hit.face {
        [0, -1, 0] => 0,
        [0, 1, 0] => 1,
        [0, 0, -1] => 2,
        [0, 0, 1] => 3,
        [-1, 0, 0] => 4,
        _ => 5,
    };
    Some(Hit { pos: hit.pos, side })
}

/// `isOnLadder`: the player is in a climbable block (`LivingEntity.onClimbable`).
fn on_ladder(game: &mut Game<'_>) -> bool {
    let entities = game.player_entity_collisions();
    let Some(player) = game.player.as_mut() else {
        return false;
    };
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
    motion.climbable().unwrap_or(false)
}

/// The movement writer's requests from these modules.
pub(crate) fn requests(
    game: &mut Game<'_>,
    active: bool,
    again: bool,
    view: &HostView,
) -> Vec<Request> {
    let client = crate::game::vape_rs::core::client().clone();
    let mut out = Vec::new();
    // NoFall Normal released jump on a claimed landing; it stays released
    // until the user lets go of jump.
    if with_client(&client, |runtime| {
        let user = runtime.movement().user(Axis::Jump, view);
        runtime.no_fall_jump_released(user)
    }) {
        out.push(Request {
            axis: Axis::Jump,
            owner: Owner::NoFall,
            priority: 3,
            pressed: false,
        });
    }
    // AutoTotem's blockMovementKeys (EventPrePlayerTick, HIGHEST): forward,
    // back, left, right, jump and sprint released; sneak stays.
    if TOTEM_HOLD.with(|h| h.get()) {
        for axis in [
            Axis::Forward,
            Axis::Back,
            Axis::Left,
            Axis::Right,
            Axis::Jump,
            Axis::Sprint,
        ] {
            out.push(Request {
                axis,
                owner: Owner::AutoTotem,
                priority: 5,
                pressed: false,
            });
        }
    }
    if !again && JUMP.with(|j| j.take()) {
        out.push(Request {
            axis: Axis::Jump,
            owner: Owner::WindCharge,
            priority: 4,
            pressed: true,
        });
    }
    let scaffold = active && with_client(&client, |runtime| runtime.enabled(ModuleId::Scaffold));
    if scaffold && game.player.is_some() {
        let edge = scaffold_edge(game);
        let held = crate::game::vape_combat::held(game);
        let Some(body) = body(game) else {
            return out;
        };
        let gui_open = with_client(&client, |runtime| runtime.gui.open);
        let requests = with_client(&client, |runtime| {
            let movement = runtime.movement();
            let forward = f32::from(u8::from(movement.user(Axis::Forward, view)))
                - f32::from(u8::from(movement.user(Axis::Back, view)));
            let sneak_held = movement.user(Axis::Sneak, view);
            let tick = scaffold::Tick {
                now_ms: runtime.now_ms(),
                no_screen: game.screen == crate::game::Screen::Playing && !gui_open,
                sneak_held,
                sneaking: body.sneaking,
                pitch: game.pitch,
                forward,
                on_ground: body.on_ground,
                on_ladder: body.on_ladder,
                edge,
                held,
            };
            runtime.scaffold_requests(&tick, &body, again)
        });
        out.extend(requests);
    }
    if BLOCK_IN_SNEAK.with(|sneak| sneak.get()) {
        out.push(Request {
            axis: Axis::Sneak,
            owner: Owner::KeyHelper,
            priority: 3,
            pressed: true,
        });
    }
    out
}

fn inv_walk_screen(game: &Game<'_>, gui_open: bool, capturing: bool) -> inv_walk::Screen {
    use crate::game::Screen;
    if capturing {
        return inv_walk::Screen::TextField;
    }
    match game.screen {
        Screen::Playing if gui_open => inv_walk::Screen::Other,
        Screen::Playing => inv_walk::Screen::None,
        Screen::Chat => inv_walk::Screen::Chat,
        Screen::Inventory if game.container.is_none() && game.remote_menu_id() == 0 => {
            inv_walk::Screen::Inventory
        }
        Screen::Paused => inv_walk::Screen::Pause,
        _ => inv_walk::Screen::Other,
    }
}

/// InvWalk's options for the screen in front of the world, and the keys
/// physically held this tick.
fn inv_walk_state(game: &Game<'_>, gui_open: bool, capturing: bool) -> (Option<inv_walk::Options>, inv_walk::Held) {
    let client = crate::game::vape_rs::core::client().clone();
    let screen = inv_walk_screen(game, gui_open, capturing);
    let options = with_client(&client, |runtime| runtime.inv_walk(screen));
    let held = inv_walk::Held {
        forward: physical(game, KeyCode::KeyW),
        back: physical(game, KeyCode::KeyS),
        left: physical(game, KeyCode::KeyA),
        right: physical(game, KeyCode::KeyD),
        jump: physical(game, KeyCode::Space),
        sneak: options.is_some_and(|o| o.sneak) && physical(game, KeyCode::ShiftLeft),
        up: game.physical_keys.contains(&KeyCode::ArrowUp),
        down: game.physical_keys.contains(&KeyCode::ArrowDown),
        turn_left: game.physical_keys.contains(&KeyCode::ArrowLeft),
        turn_right: game.physical_keys.contains(&KeyCode::ArrowRight),
    };
    (options, held)
}

fn inv_walk_keys_of(held: &inv_walk::Held) -> horizon_client::sprint::Keys {
    horizon_client::sprint::Keys {
        forward: held.forward,
        backward: held.back,
        left: held.left,
        right: held.right,
        jump: held.jump,
        shift: held.sneak,
        sprint: false,
        sprint_suppressed: false,
        analog: None,
    }
}

/// The keys InvWalk hands the local player's next controlled tick, when it is
/// active for the current screen. The game-crate caller merges them into the
/// tick's screen-released keys; the SDK's move hooks sit in the library crate
/// and cannot read this module, so a `MODIFY_ARG` on that call asks for them.
pub fn inv_walk_keys(game: &Game<'_>) -> Option<horizon_client::sprint::Keys> {
    let client = crate::game::vape_rs::core::client().clone();
    let (gui_open, capturing) = with_client(&client, |runtime| {
        (runtime.gui.open, runtime.gui.capturing())
    });
    let (options, held) = inv_walk_state(game, gui_open, capturing);
    options.map(|_| inv_walk_keys_of(&held))
}

/// The reference's `findActionAtYLevel`: the nearest crystal-base block whose
/// crystal placement is safe. The scan tries the player's y level, then the
/// one above, in the two-pass order toward the target.
fn crystal_aura_base(
    game: &Game<'_>,
    options: &crystal_aura::Options,
    feet: [f64; 3],
    eye: [f64; 3],
    health: f32,
    absorption: f32,
    player_box: crystal_aura::Aabb,
    armor: f32,
    toughness: f32,
    target: Option<([f64; 2], bool)>,
) -> Option<[i32; 3]> {
    let (px, py, pz) = (feet[0], feet[1], feet[2]);
    let (offset, close) = target?;
    let base_y = py.floor() as i32;
    let base = [px.floor() as i32, base_y, pz.floor() as i32];
    let facts = game.res.facts();
    let replaceable = |block: [i32; 3]| {
        facts
            .get(game.world.block(block[0], block[1], block[2]))
            .replaceable
    };
    for y in [base_y, base_y + 1] {
        for scan in crystal_aura::search_offsets(3, close, offset) {
            let block = [base[0] + scan[0], y, base[2] + scan[1]];
            if !crystal_aura::is_crystal_base(
                &facts
                    .get(game.world.block(block[0], block[1], block[2]))
                    .name,
            ) {
                continue;
            }
            // The crystal needs both blocks above the base.
            if !replaceable([block[0], y + 1, block[2]])
                || !replaceable([block[0], y + 2, block[2]])
            {
                continue;
            }
            let explosion = [
                f64::from(block[0]) + 0.5,
                f64::from(block[1]) + 1.0,
                f64::from(block[2]) + 0.5,
            ];
            let distance = ((explosion[0] - eye[0]).powi(2)
                + (explosion[1] - eye[1]).powi(2)
                + (explosion[2] - eye[2]).powi(2))
            .sqrt();
            if distance > 4.5 {
                continue;
            }
            // `calculateBlockDensity`: the fraction of the player's box with
            // a clear ray to the explosion, then the player's armor.
            let density = crystal_aura::block_density(
                player_box.min,
                player_box.max,
                |point| {
                    let (dx, dy, dz) = (
                        explosion[0] - point[0],
                        explosion[1] - point[1],
                        explosion[2] - point[2],
                    );
                    let reach = (dx * dx + dy * dy + dz * dz).sqrt();
                    if reach <= 0.0 {
                        return true;
                    }
                    let direction = [dx / reach, dy / reach, dz / reach];
                    game.clip_ray(point, direction, reach).is_none()
                },
            );
            let damage = crystal_aura::explosion_damage(
                crystal_aura::ExplosionType::Crystal,
                explosion,
                eye,
                density,
                armor,
                toughness,
                0.0,
            );
            if crystal_aura::self_damage_safe(
                damage,
                health,
                absorption,
                options.anti_suicide,
                options.max_self_damage,
            ) {
                return Some(block);
            }
        }
    }
    None
}

/// The reference's `validateAction`/`clickUseItem` and attack key: the host
/// presses use at the managed rotation or attacks the crystal. The pinned
/// host's local melee has no `EndCrystal` branch, so a local world damages
/// the crystal through `EndCrystal.hurtServer`'s host implementation.
fn crystal_aura_attack(game: &mut Game<'_>, id: i32, distance: f64) {
    if game.remote.is_some() {
        if game.remote_attack_entity(id, distance) {
            crate::game::remote::vape_combat_remote::swing(game);
        }
    } else {
        game.command_damage_crystal(
            id as u32,
            "minecraft:player_attack",
            horizon_client::mobs::DamageKind::PlayerAttack,
            Some(crate::game::commands::Target::Player),
            Some(crate::game::commands::Target::Player),
            None,
        );
        game.swing_arm();
    }
}

/// CrystalAura's auto loop: the real selector picks a safe base, the shared
/// silent rotation aims at the interaction point, and the reference's
/// delay/action timers drive place -> break -> attack.
fn crystal_aura_tick(game: &mut Game<'_>) {
    let client = crate::game::vape_rs::core::client().clone();
    if !with_client(&client, |runtime| runtime.enabled(ModuleId::CrystalAura)) {
        let restore = with_client(&client, |runtime| runtime.crystal_aura_release());
        if let Some(slot) = restore
            .and_then(|slot| usize::try_from(slot).ok())
            .filter(|slot| *slot < 9)
        {
            game.selected = slot;
        }
        return;
    }
    let options = with_client(&client, |runtime| {
        crystal_aura::Options::read(runtime.settings())
    });
    if options.mode != crystal_aura::Mode::Auto {
        let restore = with_client(&client, |runtime| runtime.crystal_aura_release());
        if let Some(slot) = restore
            .and_then(|slot| usize::try_from(slot).ok())
            .filter(|slot| *slot < 9)
        {
            game.selected = slot;
        }
        return;
    }
    // `attribute` computes from the modifier map, so it needs `&mut`; the
    // rest of the tick reads the player immutably.
    let (armor, toughness) = {
        let Some(player) = game.player.as_mut() else {
            return;
        };
        let armor = player.attribute("minecraft:armor").unwrap_or(0.0) as f32;
        let toughness = match player.attribute("minecraft:armor_toughness") {
            Ok(value) => value,
            Err(_) => player.attribute("armor_toughness").unwrap_or(0.0),
        } as f32;
        (armor, toughness)
    };
    let Some(player) = game.player.as_ref() else {
        return;
    };
    let (px, py, pz) = (player.position.x, player.position.y, player.position.z);
    let eye = [px, py + f64::from(player.dimensions.eye_height), pz];
    let health = player.health.health();
    let absorption = player.health.absorption();
    let bounds = player.bounds();
    let (bounds_min, bounds_max) = (bounds.min_position(), bounds.max_position());
    let player_box = crystal_aura::Aabb {
        min: [bounds_min.x, bounds_min.y, bounds_min.z],
        max: [bounds_max.x, bounds_max.y, bounds_max.z],
    };
    let can_act = matches!(game.screen, crate::game::Screen::Playing) && health > 0.0;
    let view = (game.yaw, game.pitch);
    let selected = game.selected as i32;
    let crystal_slot = game
        .inventory
        .iter()
        .take(9)
        .position(|stack| {
            stack
                .as_ref()
                .is_some_and(|stack| crystal_aura::is_end_crystal(&stack.item))
        })
        .map_or(-1, |slot| slot as i32);
    let target = game
        .mobs
        .iter()
        .filter(|mob| mob.alive())
        .filter_map(|mob| {
            let (dx, dy, dz) = (
                mob.position.x - px,
                mob.position.y - py,
                mob.position.z - pz,
            );
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();
            crystal_aura::in_range(distance, options.range).then_some((
                [mob.position.x - px, mob.position.z - pz],
                distance <= 1.75,
                mob.bounds(),
                distance,
            ))
        })
        .min_by(|a, b| a.3.total_cmp(&b.3));
    let has_target = target.is_some();
    let target_bounds = target.as_ref().map(|(_, _, bounds, _)| {
        let (min, max) = (bounds.min_position(), bounds.max_position());
        crystal_aura::Aabb {
            min: [min.x, min.y, min.z],
            max: [max.x, max.y, max.z],
        }
    });
    let base = crystal_aura_base(
        game,
        &options,
        [px, py, pz],
        eye,
        health,
        absorption,
        player_box,
        armor,
        toughness,
        target.as_ref().map(|(offset, close, _, _)| (*offset, *close)),
    );
    // The reference's `wouldIntersect`: the target's box in the crystal
    // space blocks a placement (an existing crystal is attacked instead).
    let place_blocked = match (base, target_bounds) {
        (Some(base), Some(target)) => crystal_aura::crystal_box(base).intersects(&target),
        _ => false,
    };
    let crystals: Vec<([f64; 3], i32)> = game
        .crystals
        .iter()
        .map(|crystal| {
            (
                [crystal.position.x, crystal.position.y, crystal.position.z],
                crystal.id as i32,
            )
        })
        .collect();
    let crystal_above =
        base.and_then(|base| crystal_aura::crystal_above(base, crystals.iter().copied()));
    let crystal_point = crystal_above
        .and_then(|id| crystals.iter().find(|(_, crystal)| *crystal == id))
        .map(|(position, _)| {
            crystal_aura::nearest_point(eye, crystal_aura::crystal_entity_box(*position))
        })
        .or_else(|| {
            base.map(|base| {
                [
                    f64::from(base[0]) + 0.5,
                    f64::from(base[1]) + 1.0,
                    f64::from(base[2]) + 0.5,
                ]
            })
        })
        .unwrap_or(eye);
    // The managed rotation's crosshair (the reference's
    // `getNormalReachRayTrace`), not the camera's.
    let managed = with_client(&client, |runtime| runtime.managed_view(view));
    let direction = rotation::direction(managed.0, managed.1);
    let block_hit = game.clip_ray(eye, direction, 4.5);
    let aimed_block = block_hit.map(|hit| (hit.pos, crystal_aura::facing_index(hit.face)));
    let block_distance = block_hit.map(|hit| {
        ((hit.point[0] - eye[0]).powi(2)
            + (hit.point[1] - eye[1]).powi(2)
            + (hit.point[2] - eye[2]).powi(2))
        .sqrt()
    });
    let mut aimed_crystal = None;
    let mut crystal_distance = f64::INFINITY;
    for (position, id) in &crystals {
        let bounds = crystal_aura::crystal_entity_box(*position);
        if let Some(distance) = rotation::ray_box(eye, direction, 4.5, bounds.min, bounds.max)
            && distance < crystal_distance
            && block_distance.is_none_or(|block| distance <= block)
        {
            crystal_distance = distance;
            aimed_crystal = Some(*id);
        }
    }
    let tick = crystal_aura::Tick {
        now_ms: with_client(&client, |runtime| runtime.now_ms()),
        has_target,
        base,
        crystal_above,
        crystal_slot,
        selected,
        eye,
        view,
        aimed_block,
        aimed_crystal,
        crystal_point,
        can_act,
    };
    let mut actions = with_client(&client, |runtime| runtime.crystal_aura_tick(&tick));
    if actions.use_now && place_blocked {
        actions.use_now = false;
    }
    if let Some(slot) = actions
        .select
        .and_then(|slot| usize::try_from(slot).ok())
        .filter(|slot| *slot < 9)
        && can_act
    {
        game.selected = slot;
    }
    if let Some(slot) = actions
        .restore
        .and_then(|slot| usize::try_from(slot).ok())
        .filter(|slot| *slot < 9)
    {
        game.selected = slot;
    }
    if actions.use_now && can_act {
        // `clickUseItem` at the interaction point: the placement takes the
        // managed rotation and the camera stays. A local world goes through
        // the item use, a server through the multiplayer use packet.
        let camera = (game.yaw, game.pitch);
        (game.yaw, game.pitch) = managed;
        if game.remote.is_some() {
            game.remote_use(mod_api::InteractionAttempt::Press);
        } else if game.using.is_none() {
            if let Err(error) = game.use_with_hand() {
                with_client(&client, |runtime| {
                    runtime.notify(format!("Crystal placement failed: {error}"))
                });
            }
            game.use_delay = 4;
        }
        (game.yaw, game.pitch) = camera;
    }
    if let Some(id) = actions.attack.filter(|_| can_act) {
        crystal_aura_attack(game, id, crystal_distance);
    }
}

/// The hotbar slot whose item id ends with `name`.
fn auto_anchor_hotbar(game: &Game<'_>, name: &str) -> i32 {
    game.inventory
        .iter()
        .take(9)
        .position(|stack| {
            stack
                .as_ref()
                .is_some_and(|stack| stack.item.ends_with(name))
        })
        .map_or(-1, |slot| slot as i32)
}

fn auto_anchor_count(game: &Game<'_>, slot: i32) -> i32 {
    usize::try_from(slot)
        .ok()
        .and_then(|slot| game.inventory.get(slot))
        .and_then(Option::as_ref)
        .map_or(0, |stack| stack.count as i32)
}

/// The first whitelisted hotbar slot.
fn auto_anchor_explosion_slot(game: &Game<'_>, items: &[String]) -> i32 {
    (0..9)
        .find(|slot| {
            let held = crate::game::vape_combat::item_at(game, *slot);
            crate::game::vape_rs::inventory::item_limit::matches(items, &held)
        })
        .map_or(-1, |slot| slot as i32)
}

fn auto_anchor_is_anchor(game: &Game<'_>, pos: [i32; 3]) -> bool {
    game.res
        .facts()
        .get(game.world.block(pos[0], pos[1], pos[2]))
        .name
        .contains("respawn_anchor")
}

/// The reference's `findBestAnchorFace`: the visible anchor face point with
/// the smallest angular distance from the view.
fn auto_anchor_face(
    game: &Game<'_>,
    eye: [f64; 3],
    view: (f32, f32),
    anchor: [i32; 3],
    obstruction: Option<[i32; 3]>,
) -> Option<[f64; 3]> {
    let (x, y, z) = (
        f64::from(anchor[0]),
        f64::from(anchor[1]),
        f64::from(anchor[2]),
    );
    let points = [
        [x + 0.5, y + 1.0, z + 0.5],
        [x + 0.5, y, z + 0.5],
        [x, y + 0.5, z + 0.5],
        [x + 1.0, y + 0.5, z + 0.5],
        [x + 0.5, y + 0.5, z],
        [x + 0.5, y + 0.5, z + 1.0],
        [x + 0.25, y + 1.0, z + 0.5],
        [x + 0.75, y + 1.0, z + 0.5],
        [x, y + 0.25, z + 0.5],
        [x + 1.0, y + 0.75, z + 0.5],
        [x + 0.5, y + 0.25, z],
        [x + 0.5, y + 0.75, z + 1.0],
        [x + 0.25, y, z + 0.5],
        [x + 0.75, y, z + 0.5],
    ];
    let obstruction = obstruction.map(auto_anchor::block_bounds);
    let mut best: Option<(f64, [f64; 3])> = None;
    for point in points {
        if let Some(bounds) = obstruction
            && auto_anchor::segment_hits_box(eye, point, bounds)
        {
            continue;
        }
        let (dx, dy, dz) = (point[0] - eye[0], point[1] - eye[1], point[2] - eye[2]);
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();
        if distance <= 1.0e-4 {
            continue;
        }
        let direction = [dx / distance, dy / distance, dz / distance];
        let Some(hit) = game.clip_ray(eye, direction, distance + 0.01) else {
            continue;
        };
        if hit.pos != anchor {
            continue;
        }
        let target = rotation::look_at(eye, point);
        let yaw = f64::from(rotation::wrap(target.0 - view.0));
        let pitch = f64::from(target.1 - view.1);
        let angle = (yaw * yaw + pitch * pitch).sqrt();
        if best.is_none_or(|(current, _)| angle < current) {
            best = Some((angle, point));
        }
    }
    best.map(|(_, point)| point)
}

/// The reference's `findShieldCandidate`: a glowstone block on the line
/// between the player and the anchor that partly covers the player.
fn auto_anchor_shield(
    game: &Game<'_>,
    anchor: [i32; 3],
    eye: [f64; 3],
    view: (f32, f32),
    player_box: auto_anchor::Bounds,
    feet: [f64; 3],
) -> Option<auto_anchor::ShieldCandidate> {
    let aim = auto_anchor_face(game, eye, view, anchor, None)
        .unwrap_or_else(|| auto_anchor::center(anchor));
    let (dx, dy, dz) = (aim[0] - feet[0], aim[1] - feet[1], aim[2] - feet[2]);
    let distance = (dx * dx + dy * dy + dz * dz).sqrt();
    if distance < 1.5 {
        return None;
    }
    let direction = [dx / distance, dy / distance, dz / distance];
    let mut candidates: Vec<[i32; 3]> = Vec::new();
    let mut offset = 0.8;
    while offset < distance - 0.5 {
        let block = [
            (feet[0] + direction[0] * offset).floor() as i32,
            (feet[1] + direction[1] * offset).floor() as i32,
            (feet[2] + direction[2] * offset).floor() as i32,
        ];
        if block != anchor && !candidates.contains(&block) {
            candidates.push(block);
        }
        offset += 0.4;
    }
    let facts = game.res.facts();
    let replaceable = |block: [i32; 3]| {
        facts
            .get(game.world.block(block[0], block[1], block[2]))
            .replaceable
    };
    let support = |block: [i32; 3]| -> Option<([i32; 3], u8)> {
        for (offset, facing) in [
            ([0, -1, 0], 0u8),
            ([0, 1, 0], 1),
            ([-1, 0, 0], 4),
            ([1, 0, 0], 5),
            ([0, 0, -1], 2),
            ([0, 0, 1], 3),
        ] {
            let at = [
                block[0] + offset[0],
                block[1] + offset[1],
                block[2] + offset[2],
            ];
            let state = facts.get(game.world.block(at[0], at[1], at[2]));
            if state.full_collision && !state.replaceable {
                return Some((at, facing));
            }
        }
        None
    };
    let mut best: Option<(f32, auto_anchor::ShieldCandidate)> = None;
    for block in candidates {
        if !replaceable(block) {
            continue;
        }
        let bounds = auto_anchor::block_bounds(block);
        if bounds.min[0] < player_box.max[0]
            && bounds.max[0] > player_box.min[0]
            && bounds.min[1] < player_box.max[1]
            && bounds.max[1] > player_box.min[1]
            && bounds.min[2] < player_box.max[2]
            && bounds.max[2] > player_box.min[2]
        {
            continue;
        }
        let Some((support, facing)) = support(block) else {
            continue;
        };
        if auto_anchor::anchor_fully_visible(eye, anchor, block) {
            continue;
        }
        let Some(visible) = auto_anchor_face(game, eye, view, anchor, Some(block)) else {
            continue;
        };
        let ratio = auto_anchor::visibility_ratio(visible, block, player_box);
        if ratio >= 0.15 {
            return Some(auto_anchor::ShieldCandidate {
                block,
                support,
                facing,
            });
        }
        if best.is_none_or(|(best, _)| ratio > best) {
            best = Some((
                ratio,
                auto_anchor::ShieldCandidate {
                    block,
                    support,
                    facing,
                },
            ));
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// AutoAnchor's host: the macro ledger's world inputs, the shared rotation
/// and the slot/right-click actions.
fn auto_anchor_tick(game: &mut Game<'_>) {
    let client = crate::game::vape_rs::core::client().clone();
    if !with_client(&client, |runtime| runtime.enabled(ModuleId::AutoAnchor)) {
        let restore = with_client(&client, |runtime| runtime.auto_anchor_release());
        if let Some(slot) = restore
            .and_then(|slot| usize::try_from(slot).ok())
            .filter(|slot| *slot < 9)
        {
            game.selected = slot;
        }
        AUTO_ANCHOR_RIGHT.with(|last| last.set(game.mouse_held()[1]));
        return;
    }
    let options = with_client(&client, |runtime| {
        auto_anchor::Options::read(runtime.settings())
    });
    let items = with_client(&client, |runtime| {
        auto_anchor::explosion_items(runtime.settings())
    });
    let Some(player) = game.player.as_ref() else {
        return;
    };
    let (px, py, pz) = (player.position.x, player.position.y, player.position.z);
    let eye = [px, py + f64::from(player.dimensions.eye_height), pz];
    let view = (game.yaw, game.pitch);
    let can_act = matches!(game.screen, crate::game::Screen::Playing)
        && player.health.health() > 0.0;
    let held_slot = game.selected as i32;
    let anchor_slot = auto_anchor_hotbar(game, "respawn_anchor");
    let glowstone_slot = auto_anchor_hotbar(game, "glowstone");
    let glowstone_count = auto_anchor_count(game, glowstone_slot);
    let explosion_slot = auto_anchor_explosion_slot(game, &items);
    let bounds = player.bounds();
    let (bounds_min, bounds_max) = (bounds.min_position(), bounds.max_position());
    let player_box = auto_anchor::Bounds {
        min: [bounds_min.x, bounds_min.y, bounds_min.z],
        max: [bounds_max.x, bounds_max.y, bounds_max.z],
    };
    let (anchor_pos, shield, state) = with_client(&client, |runtime| {
        let ledger = runtime.auto_anchor();
        (ledger.anchor_pos, ledger.shield, ledger.state)
    });
    let managed = with_client(&client, |runtime| runtime.managed_view(view));
    let direction = rotation::direction(managed.0, managed.1);
    let hit = game.clip_ray(eye, direction, 4.5);
    let crosshair = hit.map(|hit| auto_anchor::HitTarget {
        pos: hit.pos,
        facing: crystal_aura::facing_index(hit.face),
    });
    let crosshair_anchor = hit.is_some_and(|hit| auto_anchor_is_anchor(game, hit.pos));
    let anchor_present = anchor_pos.is_some_and(|pos| auto_anchor_is_anchor(game, pos));
    let anchor_face = if anchor_present {
        anchor_pos.and_then(|anchor| {
            auto_anchor_face(game, eye, view, anchor, shield.map(|shield| shield.block))
        })
    } else {
        None
    };
    let shield_candidate = if options.safe_anchor && shield.is_none() && anchor_present {
        anchor_pos.and_then(|anchor| {
            auto_anchor_shield(game, anchor, eye, view, player_box, [px, py, pz])
        })
    } else {
        shield
    };
    let shield_clear = shield_candidate.is_some_and(|candidate| {
        !game
            .res
            .facts()
            .get(
                game.world
                    .block(candidate.block[0], candidate.block[1], candidate.block[2]),
            )
            .replaceable
    });
    let tick = auto_anchor::Tick {
        now_ms: with_client(&client, |runtime| runtime.now_ms()),
        eye,
        view,
        held_slot,
        anchor_slot,
        glowstone_slot,
        glowstone_count,
        explosion_slot,
        crosshair,
        crosshair_anchor,
        anchor_present,
        anchor_face,
        shield: shield_candidate,
        shield_clear,
        can_act,
    };
    let right = game.mouse_held()[1];
    let rising = right && !AUTO_ANCHOR_RIGHT.with(|last| last.get());
    AUTO_ANCHOR_RIGHT.with(|last| last.set(right));
    if rising && options.mode == auto_anchor::Mode::OnPlace && state == auto_anchor::State::Idle {
        with_client(&client, |runtime| runtime.auto_anchor_start_on_place(&tick));
    }
    let actions = with_client(&client, |runtime| runtime.auto_anchor_tick(&tick));
    if let Some(slot) = actions
        .select
        .and_then(|slot| usize::try_from(slot).ok())
        .filter(|slot| *slot < 9)
    {
        game.selected = slot;
    }
    if let Some(slot) = actions
        .restore
        .and_then(|slot| usize::try_from(slot).ok())
        .filter(|slot| *slot < 9)
    {
        game.selected = slot;
    }
    if actions.uses > 0 && can_act {
        let camera = (game.yaw, game.pitch);
        (game.yaw, game.pitch) = managed;
        for _ in 0..actions.uses {
            if game.remote.is_some() {
                game.remote_use(mod_api::InteractionAttempt::Press);
            } else if game.using.is_none() {
                let _ = game.use_with_hand();
                game.use_delay = 4;
            }
        }
        (game.yaw, game.pitch) = camera;
    }
}

/// The reference's `blockPriorityIndex`: the block's display name (or id)
/// against the priority list.
fn block_in_priority(held: &crate::game::vape_rs::inventory::item_limit::HeldItem) -> usize {
    let text = format!(
        "{} {}",
        held.display_name.to_lowercase(),
        held.id.as_deref().unwrap_or("").to_lowercase()
    );
    if text.contains("wool") {
        0
    } else if text.contains("planks") {
        2
    } else if text.contains("sandstone") {
        3
    } else if text.contains("terracotta") || text.contains("stained_clay") {
        4
    } else if text.contains("end_stone") {
        5
    } else if text.contains("obsidian") {
        6
    } else if text.contains("stone") {
        1
    } else {
        7
    }
}

/// The reference's `selectBestBlockSlot`: the priority list, or the block
/// hardness in Hardest mode; the stack size breaks ties.
fn block_in_block_slot(game: &Game<'_>, options: &block_in::Options) -> i32 {
    let mut best = -1;
    let mut best_priority = usize::MAX;
    let mut best_hardness = f32::NEG_INFINITY;
    let mut best_count = 0i32;
    for slot in 0..9usize {
        let Some(stack) = game.inventory.get(slot).and_then(|s| s.as_ref()) else {
            continue;
        };
        let Some(info) = game.res.items().get(&stack.item) else {
            continue;
        };
        if info.block.is_none() || stack.count == 0 {
            continue;
        }
        let held = crate::game::vape_combat::item_at(game, slot);
        if options.use_blacklist
            && crate::game::vape_rs::inventory::item_limit::matches(&options.blacklist, &held)
        {
            continue;
        }
        let count = stack.count as i32;
        let better = if options.priority == block_in::Priority::Hardest {
            let hardness = info
                .block
                .map_or(0.0, |state| game.res.facts().get(state).hardness);
            hardness > best_hardness || (hardness == best_hardness && count > best_count)
        } else {
            let priority = block_in_priority(&held);
            priority < best_priority || (priority == best_priority && count > best_count)
        };
        if better {
            best = slot as i32;
            best_priority = block_in_priority(&held);
            best_hardness = info
                .block
                .map_or(0.0, |state| game.res.facts().get(state).hardness);
            best_count = count;
        }
    }
    best
}

/// The nearest bed's horizontal centre within the reference's 10-block scan.
fn block_in_bed(game: &Game<'_>, feet: [f64; 3]) -> Option<[f64; 2]> {
    let facts = game.res.facts();
    let (fx, fy, fz) = (
        feet[0].floor() as i32,
        feet[1].floor() as i32,
        feet[2].floor() as i32,
    );
    let mut best: Option<(f64, [f64; 2])> = None;
    for dx in -10..10 {
        for dz in -10..10 {
            for dy in -3..3 {
                let (x, z) = (fx + dx, fz + dz);
                if !facts
                    .get(game.world.block(x, fy + dy, z))
                    .name
                    .ends_with("_bed")
                {
                    continue;
                }
                let distance = f64::from(dx * dx + dz * dz);
                if best.is_none_or(|(current, _)| distance < current) {
                    best = Some((distance, [f64::from(x) + 0.5, f64::from(z) + 0.5]));
                }
            }
        }
    }
    best.map(|(_, bed)| bed)
}

/// The enclosure cells still air with a visible solid support: the four
/// cardinal cells around the player's feet block and the four above it.
fn block_in_targets(
    game: &Game<'_>,
    options: &block_in::Options,
    feet: [f64; 3],
    eye: [f64; 3],
) -> Vec<block_in::Placement> {
    let facts = game.res.facts();
    let solid = |pos: [i32; 3]| {
        let state = facts.get(game.world.block(pos[0], pos[1], pos[2]));
        state.full_collision && !state.replaceable
    };
    let replaceable = |pos: [i32; 3]| {
        facts
            .get(game.world.block(pos[0], pos[1], pos[2]))
            .replaceable
    };
    let (fx, fy, fz) = (
        feet[0].floor() as i32,
        feet[1].floor() as i32,
        feet[2].floor() as i32,
    );
    let mut out = Vec::new();
    for y in [fy, fy + 1] {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let cell = [fx + dx, y, fz + dz];
            if !replaceable(cell) {
                continue;
            }
            let mut support = None;
            for (offset, facing) in [
                ([0, -1, 0], 1u8),
                ([0, 1, 0], 0),
                ([-1, 0, 0], 5),
                ([1, 0, 0], 4),
                ([0, 0, -1], 3),
                ([0, 0, 1], 2),
            ] {
                let at = [
                    cell[0] + offset[0],
                    cell[1] + offset[1],
                    cell[2] + offset[2],
                ];
                if !solid(at) {
                    continue;
                }
                let point = block_in::face_center(at, facing);
                let (dx, dy, dz) = (
                    point[0] - eye[0],
                    point[1] - eye[1],
                    point[2] - eye[2],
                );
                let distance = (dx * dx + dy * dy + dz * dz).sqrt();
                if distance <= 1.0e-4 {
                    continue;
                }
                let direction = [dx / distance, dy / distance, dz / distance];
                if game.clip_ray(eye, direction, distance + 0.01).is_some_and(|hit| {
                    hit.pos == at && crystal_aura::facing_index(hit.face) == facing
                }) {
                    support = Some((at, facing));
                    break;
                }
            }
            if let Some((support, facing)) = support {
                out.push(block_in::Placement {
                    support,
                    facing,
                    place: cell,
                });
            }
        }
    }
    let bed = if options.bed_finder {
        block_in_bed(game, feet)
    } else {
        None
    };
    out.sort_by(|a, b| {
        let key = |placement: &block_in::Placement| {
            let dx = f64::from(placement.place[0]) - feet[0];
            let dz = f64::from(placement.place[2]) - feet[2];
            let toward_bed = bed.map_or(0.0, |bed| {
                let (bx, bz) = (bed[0] - feet[0], bed[1] - feet[2]);
                let length = (bx * bx + bz * bz).sqrt().max(1.0e-6);
                (dx * bx + dz * bz) / length
            });
            (
                i32::from(placement.place[1] != fy),
                -toward_bed,
                dx * dx + dz * dz,
            )
        };
        key(a)
            .partial_cmp(&key(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Block-In's host: the enclosure targets, the block slot, the managed
/// crosshair, the shared rotation and the placement click.
fn block_in_tick(game: &mut Game<'_>) {
    let client = crate::game::vape_rs::core::client().clone();
    if !with_client(&client, |runtime| runtime.enabled(ModuleId::BlockIn)) {
        let keep = with_client(&client, |runtime| {
            let options = block_in::Options::read(runtime.settings());
            options.sneak && options.keep_sneak
        });
        let restore = with_client(&client, |runtime| runtime.block_in_release());
        if let Some(slot) = restore
            .and_then(|slot| usize::try_from(slot).ok())
            .filter(|slot| *slot < 9)
        {
            game.selected = slot;
        }
        if !keep {
            BLOCK_IN_SNEAK.with(|sneak| sneak.set(false));
        }
        return;
    }
    let options = with_client(&client, |runtime| {
        block_in::Options::read(runtime.settings())
    });
    let Some(player) = game.player.as_ref() else {
        return;
    };
    let (px, py, pz) = (player.position.x, player.position.y, player.position.z);
    let eye = [px, py + f64::from(player.dimensions.eye_height), pz];
    let view = (game.yaw, game.pitch);
    let can_act = matches!(game.screen, crate::game::Screen::Playing)
        && player.health.health() > 0.0;
    let held_slot = game.selected as i32;
    let block_slot = block_in_block_slot(game, &options);
    let on_solid = game
        .res
        .facts()
        .get(
            game.world.block(
                px.floor() as i32,
                (py - 1.0).floor() as i32,
                pz.floor() as i32,
            ),
        )
        .full_collision;
    let targets = block_in_targets(game, &options, [px, py, pz], eye);
    let managed = with_client(&client, |runtime| runtime.managed_view(view));
    let direction = rotation::direction(managed.0, managed.1);
    let crosshair = game.clip_ray(eye, direction, 4.5).map(|hit| {
        (
            hit.pos,
            crystal_aura::facing_index(hit.face),
            hit.point,
        )
    });
    let tick = block_in::Tick {
        now_ms: with_client(&client, |runtime| runtime.now_ms()),
        eye,
        view,
        on_solid,
        held_slot,
        block_slot,
        crosshair,
        targets,
        can_act,
    };
    let actions = with_client(&client, |runtime| runtime.block_in_tick(&tick));
    BLOCK_IN_SNEAK.with(|sneak| sneak.set(actions.sneak));
    if let Some(slot) = actions
        .select
        .and_then(|slot| usize::try_from(slot).ok())
        .filter(|slot| *slot < 9)
    {
        game.selected = slot;
    }
    if let Some(slot) = actions
        .restore
        .and_then(|slot| usize::try_from(slot).ok())
        .filter(|slot| *slot < 9)
    {
        game.selected = slot;
    }
    if actions.uses > 0 && can_act {
        let camera = (game.yaw, game.pitch);
        (game.yaw, game.pitch) = managed;
        if game.remote.is_some() {
            game.remote_use(mod_api::InteractionAttempt::Press);
        } else if game.using.is_none() {
            let _ = game.use_with_hand();
            game.use_delay = 4;
        }
        (game.yaw, game.pitch) = camera;
    }
}

/// Client tick HEAD, before the movement writer.
pub(crate) fn tick(game: &mut Game<'_>) {
    if game.player.is_none() {
        return;
    }
    let client = crate::game::vape_rs::core::client().clone();
    // BedBreaker's scheduled action: the reference scans every 50 ticks.
    let due = BED_SCAN_TICKS.with(|ticks| {
        let next = ticks.get() + 1;
        ticks.set(next);
        next % 50 == 0
    });
    if due {
        let beds = bed_scan(game);
        with_client(&client, |runtime| runtime.bed_breaker_scan(&beds));
    }
    // BedBreaker's selection/cover runs after the scan.
    let _ = with_client(&client, |runtime| {
        runtime.bed_breaker_tick(
            &BedWorld { game },
            std::time::Instant::now().elapsed().as_millis() as u64,
        )
    });
    crystal_aura_tick(game);
    auto_anchor_tick(game);
    block_in_tick(game);
    // AutoTotem's silent open holds the movement keys (and attack/use) from
    // the open until one "Silent move delay" after its close.
    let totem_hold = with_client(&client, |runtime| {
        runtime.auto_totem_tick(std::time::Instant::now())
    });
    TOTEM_HOLD.with(|h| h.set(totem_hold));
    let (gui_open, capturing) = with_client(&client, |runtime| {
        (runtime.gui.open, runtime.gui.capturing())
    });
    // The shared rotation manager (`RotationManager.onTick`): a visible
    // controller turns the view; it waits while a screen is open.
    let no_screen = matches!(game.screen, crate::game::Screen::Playing) && !gui_open;
    let sensitivity = f64::from(game.settings.sensitivity);
    let body = body(game);
    let view = (game.yaw, game.pitch);
    if let Some(p) = game.player.as_ref() {
        let eye = [
            p.position.x,
            p.position.y + f64::from(p.dimensions.eye_height),
            p.position.z,
        ];
        with_client(&client, |runtime| runtime.rotation_eye(eye));
    }
    let rotated = with_client(&client, |runtime| match &body {
        Some(body) => runtime.rotation_tick_at(view, sensitivity, no_screen, body),
        None => runtime.rotation_tick(view, sensitivity, no_screen),
    });
    if let Some([yaw, pitch]) = rotated {
        game.yaw = yaw;
        game.pitch = pitch;
    }
    // The reference's bus dispatches `EventPriority.values()` from LOWEREST
    // up, so `RotationManager.onTick` (LOWEST) turns the view and refreshes
    // `normalReachRayTrace` before the bridge modes' pre-tick reads it.
    let body = self::body(game);
    scaffold_bridge_tick(game, &client, no_screen, body);
    // InvWalk.
    let (options, held) = inv_walk_state(game, gui_open, capturing);
    let keys = options.map(|o| {
        let (yaw, pitch) = inv_walk::rotate(&o, &held, game.yaw, game.pitch);
        game.yaw = yaw;
        game.pitch = pitch;
        inv_walk_keys_of(&held)
    });
    if let Some(player) = game.player.as_ref() {
        player.vape_rs_set_inv_walk(keys.filter(|_| !totem_hold));
    }
    wind_charge_tick(game, &client);
}

/// Scaffold GodBridge/TellyBridge's pre-tick: the slot, the right click
/// (`use.onTick(1)`) and the movement task's keys.
fn scaffold_bridge_tick(
    game: &mut Game<'_>,
    client: &crate::game::vape_rs::core::Client,
    no_screen: bool,
    body: Option<Body>,
) {
    let Some(body) = body else {
        return;
    };
    let bridging = with_client(client, |runtime| {
        runtime.enabled(ModuleId::Scaffold)
            && scaffold::Options::read(runtime.settings()).mode != scaffold::Mode::Legit
    });
    if !bridging {
        return;
    }
    let view = crate::game::vape_input_delays::host_view(game);
    let user = with_client(client, |runtime| {
        let movement = runtime.movement();
        Axis::ALL.map(|axis| movement.user(axis, &view))
    });
    let mouse = game.mouse.accumulated();
    let tick = BridgeTick {
        no_screen,
        body,
        inventory: inventory(game),
        hit: crosshair_block(game),
        right_mouse: game.mouse_held()[1],
        mouse: [mouse.0 as i32, mouse.1 as i32],
        view: [game.yaw, game.pitch],
        user,
    };
    let actions = with_client(client, |runtime| {
        runtime.scaffold_bridge_tick(tick, &mut HostWorld(game))
    });
    let alive = game
        .player
        .as_ref()
        .is_some_and(|p| p.health.health() > 0.0);
    if let Some(slot) = actions.select.filter(|s| *s < 9) {
        if alive && game.title.is_none() {
            game.selected = slot;
        }
    }
    if actions.click && alive {
        crate::game::remote::vape_macros::use_once(game);
    }
}

fn wind_charge_tick(game: &mut Game<'_>, client: &crate::game::vape_rs::core::Client) {
    let charge_slot = game.inventory.iter().take(9).position(|s| {
        s.as_ref()
            .is_some_and(|s| &*s.item == "minecraft:wind_charge")
    });
    let Some(player) = game.player.as_ref() else {
        return;
    };
    let tick = wind_charge::Tick {
        charge_slot,
        selected: game.selected,
        on_ground: player.ground.on_ground,
        view: [game.yaw, game.pitch],
    };
    let alive = player.health.health() > 0.0;
    let actions = with_client(client, |runtime| runtime.wind_charge_tick(&tick));
    if let Some(slot) = actions.select.filter(|s| *s < 9) {
        if alive && game.title.is_none() {
            game.selected = slot;
        }
    }
    if actions.throw && game.remote.is_some() {
        // The throw (UseItem carries the rotation) and its crosshair use the
        // managed rotation; with silent aim the camera stays.
        let view = (game.yaw, game.pitch);
        if let Some([yaw, pitch]) = actions.rotation {
            game.yaw = yaw;
            game.pitch = pitch;
        }
        game.remote_use(mod_api::InteractionAttempt::Press);
        if actions.silent {
            (game.yaw, game.pitch) = view;
        }
    }
    if actions.jump {
        JUMP.with(|j| j.set(true));
    }
}

/// BedBreaker's `onScheduledAction` scan: beds within the reference's 100
/// block x/z radius and ±20 y, only one half of each pair (the reference
/// skips the foot half). The state's `part` property is not read here; the
/// pair keeps the lower (x, z) half, which leaves the cover positions the
/// same for either half.
fn bed_scan(game: &Game<'_>) -> Vec<[i32; 3]> {
    let Some(player) = game.player.as_ref() else {
        return Vec::new();
    };
    let (px, py, pz) = (
        player.position.x.floor() as i32,
        player.position.y.floor() as i32,
        player.position.z.floor() as i32,
    );
    let facts = game.res.facts();
    let bed_at = |x: i32, y: i32, z: i32| {
        facts
            .get(game.world.block(x, y, z))
            .name
            .ends_with("_bed")
    };
    let mut beds = Vec::new();
    for dx in -100..100 {
        for dz in -100..100 {
            let (x, z) = (px + dx, pz + dz);
            for step in 0..20 {
                let mut visit = |y: i32| {
                    if !bed_at(x, y, z) {
                        return;
                    }
                    let mut other = None;
                    for (ox, oz) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                        if bed_at(x + ox, y, z + oz) {
                            other = Some((x + ox, z + oz));
                            break;
                        }
                    }
                    let Some(other) = other else {
                        return;
                    };
                    if (x, z) < other {
                        beds.push([x, y, z]);
                    }
                };
                visit(py + step);
                if step != 0 {
                    visit(py - step);
                }
            }
        }
    }
    beds
}

/// The BedBreaker host's block and hotbar queries.
struct BedWorld<'g, 'a> {
    game: &'g Game<'a>,
}

impl crate::game::vape_rs::modules::world::bed_breaker::WorldView for BedWorld<'_, '_> {
    fn block(&self, pos: [i32; 3]) -> Option<&str> {
        Some(
            &*self
                .game
                .res
                .facts()
                .get(self.game.world.block(pos[0], pos[1], pos[2]))
                .name,
        )
    }
    fn bed_facing(
        &self,
        pos: [i32; 3],
    ) -> Option<crate::game::vape_rs::modules::world::bed_breaker::Facing> {
        use crate::game::vape_rs::modules::world::bed_breaker::Facing;
        let bed = |x: i32, z: i32| {
            self
                .game
                .res
                .facts()
                .get(self.game.world.block(x, pos[1], z))
                .name
                .ends_with("_bed")
        };
        for (dx, dz, facing) in [
            (0, 1, Facing::South),
            (0, -1, Facing::West),
            (1, 0, Facing::North),
            (-1, 0, Facing::East),
        ] {
            if bed(pos[0] + dx, pos[2] + dz) {
                return Some(facing);
            }
        }
        None
    }
    fn replaceable(&self, pos: [i32; 3]) -> bool {
        self
            .game
            .res
            .facts()
            .get(self.game.world.block(pos[0], pos[1], pos[2]))
            .replaceable
    }
    fn destroy_progress(&self, pos: [i32; 3], _slot: usize) -> f32 {
        let facts = self
            .game
            .res
            .facts()
            .get(self.game.world.block(pos[0], pos[1], pos[2]));
        horizon_items::mining::destroy_progress(facts.hardness, false, 1.0)
    }
    fn hotbar(&self, slot: usize) -> Option<String> {
        self
            .game
            .inventory
            .get(slot)
            .and_then(Option::as_ref)
            .map(|stack| stack.item.to_string())
    }
}
