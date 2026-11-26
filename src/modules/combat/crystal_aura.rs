// CrystalAura's explosion math and block helpers (reference `CrystalAura`).
//
// The reference's placement/target submodules run on top of these: the
// damage model decides whether a placement or break is safe, and the
// interaction point is the ray the rotation aims at.

/// The reference's `ExplosionType` powers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplosionType {
    Crystal,
    Anchor,
    Bed,
}

impl ExplosionType {
    /// The reference's `getExplosionPower`.
    pub fn power(self) -> f32 {
        match self {
            Self::Crystal => 6.0,
            Self::Anchor | Self::Bed => 5.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Crystal => "Crystal",
            Self::Anchor => "Anchor",
            Self::Bed => "Bed",
        }
    }
}

/// The reference's `reduceDamageByArmor`.
pub fn reduce_damage_by_armor(damage: f32, armor: f32, armor_toughness: f32) -> f32 {
    let toughness_scale = 2.0 + armor_toughness / 4.0;
    let effective = (armor - damage / toughness_scale).clamp(armor * 0.2, 20.0);
    damage * (1.0 - effective / 25.0)
}

/// The reference's `reduceDamageByBlastProtection`.
pub fn reduce_damage_by_blast_protection(damage: f32, protection_level: f32) -> f32 {
    damage * (1.0 - protection_level.clamp(0.0, 20.0) / 25.0)
}

/// The reference's `calculateRawExplosionDamage`.
pub fn raw_explosion_damage(diameter: f32, normalized_distance: f64, block_density: f32) -> f32 {
    let impact = (1.0 - normalized_distance) * f64::from(block_density);
    ((impact * impact + impact) / 2.0 * 8.0 * f64::from(diameter) + 1.0) as f32
}

/// The reference's `calculateBlockDensity`: the unobstructed sample fraction
/// from the explosion centre to the target's bounding box.
pub fn block_density(
    min: [f64; 3],
    max: [f64; 3],
    mut unobstructed: impl FnMut([f64; 3]) -> bool,
) -> f32 {
    let x_step = 1.0 / ((max[0] - min[0]) * 2.0 + 1.0);
    let y_step = 1.0 / ((max[1] - min[1]) * 2.0 + 1.0);
    let z_step = 1.0 / ((max[2] - min[2]) * 2.0 + 1.0);
    if x_step < 0.0 || y_step < 0.0 || z_step < 0.0 {
        return 0.0;
    }
    let x_offset = (1.0 - (1.0 / x_step).floor() * x_step) / 2.0;
    let z_offset = (1.0 - (1.0 / z_step).floor() * z_step) / 2.0;
    let mut clear = 0u32;
    let mut total = 0u32;
    let mut x_fraction = 0.0;
    while x_fraction <= 1.0 {
        let mut y_fraction = 0.0;
        while y_fraction <= 1.0 {
            let mut z_fraction = 0.0;
            while z_fraction <= 1.0 {
                let point = [
                    min[0] + (max[0] - min[0]) * x_fraction + x_offset,
                    min[1] + (max[1] - min[1]) * y_fraction,
                    min[2] + (max[2] - min[2]) * z_fraction + z_offset,
                ];
                if unobstructed(point) {
                    clear += 1;
                }
                total += 1;
                z_fraction += z_step;
            }
            y_fraction += y_step;
        }
        x_fraction += x_step;
    }
    if total == 0 { 0.0 } else { clear as f32 / total as f32 }
}

/// The reference's `calculateExplosionDamage` at the normalized distance.
pub fn explosion_damage(
    explosion_type: ExplosionType,
    explosion: [f64; 3],
    eye: [f64; 3],
    block_density: f32,
    armor: f32,
    armor_toughness: f32,
    blast_protection: f32,
) -> f32 {
    let diameter = explosion_type.power() * 2.0;
    let dx = eye[0] - explosion[0];
    let dy = eye[1] - explosion[1];
    let dz = eye[2] - explosion[2];
    let normalized = (dx * dx + dy * dy + dz * dz).sqrt() / f64::from(diameter);
    if normalized > 1.0 {
        return -1.0;
    }
    let raw = raw_explosion_damage(diameter, normalized, block_density);
    let reduced = reduce_damage_by_armor(raw, armor, armor_toughness);
    reduce_damage_by_blast_protection(reduced, blast_protection).max(0.0)
}

/// The reference's `isSelfDamageSafe`.
pub fn self_damage_safe(
    damage: f32,
    health: f32,
    absorption: f32,
    prevent_suicide: bool,
    maximum_damage: f64,
) -> bool {
    if damage <= 0.0 {
        return true;
    }
    if prevent_suicide && health + absorption <= damage {
        return false;
    }
    f64::from(damage) <= maximum_damage
}

/// The reference's `isCrystalBaseBlock`.
pub fn is_crystal_base(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("obsidian") || name.contains("bedrock")
}

/// The reference's `isEndCrystal` by the item's resource key.
pub fn is_end_crystal(id: &str) -> bool {
    id == "minecraft:end_crystal" || id == "end_crystal"
}

/// The reference's `isObsidian`.
pub fn is_obsidian(id: &str) -> bool {
    id == "minecraft:obsidian" || id == "obsidian"
}

/// The reference's `getInteractionPoint` for a block and its facing index
/// (0 down, 1 up, 2 north, 3 south, 4 west, 5 east).
pub fn interaction_point(block: [i32; 3], facing: u8) -> [f64; 3] {
    let (x, y, z) = (
        f64::from(block[0]) + 0.5,
        f64::from(block[1]),
        f64::from(block[2]) + 0.5,
    );
    match facing {
        0 => [x, y, z],
        1 => [f64::from(block[0]) + 0.5, y + 1.0, z],
        2 => [x, y + 0.75, f64::from(block[2])],
        3 => [x, y + 0.75, f64::from(block[2]) + 1.0],
        4 => [f64::from(block[0]), y + 0.75, z],
        5 => [f64::from(block[0]) + 1.0, y + 0.75, z],
        _ => [x, y + 1.0, z],
    }
}

/// The reference's `CrystalAuraActionState`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Idle,
    PlacingObsidian,
    PlacingCrystal,
    BreakingCrystal,
}

/// The reference's `CrystalAuraAction`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    PlacingObsidian,
    PlacingCrystal,
    AttackingCrystal,
}

/// The reference's `CrystalAuraActionCandidate`: the target block and the
/// action chosen for it, with the damage efficiency that justified it.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub block: [i32; 3],
    pub facing: u8,
    pub action: Action,
    /// The damage efficiency (the reference's `getDamage()`); 0 for a
    /// placement-only action.
    pub damage: f64,
}

/// The reference's `pendingRemoveEntityId`: the crystal id whose removal the
/// next attack packet predicts, so a dead crystal is not re-attacked.
#[derive(Clone, Copy, Debug, Default)]
pub struct Prediction {
    pub attacked: bool,
    pub pending_remove: i32,
}

impl Prediction {
    pub fn reset(&mut self) {
        self.attacked = false;
        self.pending_remove = -1;
    }
}

/// The reference's `computeDamageEfficiency` bounds: the integer range the
/// explosion search covers around the explosion centre.
pub fn explosion_bounds(power: f32, centre: [f64; 3]) -> ([i32; 3], [i32; 3]) {
    let reach = f64::from(power) * 2.0 + 1.0;
    let low = [
        (centre[0] - reach).floor() as i32,
        (centre[1] - reach).floor() as i32,
        (centre[2] - reach).floor() as i32,
    ];
    let high = [
        (centre[0] + reach).floor() as i32,
        (centre[1] + reach).floor() as i32,
        (centre[2] + reach).floor() as i32,
    ];
    (low, high)
}

/// Whether a predicted position sits inside the explosion bounds.
pub fn inside_bounds(position: [f64; 3], low: [i32; 3], high: [i32; 3]) -> bool {
    let floors = [
        position[0].floor() as i32,
        position[1].floor() as i32,
        position[2].floor() as i32,
    ];
    (low[0]..=high[0]).contains(&floors[0])
        && (low[1]..=high[1]).contains(&floors[1])
        && (low[2]..=high[2]).contains(&floors[2])
}

/// The reference's `CrystalAuraTargetSubModule` state ledger: the target
/// block, the tracked crystal and the carried slot.
#[derive(Clone, Copy, Debug, Default)]
pub struct Target {
    pub state: State,
    pub block: Option<[i32; 3]>,
    pub tracked: Option<i32>,
    pub crystal_slot: i32,
    pub saved_slot: i32,
    pub prediction: Prediction,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

impl Target {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            block: None,
            tracked: None,
            crystal_slot: -1,
            saved_slot: -1,
            prediction: Prediction::default(),
        }
    }

    /// The reference's `setState`: reset the per-state fields on entry and
    /// drop the target on returning to idle.
    pub fn set_state(&mut self, state: State) {
        if self.state == state {
            return;
        }
        self.state = state;
        if state == State::Idle {
            self.block = None;
            self.tracked = None;
            self.crystal_slot = -1;
        }
    }

    /// The reference's `clearTracking`.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// The reference's `handlePlacingCrystal` entry: an existing crystal
    /// above the base moves the state to breaking instead of placing.
    pub fn placement_result(&self, existing_crystal: Option<i32>) -> (State, Option<i32>) {
        match existing_crystal {
            Some(id) => (State::BreakingCrystal, Some(id)),
            None => (State::PlacingCrystal, None),
        }
    }
}

/// The reference's `isAlive`.
pub fn is_alive(health: f32, removed: bool) -> bool {
    !removed && health > 0.0
}

/// The reference's `isInRange`.
pub fn in_range(distance: f64, range: f64) -> bool {
    distance <= range
}

/// The reference's `statusPriority`: the highest priority wins across the
/// two y-level searches.
pub fn status_priority(status: Option<&str>) -> u8 {
    match status {
        None => 0,
        Some("low eff") => 6,
        Some("blocked" | "out range" | "no place") => 5,
        Some("no candidate") => 4,
        Some("no placement block") => 3,
        Some("no target") => 2,
        Some(_) => 1,
    }
}

/// The reference's target-mode comparators (`targetMode`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetMode {
    Yaw,
    Distance,
    Armor,
    Health,
}

/// A candidate's sort keys, filled from the entity the reference sorts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetScore {
    pub angle: f64,
    pub distance: f64,
    pub armor: f64,
    pub health: f64,
}

impl TargetScore {
    /// The reference's comparator key: ascending for every mode.
    pub fn key(self, mode: TargetMode) -> f64 {
        match mode {
            TargetMode::Yaw => self.angle,
            TargetMode::Distance => self.distance,
            TargetMode::Armor => self.armor,
            TargetMode::Health => self.health,
        }
    }
}

/// Sort the candidates the way the reference's target mode does.
pub fn sort_targets(scores: &mut [TargetScore], mode: TargetMode) {
    scores.sort_by(|a, b| {
        a.key(mode)
            .partial_cmp(&b.key(mode))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// The reference's `TARGET_MEMORY_MS`: the last target stays the active one
/// for 1.2 s after it leaves the candidate list.
pub const TARGET_MEMORY_MS: u64 = 1200;

/// The reference's `rememberTarget`/`getActiveTarget` memory.
#[derive(Clone, Copy, Debug, Default)]
pub struct TargetMemory {
    last: Option<(i32, u64)>,
}

impl TargetMemory {
    pub fn remember(&mut self, id: i32, now_ms: u64) {
        self.last = Some((id, now_ms));
    }

    /// The remembered target if it was refreshed within the memory window.
    pub fn get(&self, now_ms: u64) -> Option<i32> {
        self.last
            .filter(|(_, at)| now_ms.saturating_sub(*at) <= TARGET_MEMORY_MS)
            .map(|(id, _)| id)
    }

    pub fn clear(&mut self) {
        self.last = None;
    }
}

/// The reference's `computeDamageEfficiency`: the raw damage over the
/// maximum the same diameter can deal, so candidates rank by fraction.
pub fn damage_efficiency(raw_damage: f32, diameter: f32) -> f32 {
    raw_damage / (8.0 * diameter + 1.0)
}

/// A block-aligned box, as `AxisAlignedBB` in the reference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb {
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min[0] < other.max[0]
            && self.max[0] > other.min[0]
            && self.min[1] < other.max[1]
            && self.max[1] > other.min[1]
            && self.min[2] < other.max[2]
            && self.max[2] > other.min[2]
    }
}

/// The reference's `crystalBoundingBox`: the 1x2x1 space above the base.
pub fn crystal_box(base: [i32; 3]) -> Aabb {
    Aabb {
        min: [f64::from(base[0]), f64::from(base[1]) + 1.0, f64::from(base[2])],
        max: [
            f64::from(base[0]) + 1.0,
            f64::from(base[1]) + 3.0,
            f64::from(base[2]) + 1.0,
        ],
    }
}

/// The reference's two-pass scan order: the first pass only considers the
/// half-plane toward the target (when it is farther than 1.75 blocks), the
/// second considers every offset. The centre is skipped.
pub fn search_offsets(radius: i32, target_close: bool, target_offset: [f64; 2]) -> Vec<[i32; 2]> {
    let mut offsets = Vec::with_capacity(((radius * 2 + 1).pow(2) - 1).max(0) as usize * 2);
    for pass in 0..2 {
        let only_toward = pass == 0 && !target_close;
        for x in -radius..=radius {
            for z in -radius..=radius {
                if x == 0 && z == 0 {
                    continue;
                }
                if only_toward {
                    let toward =
                        target_offset[0] * f64::from(x) + target_offset[1] * f64::from(z);
                    if toward < 0.0 {
                        continue;
                    }
                }
                offsets.push([x, z]);
            }
        }
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armor_reduction_never_exceeds_the_raw_damage() {
        let raw = raw_explosion_damage(12.0, 0.25, 1.0);
        let reduced = reduce_damage_by_armor(raw, 20.0, 8.0);
        assert!(reduced <= raw && reduced >= 0.0);
    }

    #[test]
    fn a_fully_obstructed_explosion_has_no_density() {
        let density = block_density([0.0; 3], [1.0; 3], |_| false);
        assert_eq!(density, 0.0);
    }

    #[test]
    fn a_clear_explosion_has_full_density() {
        let density = block_density([0.0; 3], [1.0; 3], |_| true);
        assert_eq!(density, 1.0);
    }

    #[test]
    fn suicide_is_rejected_only_when_it_would_kill() {
        assert!(!self_damage_safe(20.0, 12.0, 0.0, true, 100.0));
        assert!(self_damage_safe(20.0, 12.0, 0.0, false, 100.0));
        assert!(!self_damage_safe(5.0, 12.0, 0.0, false, 4.0));
    }

    #[test]
    fn a_position_just_outside_the_bounds_is_excluded() {
        let (low, high) = explosion_bounds(6.0, [0.0; 3]);
        assert_eq!(low, [-13; 3]);
        assert_eq!(high, [13; 3]);
        assert!(inside_bounds([13.9, -12.6, 0.0], low, high));
        assert!(!inside_bounds([14.1, 0.0, 0.0], low, high));
    }

    #[test]
    fn placement_prediction_resets() {
        let mut prediction = Prediction {
            attacked: true,
            pending_remove: 7,
        };
        prediction.reset();
        assert!(!prediction.attacked);
        assert_eq!(prediction.pending_remove, -1);
    }

    #[test]
    fn an_existing_crystal_switches_to_breaking() {
        let mut target = Target::new();
        target.block = Some([1, 2, 3]);
        target.set_state(State::PlacingCrystal);
        assert_eq!(
            target.placement_result(Some(9)),
            (State::BreakingCrystal, Some(9))
        );
        assert_eq!(target.placement_result(None), (State::PlacingCrystal, None));
    }

    #[test]
    fn returning_to_idle_drops_the_target() {
        let mut target = Target::new();
        target.block = Some([1, 2, 3]);
        target.tracked = Some(9);
        target.set_state(State::BreakingCrystal);
        target.set_state(State::Idle);
        assert!(target.block.is_none() && target.tracked.is_none());
        assert_eq!(target.crystal_slot, -1);
    }

    #[test]
    fn status_priority_prefers_low_efficiency_over_range() {
        assert!(status_priority(Some("low eff")) > status_priority(Some("out range")));
        assert_eq!(status_priority(None), 0);
        assert_eq!(status_priority(Some("unknown")), 1);
    }

    #[test]
    fn a_dead_or_removed_target_is_not_alive() {
        assert!(!is_alive(0.0, false));
        assert!(!is_alive(20.0, true));
        assert!(is_alive(20.0, false));
        assert!(in_range(4.0, 4.0));
        assert!(!in_range(4.5, 4.0));
    }

    #[test]
    fn distance_mode_sorts_nearest_first() {
        let mut scores = [
            TargetScore {
                angle: 10.0,
                distance: 5.0,
                armor: 2.0,
                health: 20.0,
            },
            TargetScore {
                angle: 1.0,
                distance: 2.0,
                armor: 9.0,
                health: 8.0,
            },
        ];
        sort_targets(&mut scores, TargetMode::Distance);
        assert_eq!(scores[0].distance, 2.0);
        sort_targets(&mut scores, TargetMode::Yaw);
        assert_eq!(scores[0].angle, 1.0);
        sort_targets(&mut scores, TargetMode::Armor);
        assert_eq!(scores[0].armor, 2.0);
        sort_targets(&mut scores, TargetMode::Health);
        assert_eq!(scores[0].health, 8.0);
    }

    #[test]
    fn the_target_is_remembered_for_one_point_two_seconds() {
        let mut memory = TargetMemory::default();
        memory.remember(7, 1_000);
        assert_eq!(memory.get(2_200), Some(7));
        assert_eq!(memory.get(2_201), None);
    }

    #[test]
    fn efficiency_is_the_fraction_of_the_maximum_damage() {
        let diameter = 12.0;
        let maximum = 8.0 * diameter + 1.0;
        assert!((damage_efficiency(maximum, diameter) - 1.0).abs() < 1e-6);
        assert!(damage_efficiency(maximum / 2.0, diameter) < 1.0);
        assert_eq!(damage_efficiency(0.0, diameter), 0.0);
    }

    #[test]
    fn the_crystal_box_sits_above_its_base() {
        let crystal = crystal_box([2, 5, -1]);
        assert_eq!(crystal.min, [2.0, 6.0, -1.0]);
        assert_eq!(crystal.max, [3.0, 8.0, 0.0]);
        assert!(crystal.intersects(&Aabb {
            min: [2.5, 6.5, -0.5],
            max: [2.6, 6.6, -0.4],
        }));
        assert!(!crystal.intersects(&Aabb {
            min: [2.5, 8.0, -0.5],
            max: [2.6, 9.0, -0.4],
        }));
    }

    #[test]
    fn a_far_target_scans_toward_it_first_and_skips_the_centre() {
        let offsets = search_offsets(1, false, [1.0, 0.0]);
        // 8 offsets per pass, the centre skipped; the first pass keeps only
        // the 5 with a non-negative x.
        assert_eq!(offsets.len(), 13);
        assert!(!offsets.contains(&[0, 0]));
        assert!(offsets[..5].iter().all(|offset| offset[0] >= 0));
        assert!(offsets[5..].iter().any(|offset| offset[0] < 0));
        // A close target applies the filter to both passes.
        let close = search_offsets(1, true, [1.0, 0.0]);
        assert_eq!(close.len(), 16);
        assert!(close[..8].iter().any(|offset| offset[0] < 0));
    }
}