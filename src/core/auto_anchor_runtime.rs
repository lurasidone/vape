// Runtime side of AutoAnchor: the macro state machine's shared rotation
// (silent managed or a visible fixed controller) and the claim it acts
// under. The host resolves the world snapshot and performs the slot/use
// actions.
use super::super::modules::combat::rotation::{Claimant, Fixed, Step, look_at};
use super::super::modules::utility::auto_anchor;
use super::rotation_runtime::{Shared, Silent};
use super::{ModuleId, Runtime};

impl Runtime {
    /// One tick of the reference's `onTick`: claim the rotation, run the
    /// state machine and aim at its target.
    pub fn auto_anchor_tick(&mut self, tick: &auto_anchor::Tick) -> auto_anchor::Actions {
        let options = auto_anchor::Options::read(&self.settings);
        let actions;
        {
            let mut shared = Shared::of(&mut self.combat, &mut self.rotations);
            if options.aim_assist && !shared.claim(Claimant::AutoAnchor, options.silent_aim) {
                // `hasRotationClaim` refuses: the state machine waits.
                return auto_anchor::Actions::default();
            }
            if !options.aim_assist && shared.active(Claimant::AutoAnchor) {
                shared.release(Claimant::AutoAnchor);
            }
            actions = self.auto_anchor.tick(tick, &options);
            if actions.release || actions.disable {
                shared.release(Claimant::AutoAnchor);
            } else if let Some(point) = actions.aim
                && options.aim_assist
            {
                let (yaw, pitch) = look_at(tick.eye, point);
                if options.silent_aim {
                    shared.silent(
                        Claimant::AutoAnchor,
                        tick.view,
                        Silent {
                            target: (yaw, pitch),
                            step: Step {
                                counts: options.aim_speed * 0.25,
                                clamp: true,
                                proportional: false,
                                linear: true,
                            },
                            tolerance: 0.0,
                        },
                    );
                    shared.managed.set_acceleration(true, true);
                } else {
                    let speed = options.aim_speed as f32;
                    if let Some(existing) = shared.visible_mut(Claimant::AutoAnchor) {
                        existing.target = [yaw, pitch];
                        existing.speed = speed;
                    } else {
                        let mut fixed = Fixed::new([yaw, pitch], speed);
                        fixed.tolerance = 0.0;
                        fixed.clamp = true;
                        fixed.retain = true;
                        fixed.angle_acceleration = true;
                        fixed.linear_acceleration = true;
                        fixed.cubic_acceleration = true;
                        fixed.scale_axes = false;
                        shared.visible(Claimant::AutoAnchor, fixed);
                    }
                }
            }
        }
        if let Some(message) = actions.notify {
            self.notify(message.to_owned());
        }
        if actions.disable {
            self.set_enabled(ModuleId::AutoAnchor, false);
        }
        actions
    }

    /// The On place trigger: the host saw a fresh right-click while holding
    /// an anchor. False when the mode or state does not take it.
    pub fn auto_anchor_start_on_place(&mut self, tick: &auto_anchor::Tick) -> bool {
        let options = auto_anchor::Options::read(&self.settings);
        self.auto_anchor.start_on_place(tick, &options)
    }

    /// The module is no longer running: release the rotation and return the
    /// saved hotbar slot to restore.
    pub fn auto_anchor_release(&mut self) -> Option<i32> {
        let restore = (self.auto_anchor.prev_slot >= 0).then_some(self.auto_anchor.prev_slot);
        self.auto_anchor.reset();
        Shared::of(&mut self.combat, &mut self.rotations).release(Claimant::AutoAnchor);
        restore
    }
}
