// AutoAnchor (reference `AutoAnchor`): place a respawn anchor, charge it
// with glowstone, swap to the explosion item and detonate it. The state
// machine, timers and retries follow the reference; the host resolves the
// world blocks, the managed crosshair and the right-click.
use super::super::super::core::settings::{Definition, Kind, Settings};
use serde_json::{Value, json};

pub const ID: &str = "utility.auto-anchor";

pub fn definitions() -> Vec<Definition> {
    let key = |suffix: &str| format!("{ID}.{suffix}");
    vec![
        Definition::choice(&key("mode"), "Mode", &["On bind", "On place"], "On bind"),
        Definition::boolean(&key("double-anchor"), "Double anchor", false),
        Definition::boolean(&key("safe-anchor"), "Safe anchor", false),
        Definition::boolean(
            &key("explosion-item-whitelist"),
            "Explosion item whitelist",
            false,
        ),
        Definition::new(
            &key("explosion-item"),
            "Explosion item",
            Kind::StringList {
                max_entries: 16,
                max_chars: 64,
            },
            json!(["totem of undying"]),
        ),
        Definition::boolean(&key("aim-assist"), "Aim assist", true),
        Definition::boolean(&key("silent-aim"), "Silent aim", true)
            .when(&key("aim-assist"), json!(true)),
        Definition::number(&key("aim-speed"), "Aim speed", 1.0, 12.0, 15.0, 0.1),
        Definition::new(
            &key("delay"),
            "Delay",
            Kind::Range {
                min: 0.0,
                max: 500.0,
                step: 1.0,
            },
            json!([50.0, 100.0]),
        ),
    ]
}

/// The reference's `Mode` (On bind/On place).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    OnBind,
    OnPlace,
}

impl Mode {
    pub fn read(settings: &Settings) -> Self {
        match settings.get(&format!("{ID}.mode")).and_then(Value::as_str) {
            Some("On place") => Self::OnPlace,
            _ => Self::OnBind,
        }
    }
}

/// The reference's `AnchorMacroState`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Idle,
    FindingItems,
    PlacingAnchor,
    WaitingForAnchor,
    PlacingShield,
    ChargingAnchor,
    SwappingToExplosionItem,
    DetonatingAnchor,
    Finish,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

/// The reference's `AnchorBlockHitTarget`: the block under the crosshair and
/// the face it was hit on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HitTarget {
    pub pos: [i32; 3],
    pub facing: u8,
}

/// The reference's `AnchorObstructionPlacementCandidate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShieldCandidate {
    pub block: [i32; 3],
    pub support: [i32; 3],
    pub facing: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub mode: Mode,
    pub double_anchor: bool,
    pub safe_anchor: bool,
    pub explosion_whitelist: bool,
    pub aim_assist: bool,
    pub silent_aim: bool,
    pub aim_speed: f64,
    pub delay: (f64, f64),
}

impl Options {
    pub fn read(settings: &Settings) -> Self {
        let key = |suffix: &str| format!("{ID}.{suffix}");
        Self {
            mode: Mode::read(settings),
            double_anchor: settings
                .get(&key("double-anchor"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            safe_anchor: settings
                .get(&key("safe-anchor"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            explosion_whitelist: settings
                .get(&key("explosion-item-whitelist"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            aim_assist: settings
                .get(&key("aim-assist"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            silent_aim: settings
                .get(&key("silent-aim"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            aim_speed: settings
                .get(&key("aim-speed"))
                .and_then(Value::as_f64)
                .unwrap_or(12.0),
            delay: {
                let value = settings.get(&key("delay")).and_then(Value::as_array);
                let at = |i: usize, d: f64| {
                    value
                        .as_ref()
                        .and_then(|v| v.get(i))
                        .and_then(Value::as_f64)
                        .unwrap_or(d)
                };
                (at(0, 50.0), at(1, 100.0))
            },
        }
    }
}

/// The explosion whitelist entries (the reference's `LimitValue`).
pub fn explosion_items(settings: &Settings) -> Vec<String> {
    settings
        .get(&format!("{ID}.explosion-item"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The host's world snapshot for one tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tick {
    pub now_ms: u64,
    pub eye: [f64; 3],
    pub view: (f32, f32),
    pub held_slot: i32,
    pub anchor_slot: i32,
    pub glowstone_slot: i32,
    pub glowstone_count: i32,
    /// The first whitelisted hotbar slot, -1 when none.
    pub explosion_slot: i32,
    /// The managed crosshair's block and face.
    pub crosshair: Option<HitTarget>,
    /// The crosshair block is a respawn anchor.
    pub crosshair_anchor: bool,
    /// The expected anchor block is present.
    pub anchor_present: bool,
    /// The best visible face point of the anchor, when the host found one.
    pub anchor_face: Option<[f64; 3]>,
    /// The host's shield placement candidate.
    pub shield: Option<ShieldCandidate>,
    /// The shield block is solid.
    pub shield_clear: bool,
    pub can_act: bool,
}

/// What the host does: aim the shared rotation, switch/restore the hotbar
/// slot and right-click (twice for the double anchor).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Actions {
    pub aim: Option<[f64; 3]>,
    pub select: Option<i32>,
    pub restore: Option<i32>,
    pub uses: u8,
    /// Release the rotation and the saved slot.
    pub release: bool,
    /// Disable the module (On bind finish, missing items, exhausted retries).
    pub disable: bool,
    /// A missing item notification to show.
    pub notify: Option<&'static str>,
}

/// The reference's `AnchorMacroState` ledger and action timers.
#[derive(Clone, Debug)]
pub struct Macro {
    pub state: State,
    pub anchor_slot: i32,
    pub glowstone_slot: i32,
    pub explosion_slot: i32,
    pub prev_slot: i32,
    pub anchor_pos: Option<[i32; 3]>,
    pub base_pos: Option<[i32; 3]>,
    pub hit_facing: Option<u8>,
    pub anchor_placed: bool,
    pub double_anchor_done: bool,
    pub retries: u32,
    pub wait_retries: u32,
    pub charge_retries: u32,
    pub shield: Option<ShieldCandidate>,
    pub shield_placed: bool,
    pub shield_started: bool,
    pub shield_charge_retries: u32,
    pub shield_step: i32,
    pub action_at: u64,
    pub action_delay: u64,
    rng: u64,
}

impl Default for Macro {
    fn default() -> Self {
        Self::new()
    }
}

const MAX_RETRIES: u32 = 4;
const MAX_PLACE_RETRIES: u32 = 20;

impl Macro {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            anchor_slot: -1,
            glowstone_slot: -1,
            explosion_slot: -1,
            prev_slot: -1,
            anchor_pos: None,
            base_pos: None,
            hit_facing: None,
            anchor_placed: false,
            double_anchor_done: false,
            retries: 0,
            wait_retries: 0,
            charge_retries: 0,
            shield: None,
            shield_placed: false,
            shield_started: false,
            shield_charge_retries: 0,
            shield_step: 0,
            action_at: 0,
            action_delay: 0,
            rng: 0,
        }
    }

    /// `resetState`.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// `onEnable`: the On place mode starts idle; the bind mode starts by
    /// finding its items.
    pub fn start(&mut self, mode: Mode, now_ms: u64, delay: (f64, f64)) {
        self.reset();
        self.state = match mode {
            Mode::OnPlace => State::Idle,
            Mode::OnBind => State::FindingItems,
        };
        self.reset_action(now_ms, delay);
    }

    /// `resetActionTimer`: `timer.reset()` and a fresh random delay.
    pub fn reset_action(&mut self, now_ms: u64, delay: (f64, f64)) {
        self.action_at = now_ms;
        self.action_delay = self.draw_delay(delay);
    }

    fn random(&mut self) -> f64 {
        if self.rng == 0 {
            self.rng = 0x9e37_79b9_7f4a_7c15;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    fn draw_delay(&mut self, (lo, hi): (f64, f64)) -> u64 {
        if hi <= lo {
            return lo.max(0.0) as u64;
        }
        (lo + (hi - lo) * self.random()).max(0.0) as u64
    }

    /// `timer.hasTimeElapsed(actionDelay)`.
    pub fn action_ready(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.action_at) >= self.action_delay
    }

    /// The On place trigger: right-clicking with an anchor starts waiting for
    /// the anchor the player placed.
    pub fn start_on_place(&mut self, tick: &Tick, options: &Options) -> bool {
        if options.mode != Mode::OnPlace || self.state != State::Idle {
            return false;
        }
        let Some(hit) = tick.crosshair else {
            return false;
        };
        if tick.anchor_slot < 0 || tick.glowstone_slot < 0 {
            return false;
        }
        self.glowstone_slot = tick.glowstone_slot;
        self.anchor_slot = tick.held_slot;
        self.base_pos = Some(hit.pos);
        self.hit_facing = Some(hit.facing);
        self.anchor_pos = Some(shift(hit.pos, hit.facing));
        self.prev_slot = tick.held_slot;
        self.state = State::WaitingForAnchor;
        self.retries = 0;
        self.wait_retries = 0;
        true
    }

    fn next_charge_state(&self, options: &Options) -> State {
        if options.safe_anchor {
            State::PlacingShield
        } else {
            State::ChargingAnchor
        }
    }

    fn looking_at_anchor(&self, tick: &Tick) -> bool {
        let Some(hit) = tick.crosshair else {
            return false;
        };
        hit.pos == self.base_pos.unwrap_or([i32::MIN; 3])
            || hit.pos == self.anchor_pos.unwrap_or([i32::MIN; 3])
    }

    fn anchor_aim(&self, tick: &Tick) -> Option<[f64; 3]> {
        self.anchor_pos
            .map(|pos| tick.anchor_face.unwrap_or_else(|| center(pos)))
    }

    /// One reference `onTick` step.
    pub fn tick(&mut self, tick: &Tick, options: &Options) -> Actions {
        let mut out = Actions::default();
        if !tick.can_act {
            return out;
        }
        if self.state == State::Idle {
            if options.mode == Mode::OnBind {
                out.disable = true;
            }
            return out;
        }
        match self.state {
            State::FindingItems => {
                self.anchor_slot = tick.anchor_slot;
                self.glowstone_slot = tick.glowstone_slot;
                if self.anchor_slot < 0 || self.glowstone_slot < 0 {
                    out.notify = Some(if self.anchor_slot < 0 {
                        "Respawn anchor not in hotbar"
                    } else {
                        "Glowstone not in hotbar"
                    });
                    out.disable = true;
                    return out;
                }
                self.prev_slot = tick.held_slot;
                self.state = State::PlacingAnchor;
            }
            State::PlacingAnchor => {
                if self.anchor_pos.is_none() || self.base_pos.is_none() || self.hit_facing.is_none()
                {
                    if tick.crosshair_anchor {
                        let Some(pos) = tick.crosshair.map(|hit| hit.pos) else {
                            out.disable = true;
                            return out;
                        };
                        self.base_pos = Some(pos);
                        self.anchor_pos = Some(pos);
                        self.state = self.next_charge_state(options);
                        self.reset_action(tick.now_ms, options.delay);
                        self.retries = 0;
                    } else {
                        let Some(hit) = tick.crosshair else {
                            self.retries += 1;
                            if self.retries > MAX_PLACE_RETRIES {
                                self.state = State::Finish;
                            }
                            return out;
                        };
                        self.base_pos = Some(hit.pos);
                        self.hit_facing = Some(hit.facing);
                        self.anchor_pos = Some(shift(hit.pos, hit.facing));
                        self.retries = 0;
                    }
                }
                out.aim = self.base_pos.map(top_center);
                if !self.anchor_placed && !self.looking_at_anchor(tick) {
                    self.retries += 1;
                    if self.retries <= MAX_PLACE_RETRIES {
                        return out;
                    }
                    self.state = State::Finish;
                    return out;
                }
                if !self.anchor_placed {
                    out.select = Some(self.anchor_slot);
                    out.uses = 1;
                    self.anchor_placed = true;
                    self.wait_retries = 0;
                    return out;
                }
                if !tick.anchor_present {
                    self.wait_retries += 1;
                    if self.wait_retries <= MAX_RETRIES {
                        return out;
                    }
                    self.anchor_placed = false;
                    self.wait_retries = 0;
                    self.charge_retries += 1;
                    if self.charge_retries <= MAX_RETRIES {
                        return out;
                    }
                    self.state = State::Finish;
                    return out;
                }
                self.anchor_placed = false;
                self.wait_retries = 0;
                self.charge_retries = 0;
                self.state = self.next_charge_state(options);
                self.reset_action(tick.now_ms, options.delay);
                self.retries = 0;
            }
            State::WaitingForAnchor => {
                if tick.anchor_present {
                    self.state = self.next_charge_state(options);
                    self.reset_action(tick.now_ms, options.delay);
                    self.retries = 0;
                } else if tick.crosshair_anchor {
                    if let Some(pos) = tick.crosshair.map(|hit| hit.pos) {
                        self.anchor_pos = Some(pos);
                        self.state = self.next_charge_state(options);
                        self.reset_action(tick.now_ms, options.delay);
                        self.retries = 0;
                    }
                } else {
                    self.wait_retries += 1;
                    if self.wait_retries > MAX_RETRIES {
                        self.state = State::Finish;
                    }
                }
            }
            State::PlacingShield => {
                if !self.shield_started {
                    self.shield_started = true;
                    if tick.glowstone_count < 2 {
                        self.state = State::ChargingAnchor;
                        self.reset_action(tick.now_ms, options.delay);
                        self.retries = 0;
                        return out;
                    }
                    match tick.shield {
                        Some(candidate) => {
                            self.shield = Some(candidate);
                            self.shield_placed = false;
                            self.shield_charge_retries = 0;
                            self.shield_step = 0;
                        }
                        None => {
                            self.state = State::ChargingAnchor;
                            self.reset_action(tick.now_ms, options.delay);
                            self.retries = 0;
                            return out;
                        }
                    }
                }
                let Some(candidate) = self.shield else {
                    self.state = State::ChargingAnchor;
                    self.reset_action(tick.now_ms, options.delay);
                    self.retries = 0;
                    return out;
                };
                out.aim = Some(face_center(candidate.support, candidate.facing));
                if !self.action_ready(tick.now_ms) {
                    return out;
                }
                if self.shield_step < 1 {
                    self.shield_step += 1;
                    return out;
                }
                if !self.shield_placed {
                    if tick.crosshair.is_none() {
                        self.retries += 1;
                        if self.retries > MAX_RETRIES {
                            self.state = State::ChargingAnchor;
                            self.reset_action(tick.now_ms, options.delay);
                            self.retries = 0;
                        }
                        return out;
                    }
                    out.select = Some(self.glowstone_slot);
                    out.uses = 1;
                    self.shield_placed = true;
                    self.shield_charge_retries = 0;
                    return out;
                }
                if !tick.shield_clear {
                    self.shield_charge_retries += 1;
                    if self.shield_charge_retries > MAX_RETRIES {
                        self.state = State::ChargingAnchor;
                        self.reset_action(tick.now_ms, options.delay);
                        self.retries = 0;
                    }
                    return out;
                }
                self.state = State::ChargingAnchor;
                self.reset_action(tick.now_ms, options.delay);
                self.retries = 0;
            }
            State::ChargingAnchor => {
                out.aim = self.anchor_aim(tick);
                if !self.action_ready(tick.now_ms) {
                    return out;
                }
                if !self.looking_at_anchor(tick) {
                    self.retries += 1;
                    if self.retries <= MAX_RETRIES {
                        return out;
                    }
                    self.state = State::Finish;
                    return out;
                }
                out.select = Some(self.glowstone_slot);
                out.uses = 1;
                self.state = State::SwappingToExplosionItem;
                self.reset_action(tick.now_ms, options.delay);
                self.retries = 0;
            }
            State::SwappingToExplosionItem => {
                out.aim = self.anchor_aim(tick);
                if !self.action_ready(tick.now_ms) {
                    return out;
                }
                if !self.looking_at_anchor(tick) {
                    self.retries += 1;
                    if self.retries <= MAX_RETRIES {
                        return out;
                    }
                    self.state = State::Finish;
                    return out;
                }
                self.explosion_slot = if options.explosion_whitelist && tick.explosion_slot >= 0 {
                    tick.explosion_slot
                } else {
                    self.anchor_slot
                };
                out.select = Some(self.explosion_slot);
                self.state = State::DetonatingAnchor;
                self.reset_action(tick.now_ms, options.delay);
                self.retries = 0;
            }
            State::DetonatingAnchor => {
                out.aim = self.anchor_aim(tick);
                if !self.action_ready(tick.now_ms) {
                    return out;
                }
                if !self.looking_at_anchor(tick) {
                    self.retries += 1;
                    if self.retries <= MAX_RETRIES {
                        return out;
                    }
                    self.state = State::Finish;
                    return out;
                }
                out.uses = 1;
                if options.double_anchor && !self.double_anchor_done {
                    out.uses = 2;
                    self.double_anchor_done = true;
                    self.state = State::ChargingAnchor;
                    self.reset_action(tick.now_ms, options.delay);
                    self.retries = 0;
                    return out;
                }
                self.state = State::Finish;
            }
            State::Finish => {
                out.release = true;
                out.restore = (self.prev_slot >= 0).then_some(self.prev_slot);
                if options.mode == Mode::OnPlace {
                    self.reset();
                } else {
                    out.disable = true;
                }
            }
            State::Idle => {}
        }
        out
    }
}

pub fn shift(pos: [i32; 3], facing: u8) -> [i32; 3] {
    let n = facing_normal(facing);
    [pos[0] + n[0], pos[1] + n[1], pos[2] + n[2]]
}

pub fn facing_normal(facing: u8) -> [i32; 3] {
    match facing {
        0 => [0, -1, 0],
        1 => [0, 1, 0],
        2 => [0, 0, -1],
        3 => [0, 0, 1],
        4 => [-1, 0, 0],
        _ => [1, 0, 0],
    }
}

pub fn center(pos: [i32; 3]) -> [f64; 3] {
    [
        f64::from(pos[0]) + 0.5,
        f64::from(pos[1]) + 0.5,
        f64::from(pos[2]) + 0.5,
    ]
}

pub fn top_center(pos: [i32; 3]) -> [f64; 3] {
    [
        f64::from(pos[0]) + 0.5,
        f64::from(pos[1]) + 1.0,
        f64::from(pos[2]) + 0.5,
    ]
}

/// The reference's `faceCenter`: the centre of the block face.
pub fn face_center(pos: [i32; 3], facing: u8) -> [f64; 3] {
    let n = facing_normal(facing);
    [
        f64::from(pos[0]) + 0.5 + f64::from(n[0]) * 0.5,
        f64::from(pos[1]) + 0.5 + f64::from(n[1]) * 0.5,
        f64::from(pos[2]) + 0.5 + f64::from(n[2]) * 0.5,
    ]
}

/// A block-aligned box (the shield obstruction and the player's bounds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

pub fn block_bounds(pos: [i32; 3]) -> Bounds {
    Bounds {
        min: [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])],
        max: [
            f64::from(pos[0]) + 1.0,
            f64::from(pos[1]) + 1.0,
            f64::from(pos[2]) + 1.0,
        ],
    }
}

/// `AxisAlignedBB.calculateIntercept` on the segment: whether the line from
/// `origin` to `target` enters the box.
pub fn segment_hits_box(origin: [f64; 3], target: [f64; 3], bounds: Bounds) -> bool {
    let (mut near, mut far) = (0.0f64, 1.0f64);
    for axis in 0..3 {
        let direction = target[axis] - origin[axis];
        if direction.abs() < 1e-12 {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return false;
            }
            continue;
        }
        let a = (bounds.min[axis] - origin[axis]) / direction;
        let b = (bounds.max[axis] - origin[axis]) / direction;
        near = near.max(a.min(b));
        far = far.min(a.max(b));
        if near > far {
            return false;
        }
    }
    true
}

/// The reference's `isAnchorFullyVisible`: every line from the eye to the
/// anchor samples hits the obstruction.
pub fn anchor_fully_visible(eye: [f64; 3], anchor: [i32; 3], obstruction: [i32; 3]) -> bool {
    let bounds = block_bounds(obstruction);
    let (x, y, z) = (
        f64::from(anchor[0]),
        f64::from(anchor[1]),
        f64::from(anchor[2]),
    );
    let samples = [
        [x + 0.5, y + 0.5, z + 0.5],
        [x + 0.5, y + 1.0, z + 0.5],
        [x + 0.5, y, z + 0.5],
        [x, y + 0.5, z + 0.5],
        [x + 1.0, y + 0.5, z + 0.5],
        [x + 0.5, y + 0.5, z],
        [x + 0.5, y + 0.5, z + 1.0],
    ];
    samples
        .into_iter()
        .all(|sample| segment_hits_box(eye, sample, bounds))
}

/// The reference's `computeVisibilityRatio`: the fraction of the player's
/// box samples whose line from the anchor aim point hits the obstruction.
pub fn visibility_ratio(origin: [f64; 3], obstruction: [i32; 3], player: Bounds) -> f32 {
    let bounds = block_bounds(obstruction);
    let x_step = 1.0 / ((player.max[0] - player.min[0]) * 2.0 + 1.0);
    let y_step = 1.0 / ((player.max[1] - player.min[1]) * 2.0 + 1.0);
    let z_step = 1.0 / ((player.max[2] - player.min[2]) * 2.0 + 1.0);
    if x_step < 0.0 || y_step < 0.0 || z_step < 0.0 {
        return 0.0;
    }
    let x_offset = (1.0 - (1.0 / x_step).floor() * x_step) / 2.0;
    let z_offset = (1.0 - (1.0 / z_step).floor() * z_step) / 2.0;
    let (mut obstructed, mut total) = (0u32, 0u32);
    let mut x_fraction = 0.0;
    while x_fraction <= 1.0 {
        let mut y_fraction = 0.0;
        while y_fraction <= 1.0 {
            let mut z_fraction = 0.0;
            while z_fraction <= 1.0 {
                let sample = [
                    player.min[0] + (player.max[0] - player.min[0]) * x_fraction + x_offset,
                    player.min[1] + (player.max[1] - player.min[1]) * y_fraction,
                    player.min[2] + (player.max[2] - player.min[2]) * z_fraction + z_offset,
                ];
                if segment_hits_box(origin, sample, bounds) {
                    obstructed += 1;
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
        obstructed as f32 / total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Options {
        Options {
            mode: Mode::OnBind,
            double_anchor: false,
            safe_anchor: false,
            explosion_whitelist: false,
            aim_assist: true,
            silent_aim: true,
            aim_speed: 12.0,
            delay: (0.0, 0.0),
        }
    }

    fn tick(now_ms: u64) -> Tick {
        Tick {
            now_ms,
            eye: [0.5, 101.0, 0.5],
            view: (0.0, 0.0),
            held_slot: 0,
            anchor_slot: 0,
            glowstone_slot: 1,
            glowstone_count: 8,
            explosion_slot: -1,
            crosshair: Some(HitTarget {
                pos: [2, 100, 0],
                facing: 1,
            }),
            crosshair_anchor: false,
            anchor_present: false,
            anchor_face: None,
            shield: None,
            shield_clear: false,
            can_act: true,
        }
    }

    #[test]
    fn the_sequence_places_charges_swaps_and_detonates() {
        let mut anchor = Macro::new();
        anchor.start(Mode::OnBind, 0, (0.0, 0.0));
        let options = options();
        let mut t = tick(0);
        // FINDING_ITEMS -> PLACING_ANCHOR, then aim and place.
        let a = anchor.tick(&t, &options);
        assert_eq!(anchor.state, State::PlacingAnchor);
        assert_eq!(a.aim, None);
        let a = anchor.tick(&t, &options);
        assert_eq!(a.aim, Some([2.5, 101.0, 0.5]));
        assert_eq!(a.select, Some(0));
        assert_eq!(a.uses, 1);
        // The anchor appears; the next tick moves to charging.
        t.anchor_present = true;
        let a = anchor.tick(&t, &options);
        assert_eq!(anchor.state, State::ChargingAnchor);
        assert!(!a.release);
        // Charge with glowstone.
        let a = anchor.tick(&t, &options);
        assert_eq!(a.select, Some(1));
        assert_eq!(a.uses, 1);
        assert_eq!(anchor.state, State::SwappingToExplosionItem);
        // Swap back to the anchor and detonate.
        let a = anchor.tick(&t, &options);
        assert_eq!(a.select, Some(0));
        assert_eq!(anchor.state, State::DetonatingAnchor);
        let a = anchor.tick(&t, &options);
        assert_eq!(a.uses, 1);
        assert_eq!(anchor.state, State::Finish);
        let a = anchor.tick(&t, &options);
        assert!(a.release && a.disable);
    }

    #[test]
    fn the_retry_budget_finishes_the_sequence() {
        let mut anchor = Macro::new();
        anchor.start(Mode::OnBind, 0, (0.0, 0.0));
        anchor.state = State::PlacingAnchor;
        let options = options();
        let mut t = tick(0);
        t.crosshair = None;
        for _ in 0..=MAX_PLACE_RETRIES {
            anchor.tick(&t, &options);
        }
        assert_eq!(anchor.state, State::Finish);
    }

    #[test]
    fn missing_items_disable_the_bind_mode() {
        let mut anchor = Macro::new();
        anchor.start(Mode::OnBind, 0, (0.0, 0.0));
        let options = options();
        let mut t = tick(0);
        t.anchor_slot = -1;
        let a = anchor.tick(&t, &options);
        assert!(a.disable);
        assert!(a.notify.is_some());
    }

    #[test]
    fn the_double_anchor_uses_twice_and_recharges() {
        let mut anchor = Macro::new();
        anchor.start(Mode::OnBind, 0, (0.0, 0.0));
        anchor.state = State::DetonatingAnchor;
        anchor.anchor_pos = Some([2, 101, 0]);
        anchor.base_pos = Some([2, 100, 0]);
        anchor.anchor_slot = 0;
        anchor.glowstone_slot = 1;
        let mut options = options();
        options.double_anchor = true;
        let mut t = tick(0);
        t.anchor_present = true;
        let a = anchor.tick(&t, &options);
        assert_eq!(a.uses, 2);
        assert_eq!(anchor.state, State::ChargingAnchor);
        assert!(anchor.double_anchor_done);
    }

    #[test]
    fn on_place_starts_from_a_right_clicked_block() {
        let mut anchor = Macro::new();
        anchor.start(Mode::OnPlace, 0, (0.0, 0.0));
        let mut options = options();
        options.mode = Mode::OnPlace;
        let t = tick(0);
        assert!(anchor.start_on_place(&t, &options));
        assert_eq!(anchor.state, State::WaitingForAnchor);
        assert_eq!(anchor.anchor_pos, Some([2, 101, 0]));
        assert_eq!(anchor.base_pos, Some([2, 100, 0]));
    }

    #[test]
    fn face_centers_sit_on_the_block_face() {
        assert_eq!(face_center([2, 100, 0], 1), [2.5, 101.0, 0.5]);
        assert_eq!(face_center([2, 100, 0], 0), [2.5, 100.0, 0.5]);
        assert_eq!(shift([2, 100, 0], 3), [2, 100, 1]);
    }

    #[test]
    fn a_segment_reports_the_boxes_it_enters() {
        let block = block_bounds([1, 0, 0]);
        assert!(segment_hits_box([0.0, 0.5, 0.5], [3.0, 0.5, 0.5], block));
        assert!(!segment_hits_box([0.0, 2.0, 0.5], [3.0, 2.0, 0.5], block));
        // A box behind the segment is not hit.
        assert!(!segment_hits_box([2.5, 0.5, 0.5], [3.5, 0.5, 0.5], block));
    }

    #[test]
    fn the_visibility_ratio_measures_the_cover() {
        let player = Bounds {
            min: [2.0, 100.0, 0.2],
            max: [2.6, 101.8, 0.8],
        };
        let covered = visibility_ratio([0.5, 100.5, 0.5], [1, 100, 0], player);
        assert!(covered > 0.9, "{covered}");
        let open = visibility_ratio([0.5, 100.5, 0.5], [1, 102, 0], player);
        assert_eq!(open, 0.0);
    }
}
