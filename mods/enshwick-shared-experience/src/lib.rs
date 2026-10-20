//! Every character on a server keeps up with the one furthest ahead in
//! experience.
//!
//! Every source of experience adds to one attribute, gained experience, which
//! the `player_level` system applies each tick, one level at most. A link run
//! before it reads each spawned character's level, progress and gains, moves
//! the server's high-water mark (the leader's gain once, or a character already
//! further ahead) and adds to the gains of each character below the mark, up
//! to the end of its current level. Vanilla then applies the experience,
//! raises the level and sends it to the player.

mod share;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use enshwick_api::abi::Side;
use enshwick_api::ecs::{Attributes, Ecs, Worlds};
use enshwick_api::events::{Shutdown, WorldLoaded, WorldSaved};
use enshwick_api::state::Store;
use enshwick_api::{Load, Mod, Next, Off};

pub use share::{Character, Levels, high_water};

/// `player_level`'s iterator: context, `CurrentTransform`, then `Experience`,
/// `Level` and `PlayerTracking`, each with its change tracking (two stamps,
/// entity record, type and flags, tracker), then `BalancingRegistry`,
/// `BasicPlayerInfos`, `AttributeRegistry` and the `LevelUpEvent` writer.
const WORDS: usize = 24;
const EXPERIENCE: usize = 2;
const STAMP_A: usize = 3;
const STAMP_B: usize = 4;
const ENTITY: usize = 5;
const KIND: usize = 6;
const TRACKER: usize = 7;
const LEVEL: usize = 8;
const BALANCING: usize = 20;
const REGISTRY: usize = 22;

/// The attributes of `Experience` and `Level`.
const GAINED: u32 = 0x1d7d_d102;
const PROGRESS: u32 = 0x8dcd_daf4;
const NEED: u32 = 0x0472_3f62;

/// Never called with level 0: it would read before its table.
type NeedFor = unsafe extern "system" fn(level: u32, balancing: u64) -> u32;
type LevelCap = unsafe extern "system" fn(balancing: u64) -> u32;

/// Shared experience settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Every character catches up with the one furthest ahead in experience.
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// What the dedicated server keeps between starts.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct Saved {
    /// The most cumulative experience any character has reached here.
    high_water: u64,
}

/// The engine helpers this mod uses.
#[derive(Debug)]
struct Engine {
    ecs: Ecs,
    attributes: Attributes,
    worlds: Worlds,
    need_for: NeedFor,
    level_cap: LevelCap,
    level_attribute: u32,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
/// The game's need per level, read once on the first tick.
static LEVELS: OnceLock<Levels> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);
/// Set when anything goes wrong; from then on vanilla runs alone.
static OFF: AtomicBool = AtomicBool::new(false);
/// The high-water mark of cumulative experience.
static HIGH_WATER: AtomicU64 = AtomicU64::new(0);

/// Turns a resolved address into a function pointer of type `F`.
///
/// # Safety
///
/// `address` is a function of type `F`.
unsafe fn function<F: Copy>(address: u64) -> F {
    let address = usize::try_from(address).unwrap_or(0);
    // SAFETY: the caller guarantees the type; `F` is a function pointer the size
    // of `usize`.
    unsafe { core::mem::transmute_copy(&address) }
}

fn turn_off(why: &str) {
    if !OFF.swap(true, Ordering::AcqRel) {
        enshwick_api::warn!("shared experience off, vanilla from now on: {why}");
    }
}

enshwick_api::detour! {
    static PLAYER_LEVEL: unsafe extern "system" fn(ctx: *mut u64) = player_level;
}

fn player_level(next: Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    if ENABLED.load(Ordering::Relaxed)
        && !OFF.load(Ordering::Acquire)
        && let Some(engine) = ENGINE.get()
        // SAFETY: the system's live context.
        && unsafe { engine.worlds.is_server(ctx) }
    {
        // SAFETY: the system's live context.
        let saved = unsafe { engine.ecs.row(ctx) };
        // SAFETY: called with the system's live context, before vanilla runs.
        if std::panic::catch_unwind(|| unsafe { share_experience(engine, ctx) }).is_err() {
            turn_off("the experience pass panicked");
        }
        // Hand vanilla its walk from where it would have started.
        // SAFETY: the system's live context.
        unsafe { engine.ecs.set_row(ctx, saved) };
    }
    // SAFETY: the original system takes its own context.
    unsafe { next.get()(ctx) };
}

/// One row's character and the words to write its gains.
///
/// # Safety
///
/// `words` are the current row of `player_level`'s query.
unsafe fn read_character(engine: &Engine, words: &[u64; WORDS]) -> Option<Character> {
    let experience = words[EXPERIENCE] as *const u8;
    let level = words[LEVEL] as *const u8;
    let registry = words[REGISTRY];
    let a = &engine.attributes;
    // SAFETY: the row's `Level` and attribute registry.
    let level = unsafe { a.value(registry, level, engine.level_attribute) }?;
    // SAFETY: the row's `Experience` and attribute registry.
    let progress = unsafe { a.value(registry, experience, PROGRESS) }?;
    // SAFETY: as above.
    let need = unsafe { a.value(registry, experience, NEED) }?;
    // SAFETY: as above.
    let gained = unsafe { a.value(registry, experience, GAINED) }?;
    Some(Character {
        level,
        progress,
        need,
        gained,
    })
}

/// The game's level table, read once with the balancing registry of a row.
///
/// # Safety
///
/// `words` are the current row of `player_level`'s query.
unsafe fn level_table(engine: &Engine, words: &[u64; WORDS]) -> &'static Levels {
    LEVELS.get_or_init(|| {
        let balancing = words[BALANCING];
        // SAFETY: the game's cap lookup with the row's balancing registry.
        let cap = unsafe { (engine.level_cap)(balancing) };
        // SAFETY: the game's need per level, from level 1, as `player_level`
        // calls it.
        let levels = Levels::new(cap, |level| unsafe { (engine.need_for)(level, balancing) });
        enshwick_api::info!(
            "level cap {cap}, {} experience to reach it",
            levels.total(Character {
                level: cap,
                ..Character::default()
            })
        );
        levels
    })
}

/// Moves the high-water mark and raises every character below it.
///
/// # Safety
///
/// `ctx` is the live context of `player_level`.
unsafe fn share_experience(engine: &Engine, ctx: *mut u64) {
    // First pass: the leader's gain and the furthest character.
    // SAFETY: the system's live context.
    unsafe { engine.ecs.set_row(ctx, 0) };
    // SAFETY: as above, with the system's iterator size.
    let mut query = unsafe { engine.ecs.query::<WORDS>(ctx) };
    let mut table = LEVELS.get();
    let (mut most_gained, mut furthest, mut rows) = (0u32, 0u64, 0u32);
    while let Some(words) = query.next() {
        // SAFETY: the words of the current row.
        let Some(character) = (unsafe { read_character(engine, words) }) else {
            continue;
        };
        // SAFETY: as above.
        let levels = *table.get_or_insert_with(|| unsafe { level_table(engine, words) });
        most_gained = most_gained.max(character.gained);
        furthest = furthest.max(levels.total(character));
        rows += 1;
    }
    let Some(levels) = table else {
        return;
    };
    if rows == 0 {
        return;
    }
    let previous = HIGH_WATER.load(Ordering::Relaxed);
    let mark = high_water(previous, most_gained, furthest);
    if mark != previous {
        HIGH_WATER.store(mark, Ordering::Relaxed);
    }
    // Second pass: raise each character below the mark.
    // SAFETY: the system's live context.
    unsafe { engine.ecs.set_row(ctx, 0) };
    // SAFETY: as above, with the system's iterator size.
    let mut query = unsafe { engine.ecs.query::<WORDS>(ctx) };
    while let Some(words) = query.next() {
        // SAFETY: the words of the current row.
        let Some(character) = (unsafe { read_character(engine, words) }) else {
            continue;
        };
        let added = levels.catch_up(character, mark);
        if added == 0 {
            continue;
        }
        // SAFETY: the change-tracking words of the current row, which
        // `player_level` writes.
        unsafe {
            engine.ecs.mark_changed(
                words[TRACKER],
                words[STAMP_A],
                words[STAMP_B],
                words[ENTITY],
                words[KIND],
            );
        }
        let experience = words[EXPERIENCE] as *mut u8;
        // SAFETY: the row's `Experience`, which this system writes.
        let _written = unsafe {
            engine.attributes.set(
                words[REGISTRY],
                experience,
                GAINED,
                character.gained + added,
            )
        };
    }
}

/// Writes the mark on a thread of its own.
fn save(store: &'static Store<Saved>) {
    let saved = Saved {
        high_water: HIGH_WATER.load(Ordering::Relaxed),
    };
    let spawned = std::thread::Builder::new()
        .name("shared-experience-save".into())
        .spawn(move || {
            if let Err(off) = store.save(&saved) {
                enshwick_api::warn!("experience mark not saved: {off}");
            }
        });
    if spawned.is_err() {
        enshwick_api::warn!("experience mark not saved: no thread");
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct SharedExperience;

impl Mod for SharedExperience {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(
            id: "shared-experience",
            side: SERVER,
            updates: github("themaiby/enshwick", "enshwick-shared-experience"),
            builds: [
                {
                    side: SERVER,
                    guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
                    age: 387,
                    symbols: [
                        "experience.need_for_level" => 0x93_22c0,
                        "experience.level_cap" => 0x93_3f30,
                    ],
                },
                {
                    side: CLIENT,
                    guid: "69f0e9573b32c24a8838d3fff550824c",
                    age: 410,
                    symbols: [
                        "experience.need_for_level" => 0xe7_dad0,
                        "experience.level_cap" => 0xe7_f720,
                    ],
                },
            ],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        ENABLED.store(settings.enabled, Ordering::Relaxed);
        let (need_for, level_cap) = (
            load.symbol("experience.need_for_level")?,
            load.symbol("experience.level_cap")?,
        );
        let engine = Engine {
            ecs: Ecs::resolve(load)?,
            attributes: Attributes::resolve(load)?,
            worlds: Worlds::resolve(load)?,
            // SAFETY: the mod pins this id to the need per level.
            need_for: unsafe { function(need_for) },
            // SAFETY: the mod pins this id to the level cap.
            level_cap: unsafe { function(level_cap) },
            level_attribute: u32::try_from(load.constant("altar.level_attribute")?)
                .map_err(|_| Off::new("altar.level_attribute out of range"))?,
        };
        let _first = ENGINE.set(engine);
        load.hook(&PLAYER_LEVEL, load.system("player_level")?)?;
        if load.host().build().side == Side::SERVER {
            let store: &'static Store<Saved> =
                Box::leak(Box::new(Store::new(&load.data_dir()?, "experience")));
            HIGH_WATER.store(store.load()?.high_water, Ordering::Relaxed);
            load.on::<WorldSaved>(move |()| save(store))?;
            // The process may end soon after: write before returning.
            load.on::<Shutdown>(move |()| {
                let saved = Saved {
                    high_water: HIGH_WATER.load(Ordering::Relaxed),
                };
                if let Err(off) = store.save(&saved) {
                    enshwick_api::warn!("experience mark not saved: {off}");
                }
            })?;
        } else {
            // A hosted game starts from the characters in it, its host among
            // them; the next game may be another world.
            load.on::<Shutdown>(|()| HIGH_WATER.store(0, Ordering::Relaxed))?;
        }
        load.on::<WorldLoaded>(|()| {
            enshwick_api::info!("experience mark {}", HIGH_WATER.load(Ordering::Relaxed));
        })?;
        load.on_config_change(|settings: Settings| {
            ENABLED.store(settings.enabled, Ordering::Relaxed);
            enshwick_api::info!("shared experience {}", on_off(settings.enabled));
        })?;
        enshwick_api::info!("shared experience {}", on_off(settings.enabled));
        Ok(Self)
    }
}

fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

enshwick_api::declare_mod!(SharedExperience);

#[cfg(test)]
mod tests {
    use enshwick_api::config::Config as _;

    use super::*;

    #[test]
    fn the_default_settings_parse() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!(settings.enabled);
    }

    extern "system" fn vanilla(_ctx: *mut u64) {
        VANILLA_RUNS.fetch_add(1, Ordering::Relaxed);
    }

    static VANILLA_RUNS: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn a_server_life_keeps_the_mark_and_hands_vanilla_the_tick_when_off() {
        let dir = std::env::temp_dir().join(format!("shared-experience-{}", std::process::id()));
        let loader = enshwick_test::FakeHost::new()
            .symbol("ecs.iterator_init", 0x1_0000)
            .symbol("ecs.iterator_next", 0x1_0010)
            .symbol("ecs.mark_changed", 0x1_0020)
            .symbol("ecs.attribute_find", 0x1_0030)
            .symbol("experience.need_for_level", 0x1_0040)
            .symbol("experience.level_cap", 0x1_0050)
            .constant("ecs.context_row", 8)
            .constant("ecs.descriptor_entity_records", 0x828)
            .constant("ecs.entity_record_size", 0x10)
            .constant("ecs.level_flags", 6)
            .constant("ecs.level_data_offset", 4)
            .constant("ecs.attribute_slot", 0xc)
            .constant("ecs.context_entity_world", 0x30)
            .constant("altar.level_attribute", 0x724b_f568)
            .system(
                "player_level",
                enshwick_test::address(vanilla as extern "system" fn(*mut u64)),
            )
            .settings(
                "enabled = false
",
            )
            .data_dir(dir.to_str().unwrap())
            .install();
        // SAFETY: the exports of `declare_mod!`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick_api::abi::Status::OK);
        let tick = loader.detour(&PLAYER_LEVEL).unwrap();
        // SAFETY: switched off, the link forwards without reading the context.
        unsafe { tick(core::ptr::null_mut()) };
        assert_eq!(VANILLA_RUNS.load(Ordering::Relaxed), 1);

        HIGH_WATER.store(98_765, Ordering::Relaxed);
        assert_eq!(loader.raise::<Shutdown>(&()), 1);
        let text = std::fs::read_to_string(dir.join("experience.toml")).unwrap();
        assert_eq!(toml::from_str::<Saved>(&text).unwrap().high_water, 98_765);
        let _cleaned = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_saved_mark_round_trips() {
        let text = toml::to_string(&Saved {
            high_water: 123_456,
        })
        .unwrap();
        let saved: Saved = toml::from_str(&text).unwrap();
        assert_eq!(saved.high_water, 123_456);
    }
}
