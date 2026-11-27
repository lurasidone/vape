// Lines drawn through walls. The world-lines hook splits each frame's module
// lines: the depth-tested ones stay in the fishing line's set, and the
// through-wall ones (StorageESP, ESP, Tracers, Trajectories, Search,
// PropHunt, Projectiles) are kept here. The debug-gizmos hook then appends
// them to the frame's always-on-top gizmo set, which the client draws over
// everything.
//
// This lives in the game crate (the fishing module), not in `gizmos`: the
// prebuilt game route only mirrors support modules into game-crate files, and
// a library-crate helper has no way back into the game crate.
use core::cell::RefCell;
use horizon_client_lib::gizmos::{Gizmos, Line};

thread_local! {
    static THROUGH: RefCell<Vec<Line>> = const { RefCell::new(Vec::new()) };
}

/// Keep this frame's through-wall lines for the on-top gizmo set.
pub fn stash(lines: Vec<Line>) {
    THROUGH.with(|t| *t.borrow_mut() = lines);
}

/// Move this frame's through-wall lines into the gizmos' on-top set.
pub fn extend(mut gizmos: Gizmos) -> Gizmos {
    let lines = THROUGH.with(|t| core::mem::take(&mut *t.borrow_mut()));
    gizmos.on_top.extend(lines);
    gizmos
}