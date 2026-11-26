// Runtime side of BedBreaker: the target list, the reticle selection and the
// Hypixel cover phase. The host scans beds and projects their centres; this
// keeps the reference's last-inside-reticle selection and cover lifecycle.
use super::super::modules::world::bed_breaker::{
    BedBreakPhase, BedCoverTargetSelector, BedTargetPosition, BedTargetRenderState, Mode, WorldView,
};

#[derive(Default)]
pub struct BedBreaker {
    targets: Vec<BedTargetRenderState>,
    selected: Option<usize>,
    /// The last selected target whose break progress was read.
    last_progress: Option<usize>,
    /// The cover block the Hypixel server rotation points at.
    server_rotation: Option<[i32; 3]>,
}

/// What the host acts on this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// The head-half bed block (the scan target).
    pub bed: [i32; 3],
    /// The block to claim under the crosshair: the cover in Hypixel's cover
    /// phase, otherwise the bed itself.
    pub block: [i32; 3],
    /// The hotbar slot holding the best cover tool, when a cover is active.
    pub tool_slot: Option<usize>,
    pub phase: BedBreakPhase,
}

impl BedBreaker {
    pub fn reset(&mut self) {
        self.targets.clear();
        self.selected = None;
        self.last_progress = None;
        self.server_rotation = None;
    }

    /// The reference's `onRender3D`/`onBedBreakerUpdate` pair: refresh the
    /// targets, animate visibility, select the last target inside the
    /// reticle, and resolve/finish the Hypixel cover.
    pub fn tick(
        &mut self,
        world: &impl WorldView,
        beds: &[[i32; 3]],
        inside_reticle: &[bool],
        mode: Mode,
        now_ms: u64,
    ) -> Option<Target> {
        self.targets
            .retain(|target| beds.contains(&target.position.block()));
        for (index, bed) in beds.iter().enumerate() {
            let position = BedTargetPosition::new(bed[0], bed[1], bed[2]);
            let state = if let Some(state) = self
                .targets
                .iter_mut()
                .find(|state| state.position == position)
            {
                state
            } else {
                self.targets.push(BedTargetRenderState::new(position));
                self.targets.last_mut().expect("just pushed")
            };
            state.inside_reticle = inside_reticle.get(index).copied().unwrap_or(false);
            state.update_visibility_animation(now_ms);
        }
        self.selected = self.targets.iter().rposition(|state| state.inside_reticle);
        let Some(index) = self.selected else {
            self.last_progress = None;
            self.server_rotation = None;
            return None;
        };
        if mode == Mode::Normal {
            // `onModeChanged` resets every render state when Hypixel is left.
            for state in &mut self.targets {
                state.reset_break_state();
            }
        }
        if mode == Mode::Hypixel {
            let state = &mut self.targets[index];
            if state.phase == BedBreakPhase::Unresolved {
                let cover = BedCoverTargetSelector::select(world, state.position.block());
                state.resolve_cover(cover);
            }
            if let Some(cover) = state.cover.clone()
                && world.replaceable(cover.position)
            {
                state.finish_cover();
                self.last_progress = None;
            }
        }
        let state = &self.targets[index];
        let (block, tool_slot, phase) = match (&state.cover, mode) {
            (Some(cover), Mode::Hypixel) if state.phase == BedBreakPhase::Cover => {
                (cover.position, Some(cover.tool_slot), state.phase)
            }
            _ => (state.position.block(), None, state.phase),
        };
        self.last_progress = Some(index);
        self.server_rotation =
            (mode == Mode::Hypixel && phase == BedBreakPhase::Cover).then_some(block);
        Some(Target {
            bed: state.position.block(),
            block,
            tool_slot,
            phase,
        })
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }
    pub fn last_progress(&self) -> Option<usize> {
        self.last_progress
    }
    pub fn server_rotation(&self) -> Option<[i32; 3]> {
        self.server_rotation
    }
}
