// Block-In (reference `BlockIn`): wall the player in. The host resolves the
// enclosure cells and their supports; this owns the block choice, the
// placement delay, the aim target and the sneak hold.
use super::super::super::core::settings::{Definition, Kind, Settings};
use serde_json::{Value, json};

pub const ID: &str = "world.block-in";

pub fn definitions() -> Vec<Definition> {
    let key = |suffix: &str| format!("{ID}.{suffix}");
    vec![
        Definition::boolean(&key("silent-aim"), "Silent aim", false),
        Definition::boolean(&key("sneak"), "Sneak", false),
        Definition::boolean(&key("keep-sneak"), "Keep sneak", true)
            .when(&key("sneak"), json!(true)),
        Definition::boolean(&key("bed-finder"), "Bed finder", true),
        Definition::choice(
            &key("block-priority"),
            "Block priority",
            &["Lowest cost", "Hardest"],
            "Lowest cost",
        ),
        Definition::new(
            &key("place-delay"),
            "Place delay",
            Kind::Range {
                min: 0.0,
                max: 250.0,
                step: 1.0,
            },
            json!([0.0, 30.0]),
        ),
        Definition::number(&key("aim-speed"), "Aim speed", 1.0, 12.0, 25.0, 0.1),
        Definition::boolean(&key("return-to-last-slot"), "Return to last slot", true),
        Definition::boolean(&key("use-blacklist"), "Use blacklist", true),
        Definition::new(
            &key("block-blacklist"),
            "Block blacklist",
            Kind::StringList {
                max_entries: 64,
                max_chars: 64,
            },
            json!([
                "Dispenser",
                "Note Block",
                "Cobweb",
                "TNT",
                "Monster Spawner",
                "Enchantment Table",
                "Oak Fence",
                "Jukebox",
                "Melon",
                "Command Block",
                "Anvil",
                "Glass Pane",
                "Iron Bars",
                "Ice",
                "Packed Ice",
                "Gold Ore",
                "Iron Ore",
                "Coal Ore",
                "Lapis Lazuli Ore",
                "Diamond Ore",
                "Emerald Ore",
                "Redstone Ore",
                "Chest",
                "Trapped Chest",
                "Ender Chest",
                "Crafting Table",
                "Furnace",
                "Brewing Stand",
                "Beacon",
                "Hopper",
                "Dropper",
            ]),
        ),
    ]
}

/// The reference's `blockPriorityMode`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Priority {
    LowestCost,
    Hardest,
}

impl Priority {
    pub fn read(settings: &Settings) -> Self {
        match settings
            .get(&format!("{ID}.block-priority"))
            .and_then(Value::as_str)
        {
            Some("Hardest") => Self::Hardest,
            _ => Self::LowestCost,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub silent_aim: bool,
    pub sneak: bool,
    pub keep_sneak: bool,
    pub bed_finder: bool,
    pub priority: Priority,
    pub place_delay: (f64, f64),
    pub aim_speed: f64,
    pub return_to_last_slot: bool,
    pub use_blacklist: bool,
    pub blacklist: Vec<String>,
}

impl Options {
    pub fn read(settings: &Settings) -> Self {
        let key = |suffix: &str| format!("{ID}.{suffix}");
        let boolean = |suffix: &str, default: bool| {
            settings
                .get(&key(suffix))
                .and_then(Value::as_bool)
                .unwrap_or(default)
        };
        let place_delay = {
            let value = settings.get(&key("place-delay")).and_then(Value::as_array);
            let at = |i: usize, d: f64| {
                value
                    .as_ref()
                    .and_then(|v| v.get(i))
                    .and_then(Value::as_f64)
                    .unwrap_or(d)
            };
            (at(0, 0.0), at(1, 30.0))
        };
        Self {
            silent_aim: boolean("silent-aim", false),
            sneak: boolean("sneak", false),
            keep_sneak: boolean("keep-sneak", true),
            bed_finder: boolean("bed-finder", true),
            priority: Priority::read(settings),
            place_delay,
            aim_speed: settings
                .get(&key("aim-speed"))
                .and_then(Value::as_f64)
                .unwrap_or(12.0),
            return_to_last_slot: boolean("return-to-last-slot", true),
            use_blacklist: boolean("use-blacklist", true),
            blacklist: settings
                .get(&key("block-blacklist"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        }
    }
}

/// One block the enclosure needs: the support block and face to aim at, and
/// the cell the placed block fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub support: [i32; 3],
    pub facing: u8,
    pub place: [i32; 3],
}

/// The host's world snapshot for one tick.
#[derive(Clone, Debug, Default)]
pub struct Tick {
    pub now_ms: u64,
    pub eye: [f64; 3],
    pub view: (f32, f32),
    pub on_solid: bool,
    pub held_slot: i32,
    /// The best usable block slot, -1 when none.
    pub block_slot: i32,
    /// The managed crosshair's block, face and hit point.
    pub crosshair: Option<([i32; 3], u8, [f64; 3])>,
    /// The enclosure placements still missing, in placement order.
    pub targets: Vec<Placement>,
    pub can_act: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Actions {
    pub aim: Option<[f64; 3]>,
    pub select: Option<i32>,
    pub restore: Option<i32>,
    pub uses: u8,
    pub sneak: bool,
    pub release: bool,
    pub disable: bool,
    pub notify: Option<&'static str>,
}

/// The reference's placement ledger: the carried slot, the target and the
/// timers.
#[derive(Clone, Debug)]
pub struct Macro {
    pub slot: i32,
    pub prev_slot: i32,
    pub retries: u32,
    pub click_at: u64,
    pub aim_at: u64,
    pub sneaking: bool,
    rng: u64,
}

impl Default for Macro {
    fn default() -> Self {
        Self::new()
    }
}

const MAX_AIM_RETRIES: u32 = 100;

impl Macro {
    pub fn new() -> Self {
        Self {
            slot: -1,
            prev_slot: -1,
            retries: 0,
            click_at: 0,
            aim_at: 0,
            sneaking: false,
            rng: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn random(&mut self) -> f64 {
        if self.rng == 0 {
            self.rng = 0x2545_f491_4f6c_dd1d;
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

    /// The face point the aim and the crosshair check use.
    fn target_point(placement: &Placement) -> [f64; 3] {
        face_center(placement.support, placement.facing)
    }

    pub fn tick(&mut self, tick: &Tick, options: &Options) -> Actions {
        let mut out = Actions::default();
        if !tick.can_act || !tick.on_solid {
            out.release = true;
            out.restore = self.take_prev_slot(options);
            self.reset();
            return out;
        }
        if tick.block_slot < 0 {
            out.notify = Some(if tick.targets.is_empty() {
                "Could not block in!"
            } else {
                "No blocks in hotbar!"
            });
            out.disable = true;
            return out;
        }
        if self.slot < 0 {
            self.slot = tick.block_slot;
            if self.prev_slot < 0 {
                self.prev_slot = tick.held_slot;
            }
        }
        let Some(target) = tick.targets.first() else {
            out.release = true;
            out.restore = self.take_prev_slot(options);
            out.disable = true;
            self.reset();
            self.sneaking = false;
            return out;
        };
        if tick.held_slot != self.slot {
            out.select = Some(self.slot);
        }
        out.aim = Some(Self::target_point(target));
        let facing = tick.crosshair.is_some_and(|(pos, face, point)| {
            pos == target.support
                && face == target.facing
                && distance(point, Self::target_point(target)) <= 0.3
        });
        out.sneak = options.sneak;
        if options.sneak {
            self.sneaking = true;
        }
        if facing && tick.now_ms >= self.click_at {
            out.uses = 1;
            self.click_at = tick.now_ms + self.draw_delay(options.place_delay);
            self.aim_at = tick.now_ms;
            self.retries = 0;
        } else if !facing {
            self.retries += 1;
            if self.retries > MAX_AIM_RETRIES {
                out.notify = Some("Failed to place a block!");
                out.disable = true;
            }
        }
        out
    }

    fn take_prev_slot(&mut self, options: &Options) -> Option<i32> {
        if options.return_to_last_slot && self.prev_slot >= 0 && self.prev_slot != self.slot {
            Some(self.prev_slot)
        } else {
            None
        }
    }
}

pub fn face_center(pos: [i32; 3], facing: u8) -> [f64; 3] {
    let normal = match facing {
        0 => [0.0, -0.5, 0.0],
        1 => [0.0, 0.5, 0.0],
        2 => [0.0, 0.0, -0.5],
        3 => [0.0, 0.0, 0.5],
        4 => [-0.5, 0.0, 0.0],
        _ => [0.5, 0.0, 0.0],
    };
    [
        f64::from(pos[0]) + 0.5 + normal[0],
        f64::from(pos[1]) + 0.5 + normal[1],
        f64::from(pos[2]) + 0.5 + normal[2],
    ]
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Options {
        Options {
            silent_aim: false,
            sneak: false,
            keep_sneak: true,
            bed_finder: true,
            priority: Priority::LowestCost,
            place_delay: (0.0, 0.0),
            aim_speed: 12.0,
            return_to_last_slot: true,
            use_blacklist: true,
            blacklist: Vec::new(),
        }
    }

    fn target() -> Placement {
        Placement {
            support: [0, 99, 0],
            facing: 1,
            place: [0, 100, 0],
        }
    }

    fn tick(now_ms: u64) -> Tick {
        Tick {
            now_ms,
            eye: [0.5, 101.0, 0.5],
            view: (0.0, 0.0),
            on_solid: true,
            held_slot: 0,
            block_slot: 0,
            crosshair: None,
            targets: vec![target()],
            can_act: true,
        }
    }

    #[test]
    fn the_placement_aims_at_the_support_face_and_clicks_once() {
        let mut block_in = Macro::new();
        let mut options = options();
        options.place_delay = (10.0, 10.0);
        let mut t = tick(0);
        let a = block_in.tick(&t, &options);
        assert_eq!(a.aim, Some([0.5, 100.0, 0.5]));
        assert_eq!(a.uses, 0);
        t.crosshair = Some(([0, 99, 0], 1, [0.5, 100.0, 0.5]));
        let a = block_in.tick(&t, &options);
        assert_eq!(a.uses, 1);
        // The click delay holds the next placement.
        t.now_ms = 5;
        let a = block_in.tick(&t, &options);
        assert_eq!(a.uses, 0);
    }

    #[test]
    fn a_filled_enclosure_releases_and_restores_the_slot() {
        let mut block_in = Macro::new();
        let options = options();
        let mut t = tick(0);
        block_in.tick(&t, &options);
        block_in.slot = 2;
        t.held_slot = 2;
        t.targets.clear();
        let a = block_in.tick(&t, &options);
        assert!(a.release);
        assert_eq!(a.restore, Some(0));
    }

    #[test]
    fn no_usable_blocks_disables_the_module() {
        let mut block_in = Macro::new();
        let options = options();
        let mut t = tick(0);
        t.block_slot = -1;
        let a = block_in.tick(&t, &options);
        assert!(a.disable);
        assert!(a.notify.is_some());
    }

    #[test]
    fn the_face_centre_sits_on_the_support() {
        assert_eq!(face_center([0, 99, 0], 1), [0.5, 100.0, 0.5]);
        assert_eq!(face_center([0, 99, 0], 5), [1.0, 99.5, 0.5]);
    }
}
