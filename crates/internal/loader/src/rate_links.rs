//! The game functions game rates scale: the getters of the game's own
//! factors and the kill experience formula, each linked when a mod declared
//! a rate that needs it. The detours scale what the game answers and leave
//! its settings alone, so the server never saves a scaled value to its
//! configuration.

use enshwick::abi::{HookFlags, RateId};

use crate::chains::Next;
use crate::fuse::Fuse;
use crate::host::Loader;
use crate::rates::{LINKS, RATES, experience_rate, scale_count};

/// `f32 (data)`: a factor getter of the game's data object, in `xmm0`.
type FactorFn = unsafe extern "system" fn(data: *const u8) -> f32;
/// `f32 (data, source)`: the experience factor for an experience source.
type SourceFactorFn = unsafe extern "system" fn(data: *const u8, source: u8) -> f32;
/// `i64 (level, other level, xp mod, group size, data)`: the rounded
/// experience of a kill, with the combat factor already in it.
type KillFn = unsafe extern "system" fn(
    level: u32,
    other: *const u8,
    xp_mod: f32,
    group: i32,
    data: *const u8,
) -> i64;

// SAFETY: `FactorFn` is the signature of the table's
// `settings.resource_drops` on every build that has it, and this slot is
// the `next` of the one link `link` makes on it, whose detour
// `resource_drops` calls `get`.
static DROPS_NEXT: Next<FactorFn> = unsafe { Next::new() };
// SAFETY: `FactorFn` is the signature of the table's
// `settings.factory_speed`, and this slot is the `next` of the one link
// `link` makes on it, whose detour `factory_speed` calls `get`.
static FACTORY_NEXT: Next<FactorFn> = unsafe { Next::new() };
// SAFETY: `SourceFactorFn` is the signature of the table's
// `experience.source_factor`, and this slot is the `next` of the one link
// `link` makes on it, whose detour `source_factor` calls `get`.
static SOURCE_NEXT: Next<SourceFactorFn> = unsafe { Next::new() };
// SAFETY: `KillFn` is the signature of the table's `experience.kill`, and
// this slot is the `next` of the one link `link` makes on it, whose detour
// `kill` calls `get`.
static KILL_NEXT: Next<KillFn> = unsafe { Next::new() };

/// The rates' switch.
static FUSE: Fuse = Fuse::new("game rates");

/// The factor in force for `rate`, 1 once the rates' fuse is off.
fn factor(rate: RateId) -> f32 {
    FUSE.run("a game rate", || RATES.factor(rate, crate::host::is_off))
        .unwrap_or(1.0)
}

/// Whether a rate some mod declared links `symbol`.
fn wanted(symbol: &str) -> bool {
    LINKS
        .iter()
        .any(|&(rate, symbols)| symbols.contains(&symbol) && RATES.declared(rate))
}

/// Seals the rates mods declared and links the functions they need.
pub(crate) fn link(loader: &'static Loader) {
    RATES.seal();
    let address = |name| loader.address(name);
    let mut linked = 0;
    if wanted("settings.resource_drops") {
        linked += usize::from(loader.link_feature(
            "the resource drop rate",
            address("settings.resource_drops"),
            &DROPS_NEXT,
            resource_drops,
            HookFlags::NONE,
        ));
    }
    if wanted("settings.factory_speed") {
        linked += usize::from(loader.link_feature(
            "the workshop speed rate",
            address("settings.factory_speed"),
            &FACTORY_NEXT,
            factory_speed,
            HookFlags::NONE,
        ));
    }
    if wanted("experience.source_factor") {
        linked += usize::from(loader.link_feature(
            "the experience rates",
            address("experience.source_factor"),
            &SOURCE_NEXT,
            source_factor,
            HookFlags::NONE,
        ));
    }
    if wanted("experience.kill") {
        linked += usize::from(loader.link_feature(
            "the combat experience rate",
            address("experience.kill"),
            &KILL_NEXT,
            kill,
            HookFlags::NONE,
        ));
    }
    if linked > 0 {
        crate::log!(INFO, "game rates: {linked} game functions scaled by mods");
    }
}

unsafe extern "system" fn resource_drops(data: *const u8) -> f32 {
    // SAFETY: the game's argument to `settings.resource_drops`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    let vanilla = unsafe { DROPS_NEXT.get()(data) };
    vanilla * factor(RateId::RESOURCE_DROPS)
}

/// The game's answer is how long production takes per unit of time, the
/// inverse of its speed factor, 0 for a factor of about 0 (server 399,
/// client 426): a faster rate divides it, and a rate of 0 answers 0, as the
/// game's own setting at 0 does.
unsafe extern "system" fn factory_speed(data: *const u8) -> f32 {
    // SAFETY: the game's argument to `settings.factory_speed`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    let vanilla = unsafe { FACTORY_NEXT.get()(data) };
    let speed = factor(RateId::WORKSHOP_SPEED);
    if speed > 0.0 { vanilla / speed } else { 0.0 }
}

unsafe extern "system" fn source_factor(data: *const u8, source: u8) -> f32 {
    // SAFETY: the game's arguments to `experience.source_factor`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    let vanilla = unsafe { SOURCE_NEXT.get()(data, source) };
    experience_rate(source).map_or(vanilla, |rate| vanilla * factor(rate))
}

unsafe extern "system" fn kill(
    level: u32,
    other: *const u8,
    xp_mod: f32,
    group: i32,
    data: *const u8,
) -> i64 {
    // SAFETY: the game's arguments to `experience.kill`, passed on unchanged
    // to the rest of the chain, which the slot holds since the chain is
    // wired before the target jumps here.
    let vanilla = unsafe { KILL_NEXT.get()(level, other, xp_mod, group, data) };
    scale_count(vanilla, factor(RateId::EXPERIENCE_COMBAT))
}
