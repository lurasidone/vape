// CrystalAura's explosion math and block helpers (reference `CrystalAura`).
//
// The reference's placement/target submodules run on top of these: the
// damage model decides whether a placement or break is safe, and the
// interaction point is the ray the rotation aims at.
use super::super::super::core::settings::{Definition, Kind, Settings};
use serde_json::{Value, json};

pub const ID: &str = "combat.crystal-aura";

/// The placement click waits this long for its crystal before it may click
/// again (`placementSent`); the reference's use delay is four ticks.
pub const PLACEMENT_TIMEOUT_MS: u64 = 500;

pub fn definitions() -> Vec<Definition> {
    vec![
        Definition::choice(&format!("{ID}.mode"), "Mode", &["Auto", "Manual"], "Auto"),
        Definition::number(&format!("{ID}.range"), "Range", 1.0, 4.5, 6.0, 0.1),
        Definition::number(
            &format!("{ID}.max-self-damage"),
            "Max self damage",
            0.0,
            19.0,
            20.0,
            1.0,
        ),
        Definition::boolean(&format!("{ID}.anti-suicide"), "Anti suicide", true),
        Definition::number(&format!("{ID}.aim-speed"), "Aim speed", 1.0, 4.5, 10.0, 0.1),
        // The reference's `activationDelay` (`Delay`, 50-150 ms by default,
        // re-drawn every tick) before the first action.
        Definition::new(
            &format!("{ID}.delay"),
            "Delay",
            Kind::Range {
                min: 50.0,
                max: 500.0,
                step: 1.0,
            },
            json!([50.0, 150.0]),
        ),
        // The reference manual submodule's `delay` (0-100 ms by default)
        // between a completed break/place cycle and the next one.
        Definition::new(
            &format!("{ID}.action-delay"),
            "Action delay",
            Kind::Range {
                min: 0.0,
                max: 500.0,
                step: 1.0,
            },
            json!([0.0, 100.0]),
        ),
    ]
}

/// The reference's `Mode` (Auto/Manual submodules).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Auto,
    Manual,
}

impl Mode {
    pub fn read(settings: &Settings) -> Self {
        match settings.get(&format!("{ID}.mode")).and_then(Value::as_str) {
            Some("Manual") => Self::Manual,
            _ => Self::Auto,
        }
    }
}

/// The auto placement loop's settings, read once per tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub mode: Mode,
    pub range: f64,
    pub max_self_damage: f64,
    pub anti_suicide: bool,
    pub aim_speed: f64,
    /// The activation delay range (`RandomValue.getRandomValue`).
    pub delay: (f64, f64),
    /// The delay between completed break/place cycles.
    pub action_delay: (f64, f64),
}

impl Options {
    pub fn read(settings: &Settings) -> Self {
        let number = |suffix: &str, default: f64| {
            settings
                .get(&format!("{ID}.{suffix}"))
                .and_then(Value::as_f64)
                .unwrap_or(default)
        };
        let range = |suffix: &str, default: (f64, f64)| {
            let value = settings
                .get(&format!("{ID}.{suffix}"))
                .and_then(Value::as_array);
            let at = |i: usize, d: f64| {
                value
                    .as_ref()
                    .and_then(|v| v.get(i))
                    .and_then(Value::as_f64)
                    .unwrap_or(d)
            };
            (at(0, default.0), at(1, default.1))
        };
        Self {
            mode: Mode::read(settings),
            range: number("range", 4.5),
            max_self_damage: number("max-self-damage", 19.0),
            anti_suicide: settings
                .get(&format!("{ID}.anti-suicide"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            aim_speed: number("aim-speed", 4.5),
            delay: range("delay", (50.0, 150.0)),
            action_delay: range("action-delay", (0.0, 100.0)),
        }
    }
}

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
    if total == 0 {
        0.0
    } else {
        clear as f32 / total as f32
    }
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

/// The auto placement loop's world inputs for one tick. The host resolves
/// the selector, the crystal above the base and the crosshair along the
/// managed rotation; the loop owns the state and the timers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tick {
    pub now_ms: u64,
    /// An alive mob within range.
    pub has_target: bool,
    /// The selector's safe crystal-base block.
    pub base: Option<[i32; 3]>,
    /// The crystal above `base`, when one exists.
    pub crystal_above: Option<i32>,
    /// The hotbar slot holding an end crystal, -1 when none.
    pub crystal_slot: i32,
    pub selected: i32,
    pub eye: [f64; 3],
    pub view: (f32, f32),
    /// The block under the managed rotation's crosshair, with its facing
    /// index (0 down, 1 up, 2 north, 3 south, 4 west, 5 east).
    pub aimed_block: Option<([i32; 3], u8)>,
    /// The crystal under the managed rotation's crosshair.
    pub aimed_crystal: Option<i32>,
    /// The nearest point on the tracked crystal's box to the eye.
    pub crystal_point: [f64; 3],
    /// No screen is open and the player is alive.
    pub can_act: bool,
}

/// What the host does after a tick: aim the shared rotation at `aim`, switch
/// the carried slot, restore the saved slot on release, press use or attack.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Actions {
    pub aim: Option<[f64; 3]>,
    pub select: Option<i32>,
    pub restore: Option<i32>,
    pub use_now: bool,
    pub attack: Option<i32>,
    /// Release the rotation and the saved slot (reset).
    pub release: bool,
}

/// The reference's `CrystalAuraTargetSubModule` state ledger: the target
/// block, the tracked crystal, the carried slot and the delay/action timers.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub state: State,
    pub block: Option<[i32; 3]>,
    pub tracked: Option<i32>,
    pub crystal_slot: i32,
    pub saved_slot: i32,
    pub prediction: Prediction,
    /// The reference's `delayTimer`: the last reset (activation delay).
    pub delay_at: u64,
    /// The reference's `actionTimer`: the last reset (between cycles).
    pub action_at: u64,
    /// Completed break/place cycles since the last reset.
    pub cycles: u32,
    /// The reference's `placementSent`: a placement click waits for its
    /// crystal.
    pub placement_sent: bool,
    /// When the placement click was sent.
    pub placed_at: u64,
    /// `RandomValue.getRandomValue`'s generator.
    rng: u64,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

impl Default for Target {
    fn default() -> Self {
        Self::new()
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
            delay_at: 0,
            action_at: 0,
            cycles: 0,
            placement_sent: false,
            placed_at: 0,
            rng: 0,
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
            self.placement_sent = false;
        }
    }

    /// The reference's `clearTracking`.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// `clearTracking` plus both timers reset at `now_ms`.
    pub fn reset_at(&mut self, now_ms: u64) {
        self.reset();
        self.delay_at = now_ms;
        self.action_at = now_ms;
    }

    /// The reference's `handlePlacingCrystal` entry: an existing crystal
    /// above the base moves the state to breaking instead of placing.
    pub fn placement_result(&self, existing_crystal: Option<i32>) -> (State, Option<i32>) {
        match existing_crystal {
            Some(id) => (State::BreakingCrystal, Some(id)),
            None => (State::PlacingCrystal, None),
        }
    }

    /// `Random.nextDouble`.
    fn random(&mut self) -> f64 {
        if self.rng == 0 {
            self.rng = 0x2545_f491_4f6c_dd1d;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    /// `(long) RandomValue.getRandomValue()`: uniform in `[lo, hi)`.
    fn draw_delay(&mut self, (lo, hi): (f64, f64)) -> u64 {
        if hi <= lo {
            return lo.max(0.0) as u64;
        }
        (lo + (hi - lo) * self.random()).max(0.0) as u64
    }

    /// `delayTimer.hasTimeElapsed((long) activationDelay.getRandomValue())`.
    pub fn activation_ready(&mut self, now_ms: u64, delay: (f64, f64)) -> bool {
        let threshold = self.draw_delay(delay);
        now_ms.saturating_sub(self.delay_at) >= threshold
    }

    /// `actionTimer.hasTimeElapsed((long) delay.getRandomValue())`.
    pub fn action_ready(&mut self, now_ms: u64, delay: (f64, f64)) -> bool {
        let threshold = self.draw_delay(delay);
        now_ms.saturating_sub(self.action_at) >= threshold
    }

    /// One tick of the reference's auto placement loop: validate the parked
    /// action, start a new one after the activation delay, select the
    /// crystal and act once the managed rotation's crosshair is on it.
    pub fn tick(&mut self, tick: &Tick, options: &Options) -> Actions {
        let mut out = Actions::default();
        if !tick.can_act || !tick.has_target || tick.base.is_none() || tick.crystal_slot < 0 {
            out.restore = (self.saved_slot >= 0).then_some(self.saved_slot);
            out.release = true;
            self.reset_at(tick.now_ms);
            return out;
        }
        let base = tick.base.expect("checked above");

        // The selector moved off this base: drop the parked action.
        if matches!(self.state, State::PlacingCrystal | State::BreakingCrystal)
            && self.block != Some(base)
        {
            self.set_state(State::Idle);
        }
        match self.state {
            State::PlacingCrystal => {
                if let Some(id) = tick.crystal_above {
                    self.tracked = Some(id);
                    self.placement_sent = false;
                    self.set_state(State::BreakingCrystal);
                } else if self.placement_sent
                    && tick.now_ms.saturating_sub(self.placed_at) > PLACEMENT_TIMEOUT_MS
                {
                    // The click did not produce a crystal; allow another.
                    self.placement_sent = false;
                }
            }
            State::BreakingCrystal => match tick.crystal_above {
                Some(id) => self.tracked = Some(id),
                None => {
                    self.tracked = None;
                    self.placement_sent = false;
                    self.set_state(State::PlacingCrystal);
                    self.action_at = tick.now_ms;
                }
            },
            State::Idle | State::PlacingObsidian => {}
        }

        if self.state == State::Idle
            && self.activation_ready(tick.now_ms, options.delay)
            && (self.cycles == 0 || self.action_ready(tick.now_ms, options.action_delay))
        {
            self.block = Some(base);
            match tick.crystal_above {
                Some(id) => {
                    self.tracked = Some(id);
                    self.set_state(State::BreakingCrystal);
                }
                None => self.set_state(State::PlacingCrystal),
            }
        }

        if self.state != State::Idle && tick.selected != tick.crystal_slot {
            if self.saved_slot < 0 {
                self.saved_slot = tick.selected;
            }
            out.select = Some(tick.crystal_slot);
        }

        match self.state {
            State::PlacingCrystal => {
                out.aim = Some(interaction_point(base, 1));
                // The manual submodule's `handlePlacingCrystal` waits the
                // action delay after a break before clicking again.
                if !self.placement_sent
                    && self.action_ready(tick.now_ms, options.action_delay)
                    && tick.aimed_block == Some((base, 1))
                {
                    out.use_now = true;
                    self.placement_sent = true;
                    self.placed_at = tick.now_ms;
                }
            }
            State::BreakingCrystal => {
                out.aim = Some(tick.crystal_point);
                if self.tracked.is_some() && tick.aimed_crystal == self.tracked {
                    out.attack = self.tracked;
                    out.aim = None;
                    self.cycles = self.cycles.saturating_add(1);
                    self.action_at = tick.now_ms;
                    self.placement_sent = false;
                    self.set_state(State::Idle);
                }
            }
            State::Idle | State::PlacingObsidian => {}
        }
        out
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
        min: [
            f64::from(base[0]),
            f64::from(base[1]) + 1.0,
            f64::from(base[2]),
        ],
        max: [
            f64::from(base[0]) + 1.0,
            f64::from(base[1]) + 3.0,
            f64::from(base[2]) + 1.0,
        ],
    }
}

/// `EndCrystal.getBoundingBox`: 2 x 2 x 2 around its position (`width` 2,
/// `height` 2, centred horizontally).
pub fn crystal_entity_box(position: [f64; 3]) -> Aabb {
    Aabb {
        min: [position[0] - 1.0, position[1], position[2] - 1.0],
        max: [position[0] + 1.0, position[1] + 2.0, position[2] + 1.0],
    }
}

/// The reference's `findCrystalAbove`: the crystal whose entity box
/// intersects the 1x2x1 search box above the base.
pub fn crystal_above(
    base: [i32; 3],
    crystals: impl IntoIterator<Item = ([f64; 3], i32)>,
) -> Option<i32> {
    let search = Aabb {
        min: [
            f64::from(base[0]) - 0.5,
            f64::from(base[1]) + 0.99,
            f64::from(base[2]) - 0.5,
        ],
        max: [
            f64::from(base[0]) + 0.5,
            f64::from(base[1]) + 3.0,
            f64::from(base[2]) + 0.5,
        ],
    };
    crystals
        .into_iter()
        .find(|(position, _)| crystal_entity_box(*position).intersects(&search))
        .map(|(_, id)| id)
}

/// `RotationUtil.M(eye, box, 0, 0, 0)`: the nearest point on the box.
pub fn nearest_point(eye: [f64; 3], bounds: Aabb) -> [f64; 3] {
    [
        eye[0].clamp(bounds.min[0], bounds.max[0]),
        eye[1].clamp(bounds.min[1], bounds.max[1]),
        eye[2].clamp(bounds.min[2], bounds.max[2]),
    ]
}

/// `Direction.get3DDataValue` from a hit's face normal.
pub fn facing_index(face: [i32; 3]) -> u8 {
    match face {
        [0, -1, 0] => 0,
        [0, 1, 0] => 1,
        [0, 0, -1] => 2,
        [0, 0, 1] => 3,
        [-1, 0, 0] => 4,
        _ => 5,
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
                    let toward = target_offset[0] * f64::from(x) + target_offset[1] * f64::from(z);
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

    fn options() -> Options {
        Options {
            mode: Mode::Auto,
            range: 4.5,
            max_self_damage: 12.0,
            anti_suicide: true,
            aim_speed: 4.5,
            delay: (50.0, 50.0),
            action_delay: (0.0, 0.0),
        }
    }

    fn tick(now_ms: u64) -> Tick {
        Tick {
            now_ms,
            has_target: true,
            base: Some([1, 2, 3]),
            crystal_slot: 0,
            selected: 0,
            can_act: true,
            ..Tick::default()
        }
    }

    #[test]
    fn the_activation_delay_gates_the_first_action() {
        let mut target = Target::new();
        target.reset_at(1_000);
        let options = options();
        assert_eq!(target.tick(&tick(1_020), &options), Actions::default());
        let actions = target.tick(&tick(1_050), &options);
        assert_eq!(target.state, State::PlacingCrystal);
        assert_eq!(actions.aim, Some([1.5, 3.0, 3.5]));
        assert!(!actions.use_now);
    }

    #[test]
    fn a_crosshair_on_the_base_places_once_then_waits() {
        let mut target = Target::new();
        target.reset_at(1_000);
        let options = options();
        let mut tick = tick(1_050);
        tick.aimed_block = Some(([1, 2, 3], 1));
        assert!(target.tick(&tick, &options).use_now);
        // `placementSent` holds the second click until the crystal appears.
        assert!(!target.tick(&tick, &options).use_now);
        // The reference's use delay expires; a failed click retries.
        tick.now_ms = 1_050 + PLACEMENT_TIMEOUT_MS + 1;
        assert!(target.tick(&tick, &options).use_now);
    }

    #[test]
    fn a_crystal_above_the_base_is_attacked_and_restarts_the_cycle() {
        let mut target = Target::new();
        target.reset_at(1_000);
        let options = options();
        let mut tick = tick(1_050);
        target.tick(&tick, &options);
        tick.crystal_above = Some(7);
        tick.aimed_crystal = Some(7);
        tick.crystal_point = [1.5, 3.0, 3.5];
        let actions = target.tick(&tick, &options);
        assert_eq!(actions.attack, Some(7));
        assert_eq!(target.state, State::Idle);
        assert_eq!(target.cycles, 1);
    }

    #[test]
    fn the_action_delay_gates_the_next_cycle() {
        let mut target = Target::new();
        target.reset_at(1_000);
        let mut options = options();
        options.action_delay = (100.0, 100.0);
        let mut tick = tick(1_050);
        target.tick(&tick, &options);
        tick.crystal_above = Some(7);
        tick.aimed_crystal = Some(7);
        target.tick(&tick, &options);
        assert_eq!(target.state, State::Idle);
        tick.crystal_above = None;
        tick.aimed_crystal = None;
        tick.now_ms = 1_100;
        assert_eq!(target.tick(&tick, &options), Actions::default());
        tick.now_ms = 1_150;
        assert_eq!(target.tick(&tick, &options).aim, Some([1.5, 3.0, 3.5]));
        assert_eq!(target.state, State::PlacingCrystal);
    }

    #[test]
    fn resetting_reports_the_saved_slot_and_drops_the_action() {
        let mut target = Target::new();
        target.reset_at(1_000);
        target.saved_slot = 4;
        target.block = Some([1, 2, 3]);
        target.set_state(State::PlacingCrystal);
        let mut tick = tick(1_050);
        tick.can_act = false;
        let actions = target.tick(&tick, &options());
        assert!(actions.release);
        assert_eq!(actions.restore, Some(4));
        assert_eq!(target.state, State::Idle);
        assert!(target.block.is_none());
    }

    #[test]
    fn the_crystal_above_a_base_matches_its_entity_box() {
        let base = [2, 5, -1];
        assert_eq!(crystal_above(base, [([2.5, 6.0, -0.5], 3)]), Some(3));
        // The next base's crystal touches only at the search box's edge.
        assert_eq!(crystal_above(base, [([3.5, 6.0, -0.5], 4)]), None);
        // A crystal three blocks up is out of the 1x2x1 search box.
        assert_eq!(crystal_above(base, [([2.5, 8.0, -0.5], 5)]), None);
    }

    #[test]
    fn the_nearest_crystal_point_is_clamped_to_the_box() {
        let point = nearest_point([0.0, 0.0, 0.0], crystal_entity_box([3.0, 2.0, 0.0]));
        assert_eq!(point, [2.0, 2.0, 0.0]);
        let inside = nearest_point([3.0, 3.0, 0.0], crystal_entity_box([3.0, 2.0, 0.0]));
        assert_eq!(inside, [3.0, 3.0, 0.0]);
    }
}
