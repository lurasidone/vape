// BedBreaker (reference `BedBreaker`): head-half bed targets, the Hypixel
// cover selector, and the per-target render/break state. The host supplies
// the block and hotbar queries through `WorldView`; this module owns the
// reference's selection rules.
use super::super::super::core::settings::{Definition, Settings};
use serde_json::Value;

pub const ID: &str = "world.bed-breaker";

pub fn definitions() -> Vec<Definition> {
    vec![Definition::choice(
        &format!("{ID}.mode"),
        "Mode",
        &["Normal", "Hypixel"],
        "Normal",
    )]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Hypixel,
}
impl Mode {
    pub fn read(settings: &Settings) -> Self {
        match settings.get(&format!("{ID}.mode")).and_then(Value::as_str) {
            Some("Hypixel") => Self::Hypixel,
            _ => Self::Normal,
        }
    }
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Hypixel => "Hypixel",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BedBreakPhase {
    Unresolved,
    Cover,
    Bed,
}

/// `EnumFacing.getIndex()` 2..5 for the bed's facing; the offset points from
/// the head half to the foot half.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    South,
    West,
    North,
    East,
}
impl Facing {
    pub fn offset(self) -> [i32; 3] {
        match self {
            Self::South => [0, 0, 1],
            Self::West => [0, 0, -1],
            Self::North => [1, 0, 0],
            Self::East => [-1, 0, 0],
        }
    }
}

/// One bed half at its exact block coordinates (the reference keeps doubles
/// for the render position and floors them for block queries).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BedTargetPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl BedTargetPosition {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self {
            x: f64::from(x),
            y: f64::from(y),
            z: f64::from(z),
        }
    }
    pub fn block_x(self) -> i32 {
        self.x.floor() as i32
    }
    pub fn block_y(self) -> i32 {
        self.y.floor() as i32
    }
    pub fn block_z(self) -> i32 {
        self.z.floor() as i32
    }
    pub fn block(self) -> [i32; 3] {
        [self.block_x(), self.block_y(), self.block_z()]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BedCoverTarget {
    pub position: [i32; 3],
    /// The hotbar stack the best progress was measured with, if any.
    pub tool: Option<String>,
    pub tool_slot: usize,
}

#[derive(Clone, Debug)]
pub struct BedTargetRenderState {
    pub position: BedTargetPosition,
    pub visibility: f32,
    pub inside_reticle: bool,
    pub obstruction: Option<[f64; 3]>,
    pub phase: BedBreakPhase,
    pub cover: Option<BedCoverTarget>,
    last_animation_ms: u64,
}
impl BedTargetRenderState {
    pub fn new(position: BedTargetPosition) -> Self {
        Self {
            position,
            visibility: 0.0,
            inside_reticle: false,
            obstruction: None,
            phase: BedBreakPhase::Unresolved,
            cover: None,
            last_animation_ms: 0,
        }
    }
    pub fn set_obstruction(&mut self, point: Option<[f64; 3]>) {
        self.obstruction = point;
    }
    pub fn resolve_cover(&mut self, cover: Option<BedCoverTarget>) {
        self.phase = if cover.is_some() {
            BedBreakPhase::Cover
        } else {
            BedBreakPhase::Bed
        };
        self.cover = cover;
    }
    pub fn finish_cover(&mut self) {
        self.cover = None;
        self.phase = BedBreakPhase::Bed;
    }
    pub fn reset_break_state(&mut self) {
        self.cover = None;
        self.phase = BedBreakPhase::Unresolved;
        self.obstruction = None;
    }
    /// `updateVisibilityAnimation`: a 10 ms timer moves 0.05 toward the
    /// reticle state, clamped to [0, 1].
    pub fn update_visibility_animation(&mut self, now_ms: u64) {
        if now_ms.saturating_sub(self.last_animation_ms) < 10 {
            return;
        }
        let step = if self.inside_reticle { 0.05 } else { -0.05 };
        self.visibility = (self.visibility + step).clamp(0.0, 1.0);
        self.last_animation_ms = now_ms;
    }
}

/// The block and hotbar queries the cover selector needs. `block` returns the
/// namespaced block id (`minecraft:red_bed`); `destroy_progress` is
/// `BlockState.getPlayerRelativeDestroyProgress` with the stack in `slot`.
pub trait WorldView {
    fn block(&self, pos: [i32; 3]) -> Option<&str>;
    fn bed_facing(&self, pos: [i32; 3]) -> Option<Facing>;
    fn replaceable(&self, pos: [i32; 3]) -> bool;
    fn destroy_progress(&self, pos: [i32; 3], slot: usize) -> f32;
    fn hotbar(&self, slot: usize) -> Option<String>;
}

pub fn is_bed(block: &str) -> bool {
    block
        .trim_start_matches("minecraft:")
        .ends_with("_bed")
}

/// `BedCoverTargetSelector.select`: the cover block with the best destroy
/// progress, or `None` when the bed is exposed or no cover exists.
pub struct BedCoverTargetSelector;
impl BedCoverTargetSelector {
    pub fn select(world: &impl WorldView, bed: [i32; 3]) -> Option<BedCoverTarget> {
        let block = world.block(bed)?;
        if !is_bed(block) {
            return None;
        }
        let facing = world.bed_facing(bed)?;
        let cover_positions = first_layer_positions(bed, facing);
        if cover_positions.iter().any(|pos| world.replaceable(*pos)) {
            return None;
        }
        let mut best: Option<(BedCoverTarget, f32)> = None;
        for position in cover_positions {
            for slot in 0..9 {
                let progress = world.destroy_progress(position, slot);
                if best.as_ref().is_some_and(|(_, best)| progress <= *best) {
                    continue;
                }
                best = Some((
                    BedCoverTarget {
                        position,
                        tool: world.hotbar(slot),
                        tool_slot: slot,
                    },
                    progress,
                ));
            }
        }
        best.map(|(target, _)| target)
    }
}

/// `getFirstLayerPositions`: the four sides and the top of both bed halves,
/// without the two bed blocks themselves.
fn first_layer_positions(bed: [i32; 3], facing: Facing) -> Vec<[i32; 3]> {
    let other = [
        bed[0] + facing.offset()[0],
        bed[1],
        bed[2] + facing.offset()[2],
    ];
    let mut positions = Vec::with_capacity(10);
    let mut push = |pos: [i32; 3]| {
        if pos != bed && pos != other && !positions.contains(&pos) {
            positions.push(pos);
        }
    };
    for half in [bed, other] {
        push([half[0], half[1] + 1, half[2]]);
        push([half[0] - 1, half[1], half[2]]);
        push([half[0] + 1, half[1], half[2]]);
        push([half[0], half[1], half[2] - 1]);
        push([half[0], half[1], half[2] + 1]);
    }
    positions
}
