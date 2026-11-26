// Runtime side of Block-In: the shared rotation (silent managed or a visible
// fixed controller) the placement is aimed under, and the module state
// reset on disable.
use super::super::modules::combat::rotation::{Claimant, Fixed, Step, look_at};
use super::super::modules::world::block_in;
use super::rotation_runtime::{Shared, Silent};
use super::{ModuleId, Runtime};

impl Runtime {
    /// One tick of the reference's `BlockIn.onTick`: claim the rotation, run
    /// the placement ledger and aim at its support face.
    pub fn block_in_tick(&mut self, tick: &block_in::Tick) -> block_in::Actions {
        let options = block_in::Options::read(&self.settings);
        let actions;
        {
            let mut shared = Shared::of(&mut self.combat, &mut self.rotations);
            if !shared.claim(Claimant::BlockIn, options.silent_aim) {
                return block_in::Actions::default();
            }
            actions = self.block_in.tick(tick, &options);
            if actions.release || actions.disable {
                shared.release(Claimant::BlockIn);
            } else if let Some(point) = actions.aim {
                let (yaw, pitch) = look_at(tick.eye, point);
                if options.silent_aim {
                    shared.silent(
                        Claimant::BlockIn,
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
                } else {
                    let speed = options.aim_speed as f32;
                    if let Some(existing) = shared.visible_mut(Claimant::BlockIn) {
                        existing.target = [yaw, pitch];
                        existing.speed = speed;
                    } else {
                        let mut fixed = Fixed::new([yaw, pitch], speed);
                        fixed.tolerance = 0.0;
                        fixed.clamp = true;
                        fixed.retain = true;
                        fixed.linear_acceleration = true;
                        fixed.scale_axes = true;
                        shared.visible(Claimant::BlockIn, fixed);
                    }
                }
            }
        }
        if let Some(message) = actions.notify {
            self.notify(message.to_owned());
        }
        if actions.disable {
            self.set_enabled(ModuleId::BlockIn, false);
        }
        actions
    }

    /// The module is no longer running: release the rotation and return the
    /// saved hotbar slot to restore.
    pub fn block_in_release(&mut self) -> Option<i32> {
        let restore = (self.block_in.prev_slot >= 0).then_some(self.block_in.prev_slot);
        self.block_in.reset();
        Shared::of(&mut self.combat, &mut self.rotations).release(Claimant::BlockIn);
        restore
    }
}
