//! The game functions game rates scale: the getters of the game's own
//! factors and the kill experience formula, each linked when a mod declared
//! a rate that needs it. The detours scale what the game answers and leave
//! its settings alone, so the server never saves a scaled value to its
//! configuration.

use std::sync::OnceLock;

use enshwick::abi::{HookFlags, RateId};

use crate::chains::Next;
use crate::fuse::Fuse;
use crate::host::Loader;
use crate::pinned::{self, Symbol};
use crate::rates::{
    LINKS, RATES, experience_rates, scale_points, scale_stack, scale_whole, workshop_duration,
};

/// The build table ids this part of the loader names; the comfort rate
/// shares the comfort link of [`crate::progress`].
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::experience::KILL.id(),
    pinned::experience::SOURCE_FACTOR.id(),
    pinned::loot::STACK_ROLL.id(),
    pinned::settings::FACTORY_SPEED.id(),
    pinned::settings::RESOURCE_DROPS.id(),
    pinned::skill::POINTS_PER_LEVEL.id(),
];

/// Where a rolled loot slot (`LootSlotInfo`, 16 bytes) keeps its item id
/// (`u32`) and its stack's count (`u16`), on server 399 and client 426.
const SLOT_ITEM: usize = 0;
const SLOT_COUNT: usize = 4;

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

/// `void (out slot, item id, context, rarity)`: rolls one stack of random
/// loot into `out`; every loot roll of the game goes through it.
type StackRollFn =
    unsafe extern "system" fn(out: *mut u8, item: *const u32, context: *mut u8, rarity: *const u8);

/// `u32 (data)`: the skill points a character gains per level, in `eax`.
type PointsFn = unsafe extern "system" fn(data: *const u8) -> u32;

// SAFETY: `FactorFn` is the signature of the table's
// `settings.resource_drops` on servers 387 and 399 and clients 410 and 426 (the rates engine report),
// and this slot is
// the `next` of the one link `link` makes on it, whose detour
// `resource_drops` calls `get`.
static DROPS_NEXT: Next<FactorFn> = unsafe { Next::new() };
// SAFETY: `FactorFn` is the signature of the table's `settings.factory_speed` on servers
// 387 and 399 and clients 410 and 426 (the rates engine report), and this slot is the `next` of the one link
// `link` makes on it, whose detour `factory_speed` calls `get`.
static FACTORY_NEXT: Next<FactorFn> = unsafe { Next::new() };
// SAFETY: `SourceFactorFn` is the signature of the table's `experience.source_factor` on servers
// 387 and 399 and clients 410 and 426 (the rates engine report), and this slot is the `next` of the one link
// `link` makes on it, whose detour `source_factor` calls `get`.
static SOURCE_NEXT: Next<SourceFactorFn> = unsafe { Next::new() };
// SAFETY: `KillFn` is the signature of the table's `experience.kill` on servers
// 387 and 399 and clients 410 and 426 (the rates engine report), and this slot is the `next` of the one link `link` makes on it, whose detour
// `kill` calls `get`.
static KILL_NEXT: Next<KillFn> = unsafe { Next::new() };

// SAFETY: `PointsFn` is the signature of the table's `skill.points_per_level` on servers
// 387 and 399 and clients 410 and 426 (the rates engine report), and this slot is the `next` of the one link
// `link` makes on it, whose detour `skill_points` calls `get`.
static POINTS_NEXT: Next<PointsFn> = unsafe { Next::new() };

// SAFETY: `StackRollFn` is the signature of the table's `loot.stack_roll` on servers
// 387 and 399 and clients 410 and 426 (the rates engine report), and this slot is the `next` of the one link `link` makes on it, whose
// detour `stack_roll` calls `get`.
static STACK_NEXT: Next<StackRollFn> = unsafe { Next::new() };

/// What the detours read, set when they link.
struct Linked {
    /// The loader, for the item lookups of the loot rate.
    loader: &'static Loader,
    /// The experience rate of each of the game's experience source values.
    experience: [Option<RateId>; 256],
}

static LOADED: OnceLock<Linked> = OnceLock::new();

/// The rates' switch.
static FUSE: Fuse = Fuse::new("game rates");

/// The factor in force for `rate`, 1 once the rates' fuse is off.
pub(crate) fn factor(rate: RateId) -> f32 {
    FUSE.run("a game rate", || RATES.factor(rate, crate::host::is_off))
        .unwrap_or(1.0)
}

/// Whether a rate some mod declared links `symbol`.
pub(crate) fn wanted(symbol: Symbol) -> bool {
    LINKS
        .iter()
        .any(|&(rate, symbols)| symbols.contains(&symbol) && RATES.declared(rate))
}

/// Seals the rates mods declared and links the functions they need; the
/// comfort rate rides on the comfort link of [`crate::progress`], which
/// links after this.
pub(crate) fn link(loader: &'static Loader) {
    RATES.seal();
    let _first = LOADED.set(Linked {
        loader,
        experience: experience_rates(|id| crate::catalogue::constant(id, &loader.reflection)),
    });
    let linked = [
        link_one(
            loader,
            "the resource drop rate",
            pinned::settings::RESOURCE_DROPS,
            &DROPS_NEXT,
            resource_drops as FactorFn,
        ),
        link_one(
            loader,
            "the workshop speed rate",
            pinned::settings::FACTORY_SPEED,
            &FACTORY_NEXT,
            factory_speed as FactorFn,
        ),
        link_one(
            loader,
            "the experience rates",
            pinned::experience::SOURCE_FACTOR,
            &SOURCE_NEXT,
            source_factor as SourceFactorFn,
        ),
        link_one(
            loader,
            "the combat experience rate",
            pinned::experience::KILL,
            &KILL_NEXT,
            kill as KillFn,
        ),
        link_one(
            loader,
            "the skill point rate",
            pinned::skill::POINTS_PER_LEVEL,
            &POINTS_NEXT,
            skill_points as PointsFn,
        ),
        link_one(
            loader,
            "the loot rate",
            pinned::loot::STACK_ROLL,
            &STACK_NEXT,
            stack_roll as StackRollFn,
        ),
    ]
    .into_iter()
    .filter(|&linked| linked)
    .count();
    if linked > 0 {
        crate::log!(INFO, "game rates: {linked} game functions scaled by mods");
    }
}

/// Links `detour` on `symbol` when a declared rate needs it and the code
/// there is what the table expects; whether it linked.
fn link_one<F: Copy>(
    loader: &Loader,
    feature: &str,
    symbol: Symbol,
    next: &'static Next<F>,
    detour: F,
) -> bool {
    if !wanted(symbol) {
        return false;
    }
    let target = loader.address(symbol).filter(|_| loader.code_holds(symbol));
    let Some(target) = target else {
        crate::log!(
            WARN,
            "{feature} off on {}: the code at `{symbol}` is not what the table expects",
            loader.build.name
        );
        return false;
    };
    loader.link_feature(feature, Some(target), next, detour, HookFlags::NONE)
}

unsafe extern "system" fn resource_drops(data: *const u8) -> f32 {
    // SAFETY: the game's argument to `settings.resource_drops`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    let vanilla = unsafe { DROPS_NEXT.get()(data) };
    vanilla * factor(RateId::RESOURCE_DROPS)
}

/// The game's answer is how long production takes per unit of time, the
/// inverse of its speed setting (the rates engine report, servers 387 and
/// 399, clients 410 and 426).
unsafe extern "system" fn factory_speed(data: *const u8) -> f32 {
    // SAFETY: the game's argument to `settings.factory_speed`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    let vanilla = unsafe { FACTORY_NEXT.get()(data) };
    workshop_duration(vanilla, factor(RateId::WORKSHOP_SPEED))
}

unsafe extern "system" fn source_factor(data: *const u8, source: u8) -> f32 {
    // SAFETY: the game's arguments to `experience.source_factor`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    let vanilla = unsafe { SOURCE_NEXT.get()(data, source) };
    LOADED
        .get()
        .and_then(|linked| linked.experience[usize::from(source)])
        .map_or(vanilla, |rate| vanilla * factor(rate))
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
    scale_whole(vanilla, factor(RateId::EXPERIENCE_COMBAT))
}

unsafe extern "system" fn skill_points(data: *const u8) -> u32 {
    // SAFETY: the game's argument to `skill.points_per_level`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    let vanilla = unsafe { POINTS_NEXT.get()(data) };
    scale_points(vanilla, factor(RateId::SKILL_POINTS))
}

unsafe extern "system" fn stack_roll(
    out: *mut u8,
    item: *const u32,
    context: *mut u8,
    rarity: *const u8,
) {
    // SAFETY: the game's arguments to `loot.stack_roll`, passed on unchanged
    // to the rest of the chain, which the slot holds since the chain is
    // wired before the target jumps here.
    unsafe { STACK_NEXT.get()(out, item, context, rarity) };
    let rate = factor(RateId::LOOT);
    if out.is_null() || rate.to_bits() == 1.0_f32.to_bits() {
        return;
    }
    // SAFETY: `out` is the game's non-null slot (checked above), which the
    // roll just wrote and its caller reads only after this returns.
    let _done = FUSE.run("the loot rate", || unsafe { scale_slot(out, rate) });
}

/// Scales the count of the stack the game rolled into `slot`, held to the
/// item's stack size; leaves it as rolled when the item is not found.
///
/// # Safety
///
/// - `slot` points at a `LootSlotInfo` the game rolled, writable for 16
///   bytes, which no other thread touches during the call.
#[cold]
unsafe fn scale_slot(slot: *mut u8, rate: f32) {
    let slot = slot.addr();
    // SAFETY: the caller's slot holds the item id at `SLOT_ITEM`.
    let item: u32 = unsafe { crate::memory::read(slot, SLOT_ITEM) };
    // SAFETY: the caller's slot holds the stack's count at `SLOT_COUNT`.
    let count: u16 = unsafe { crate::memory::read(slot, SLOT_COUNT) };
    let Some(max) = LOADED
        .get()
        .and_then(|linked| crate::inventory::item_info(linked.loader, item).ok())
        .map(|info| info.max_stack)
    else {
        return;
    };
    // SAFETY: the caller's slot is writable at `SLOT_COUNT`, a `u16`.
    unsafe { crate::memory::write(slot, SLOT_COUNT, scale_stack(count, rate, max)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_linked_symbol_has_its_table_id() {
        for (rate, symbols) in LINKS {
            for symbol in *symbols {
                assert!(
                    crate::table_ids::TABLE_IDS
                        .iter()
                        .any(|(_, ids)| ids.contains(&symbol.id())),
                    "{rate:?}: {symbol}"
                );
            }
        }
    }
}
