// NoFall and Fly at the local player's motion packet (`EventPreMotion`). The
// send-state HEAD decides the on-ground flag; the argument modifier on the
// position sender's update applies it to this packet only.
use crate::game::Game;
use crate::game::vape_rs::core::{ModuleId, with_client};
use horizon_protocol::packets::play::{self, Serverbound as S};

thread_local! {
    static GROUND: core::cell::Cell<Option<bool>> = const { core::cell::Cell::new(None) };
}

/// `remote_send_state` HEAD.
pub fn pre_motion(game: &mut Game<'_>) {
    GROUND.with(|g| g.set(None));
    let Some(player) = game.player.as_ref() else {
        return;
    };
    let client = crate::game::vape_rs::core::client().clone();
    let dead = player.health.health() <= 0.0;
    let skip = dead || game.creative || player.flying;
    let (fall, motion_y, collision) = (
        player.fall_distance as f32,
        player.velocity.y,
        player.collision.horizontal,
    );
    // Step: a recognised step at this send.
    let sample = crate::game::vape_rs::modules::movement::step::Sample {
        position: [player.position.x, player.position.y, player.position.z],
        on_ground: player.ground.on_ground,
        sneaking: game.keys.raw(&winit::keyboard::KeyCode::ShiftLeft),
    };
    let (fly, effects) = with_client(&client, |runtime| {
        (
            runtime.enabled(ModuleId::Fly) && runtime.session().in_world(),
            runtime.no_fall_motion(skip, fall, motion_y),
        )
    });
    if effects.status_pair {
        for on_ground in [true, false] {
            game.send(S::MovePlayerStatus(play::MovePlayerStatus {
                flags: u8::from(on_ground) | u8::from(collision) << 1,
            }));
        }
    }
    if effects.claim_ground {
        // NoFall Normal: a held jump is released for the claimed landing.
        let jump = game.keys.raw(&winit::keyboard::KeyCode::Space);
        with_client(&client, |runtime| runtime.no_fall_claimed(jump));
    }
    // The reference's two intermediate step positions are not sent: a 26.3
    // server allows one position per client tick and disconnects the player
    // ("Invalid move player packet received"). The step still resets Speed
    // and runs the timer.
    with_client(&client, |runtime| runtime.step_motion(sample));
    let server_block = with_client(&client, |runtime| runtime.bed_breaker_server_rotation());
    if let Some(block) = server_block {
        let Some(player) = game.player.as_ref() else {
            return;
        };
        let eye = [
            player.position.x,
            player.position.y + f64::from(player.dimensions.eye_height),
            player.position.z,
        ];
        let target = [
            f64::from(block[0]) + 0.5,
            f64::from(block[1]) + 0.5,
            f64::from(block[2]) + 0.5,
        ];
        let angles = bed_angles(eye, target);
        with_client(&client, |runtime| {
            runtime.set_bed_breaker_rotation((game.yaw, game.pitch), angles)
        });
    } else {
        with_client(&client, |runtime| runtime.release_bed_breaker_rotation());
    }
    let (yaw, pitch) =
        with_client(&client, |runtime| runtime.managed_view((game.yaw, game.pitch)));
    game.yaw = yaw;
    game.pitch = pitch;
    let ground = if fly {
        Some(false)
    } else {
        effects.claim_ground.then_some(true)
    };
    GROUND.with(|g| g.set(ground));
}

/// The claimed on-ground value for the next movement packet, if any.
pub fn take_ground() -> Option<bool> {
    GROUND.with(|g| g.take())
}

/// The yaw/pitch from an eye to a block centre, in the game's convention.
fn bed_angles(eye: [f64; 3], target: [f64; 3]) -> (f32, f32) {
    let dx = target[0] - eye[0];
    let dy = target[1] - eye[1];
    let dz = target[2] - eye[2];
    (
        (-dx).atan2(dz).to_degrees() as f32,
        (-dy).atan2((dx * dx + dz * dz).sqrt()).to_degrees() as f32,
    )
}
