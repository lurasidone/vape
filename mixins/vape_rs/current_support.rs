//! Current SDK support for state carried across Horizon's movement hooks.
//! These are traits because the SDK owns `PlayerMotion`, `MotionWorld` and
//! `FixedStep`, so a mod cannot add inherent methods to those types.
use std::cell::Cell;

thread_local! {
    static SAFE_WALK: Cell<usize> = const { Cell::new(0) };
    static NO_SLOWDOWN: Cell<usize> = const { Cell::new(0) };
    static FLY: Cell<(usize, [f64; 3])> = const { Cell::new((0, [0.0; 3])) };
    static FLY_BEFORE: Cell<([f64; 3], f64, usize)> =
        const { Cell::new(([f64::NAN; 3], f64::NAN, 0)) };
    static INV_WALK: Cell<(usize, Option<crate::sprint::Keys>)> = const { Cell::new((0, None)) };
    static SPRINT: Cell<usize> = const { Cell::new(0) };
    static TIMER: Cell<(usize, f32)> = const { Cell::new((0, 1.0)) };
    static TIMER_FRACTION: Cell<(usize, f32)> = const { Cell::new((0, 0.0)) };
    static TIMER_REENTRY: Cell<bool> = const { Cell::new(false) };
}

/// Fly/Speed/LongJump: the pre-travel position and fall of the local player
/// for the tick whose vanilla travel the module's move replaces.
pub(crate) fn vape_rs_set_fly_before(position: [f64; 3], fall_distance: f64, events: usize) {
    FLY_BEFORE.with(|cell| cell.set((position, fall_distance, events)));
}

pub(crate) fn vape_rs_take_fly_before() -> Option<([f64; 3], f64, usize)> {
    let (position, fall_distance, events) = FLY_BEFORE.with(Cell::take);
    (!position[0].is_nan()).then_some((position, fall_distance, events))
}

pub trait PlayerMotionExt {
    fn vape_rs_set_safe_walk(&self, on: bool);
    fn vape_rs_safe_walk(&self) -> bool;
    fn vape_rs_set_no_slowdown(&self, on: bool);
    fn vape_rs_no_slowdown(&self) -> bool;
    fn vape_rs_set_fly_move(&self, movement: Option<[f64; 3]>);
    fn vape_rs_take_fly_move(
        &self,
        host: horizon_core::geometry::Vec3,
    ) -> Option<horizon_core::geometry::Vec3>;
    fn vape_rs_set_inv_walk(&self, keys: Option<crate::sprint::Keys>);
    fn vape_rs_take_inv_walk(&self) -> Option<crate::sprint::Keys>;
    fn vape_rs_set_no_slowdown_sprint(&self, on: bool);
    fn vape_rs_no_slowdown_sprint(&self) -> bool;
}

impl PlayerMotionExt for crate::motion::PlayerMotion {
    fn vape_rs_set_safe_walk(&self, on: bool) {
        mark(&SAFE_WALK, self, on);
    }
    fn vape_rs_safe_walk(&self) -> bool {
        marked(&SAFE_WALK, self)
    }
    fn vape_rs_set_no_slowdown(&self, on: bool) {
        mark(&NO_SLOWDOWN, self, on);
    }
    fn vape_rs_no_slowdown(&self) -> bool {
        marked(&NO_SLOWDOWN, self)
    }
    fn vape_rs_set_fly_move(&self, movement: Option<[f64; 3]>) {
        let id = self as *const Self as usize;
        FLY.with(|cell| match movement {
            Some(movement) => cell.set((id, movement)),
            None if cell.get().0 == id => cell.set((0, [0.0; 3])),
            None => {}
        });
    }
    fn vape_rs_take_fly_move(
        &self,
        host: horizon_core::geometry::Vec3,
    ) -> Option<horizon_core::geometry::Vec3> {
        FLY.with(|cell| {
            let (id, movement) = cell.get();
            (id == self as *const Self as usize).then(|| {
                cell.set((0, [0.0; 3]));
                let pick = |v: f64, h: f64| if v.is_nan() { h } else { v };
                horizon_core::geometry::Vec3::new(
                    pick(movement[0], host.x),
                    pick(movement[1], host.y),
                    pick(movement[2], host.z),
                )
            })
        })
    }
    fn vape_rs_set_inv_walk(&self, keys: Option<crate::sprint::Keys>) {
        let id = self as *const Self as usize;
        INV_WALK.with(|cell| match keys {
            Some(keys) => cell.set((id, Some(keys))),
            None if cell.get().0 == id => cell.set((0, None)),
            None => {}
        });
    }
    fn vape_rs_take_inv_walk(&self) -> Option<crate::sprint::Keys> {
        INV_WALK.with(|cell| {
            let (id, keys) = cell.get();
            (id == self as *const Self as usize).then(|| {
                cell.set((0, None));
                keys
            })?
        })
    }
    fn vape_rs_set_no_slowdown_sprint(&self, on: bool) {
        mark(&SPRINT, self, on);
    }
    fn vape_rs_no_slowdown_sprint(&self) -> bool {
        marked(&SPRINT, self)
    }
}

fn mark(cell: &'static std::thread::LocalKey<Cell<usize>>, value: &impl Sized, on: bool) {
    let id = value as *const _ as *const () as usize;
    cell.with(|cell| {
        if on {
            cell.set(id)
        } else if cell.get() == id {
            cell.set(0)
        }
    });
}
fn marked(cell: &'static std::thread::LocalKey<Cell<usize>>, value: &impl Sized) -> bool {
    let id = value as *const _ as *const () as usize;
    cell.with(|cell| cell.get() == id)
}

pub trait MotionWorldExt {
    fn vape_rs_surface(&mut self) -> (bool, bool);
}
impl MotionWorldExt for crate::motion::MotionWorld<'_, '_, '_> {
    fn vape_rs_surface(&mut self) -> (bool, bool) {
        let p = self.player.position;
        let climbing = self.climbable().unwrap_or(false);
        let feet = horizon_core::position::BlockPos::containing(p);
        let state = self.scene.block(feet.x, feet.y, feet.z);
        let name = self
            .assets
            .registry
            .state(state)
            .map(|s| s.name.as_str())
            .unwrap_or("");
        let vine = climbing && name.trim_start_matches("minecraft:") == "vine";
        let below = horizon_core::position::BlockPos::containing(
            horizon_core::geometry::Vec3::new(p.x, p.y - 0.1, p.z),
        );
        let ice = self
            .resources
            .blocks
            .get(self.scene.block(below.x, below.y, below.z))
            .is_ok_and(|b| b.coefficients.friction >= 0.98);
        (vine, ice)
    }
}

pub trait GameExt {
    fn guest_hit(&self) -> Option<(u32, f64)>;
}
impl GameExt for crate::game::Game<'_> {
    fn guest_hit(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        let host = self.lan.as_ref()?;
        let mut nearest = None;
        for guest in host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead)
        {
            let d = guest.dimensions();
            let half = f64::from(d.width) * 0.5;
            let p = guest.position;
            let Some(t) = ray_box(
                eye,
                dir,
                6.0,
                [p[0] - half, p[1], p[2] - half],
                [p[0] + half, p[1] + f64::from(d.height), p[2] + half],
            ) else {
                continue;
            };
            if nearest.is_none_or(|(_, best)| t < best) {
                nearest = Some((guest.conn, t));
            }
        }
        nearest
    }
}

fn ray_box(eye: [f64; 3], dir: [f64; 3], reach: f64, min: [f64; 3], max: [f64; 3]) -> Option<f64> {
    let (mut lo, mut hi) = (0.0_f64, reach);
    for axis in 0..3 {
        if dir[axis].abs() < 1.0e-9 {
            if eye[axis] < min[axis] || eye[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - eye[axis]) / dir[axis];
            let b = (max[axis] - eye[axis]) / dir[axis];
            lo = lo.max(a.min(b));
            hi = hi.min(a.max(b));
            if hi < lo {
                return None;
            }
        }
    }
    Some(lo)
}

pub trait FixedStepExt {
    fn vape_rs_set_timer_speed(&self, speed: f32);
    fn vape_rs_timer_speed(&self) -> Option<f32>;
    fn vape_rs_timer_steps(&self, steps: u32) -> u32;
    fn vape_rs_advance_scaled(
        &mut self,
        elapsed: std::time::Duration,
        target_millis: f32,
        speed: f32,
    ) -> u32;
}
impl FixedStepExt for crate::clock::FixedStep {
    fn vape_rs_set_timer_speed(&self, speed: f32) {
        let id = self as *const Self as usize;
        TIMER.with(|cell| {
            if speed != 1.0 {
                cell.set((id, speed))
            } else if cell.get().0 == id {
                cell.set((0, 1.0))
            }
        });
        if speed == 1.0 {
            TIMER_FRACTION.with(|cell| {
                if cell.get().0 == id {
                    cell.set((id, 0.0));
                }
            });
        }
    }
    fn vape_rs_timer_speed(&self) -> Option<f32> {
        if TIMER_REENTRY.with(Cell::get) {
            return None;
        }
        TIMER.with(|cell| {
            let (id, speed) = cell.get();
            (id == self as *const Self as usize && speed != 1.0).then_some(speed)
        })
    }
    fn vape_rs_timer_steps(&self, steps: u32) -> u32 {
        let speed = self.vape_rs_timer_speed().unwrap_or(1.0).max(0.0);
        let id = self as *const Self as usize;
        TIMER_FRACTION.with(|cell| {
            let (owner, fraction) = cell.get();
            let fraction = if owner == id { fraction } else { 0.0 };
            let ticks = (steps as f32 * speed + fraction).floor();
            let ticks = ticks.clamp(0.0, u32::MAX as f32) as u32;
            cell.set((id, steps as f32 * speed + fraction - ticks as f32));
            ticks
        })
    }
    fn vape_rs_advance_scaled(
        &mut self,
        elapsed: std::time::Duration,
        target_millis: f32,
        speed: f32,
    ) -> u32 {
        TIMER_REENTRY.with(|cell| cell.set(true));
        let ticks = self.advance(elapsed.mul_f32(speed.max(0.0)), target_millis);
        TIMER_REENTRY.with(|cell| cell.set(false));
        ticks
    }
}
