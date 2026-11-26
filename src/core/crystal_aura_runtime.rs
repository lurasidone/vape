// Runtime side of CrystalAura's auto placement loop: the state ledger, the
// activation/action timers and the shared silent rotation aimed at the
// interaction point. The host resolves the selector, the crystal above the
// base and the crosshair along the managed rotation, then performs the
// returned slot/use/attack actions.
use super::super::modules::combat::crystal_aura;
use super::super::modules::combat::rotation::{Claimant, Step, look_at};
use super::Runtime;
use super::rotation_runtime::{Shared, Silent};

impl Runtime {
    /// One tick of the reference's auto placement loop. The rotation is
    /// claimed and aimed here; `Actions` tells the host what to press.
    pub fn crystal_aura_tick(&mut self, tick: &crystal_aura::Tick) -> crystal_aura::Actions {
        let options = crystal_aura::Options::read(&self.settings);
        let mut actions = self.crystal_aura.tick(tick, &options);
        let mut shared = Shared::of(&mut self.combat, &mut self.rotations);
        if actions.release {
            shared.release(Claimant::CrystalAura);
            return actions;
        }
        if let Some(point) = actions.aim {
            if !shared.claim(Claimant::CrystalAura, true) {
                // The reference's `aim lock`: no slot switch and no action.
                actions.select = None;
                actions.use_now = false;
                actions.attack = None;
                return actions;
            }
            let (yaw, pitch) = look_at(tick.eye, point);
            shared.silent(
                Claimant::CrystalAura,
                tick.view,
                Silent {
                    target: (yaw, pitch),
                    step: Step {
                        counts: options.aim_speed * 0.25,
                        clamp: true,
                        proportional: true,
                        linear: true,
                    },
                    tolerance: 0.0,
                },
            );
        }
        // A completed cycle reports its attack without an aim; it still needs
        // the claim the action was aimed under (the reference acts only while
        // it owns the rotation).
        if !shared.owned_by(Claimant::CrystalAura) {
            actions.use_now = false;
            actions.attack = None;
        }
        actions
    }

    /// The module is no longer running: release the rotation and return the
    /// saved hotbar slot to restore.
    pub fn crystal_aura_release(&mut self) -> Option<i32> {
        let restore = (self.crystal_aura.saved_slot >= 0).then_some(self.crystal_aura.saved_slot);
        self.crystal_aura.reset();
        Shared::of(&mut self.combat, &mut self.rotations).release(Claimant::CrystalAura);
        restore
    }
}
