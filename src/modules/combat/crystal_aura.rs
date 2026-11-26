//! CrystalAura's explosion math and block helpers (reference `CrystalAura`).
//!
//! The reference's placement/target submodules run on top of these: the
//! damage model decides whether a placement or break is safe, and the
//! interaction point is the ray the rotation aims at.

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
}