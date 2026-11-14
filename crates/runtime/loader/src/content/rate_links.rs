//! The game functions game rates scale: the getters of the game's own
//! factors and the kill experience formula, each linked when a mod declared
//! a rate that needs it. The detours scale what the game answers and leave
//! its settings alone, so the server never saves a scaled value to its
//! configuration.

use std::sync::OnceLock;

use enshwick_core::abi::{Event, HookFlags, RateId, Status};
use enshwick_core::content::recipes::RecipeLayout;

use crate::content::rates::{
    LINKS, RATES, experience_rates, scale_points, scale_stack, scale_whole, workshop_duration,
};
use crate::engine::chains::Next;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned::{self, Symbol};
use crate::inventory::loot_slot::SlotFields;

/// The build table ids this part of the loader names; the comfort rate
/// shares the comfort link of [`crate::knowledge::progress`].
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::experience::KILL.id(),
    pinned::experience::SOURCE_FACTOR.id(),
    pinned::loot::STACK_ROLL.id(),
    pinned::settings::FACTORY_SPEED.id(),
    pinned::settings::RESOURCE_DROPS.id(),
    pinned::skill::POINTS_PER_LEVEL.id(),
];

/// The switch of each rate's link, named as the log and the Mods screen
/// name the rate: a fault in one turns that link off, and the other rates
/// keep running.
static DROPS_FUSE: Fuse = Fuse::new("the resource drop rate");
static FACTORY_FUSE: Fuse = Fuse::new("the workshop speed rate");
static EXPERIENCE_FUSE: Fuse = Fuse::new("the experience rates");
static KILL_FUSE: Fuse = Fuse::new("the combat experience rate");
static POINTS_FUSE: Fuse = Fuse::new("the skill point rate");
static LOOT_FUSE: Fuse = Fuse::new("the loot rate");

/// What the detours read, set when they link.
struct Linked {
    /// The experience rate of each of the game's experience source values.
    experience: [Option<RateId>; 256],
    /// Where a rolled loot slot keeps its item and count, from reflection;
    /// `None` keeps the loot rate off.
    slot: Option<SlotFields>,
}

static LINKED: OnceLock<Linked> = OnceLock::new();

/// The switch of the factors in force, which every rate reads: a fault
/// there leaves every rate at 1.
static RATES_FUSE: Fuse = Fuse::new("game rates");

/// The factor in force for `rate`, 1 once the rates' fuse is off.
pub(crate) fn factor(rate: RateId) -> f32 {
    RATES_FUSE
        .run("a game rate", || {
            RATES.factor(rate, crate::engine::loader::is_off)
        })
        .unwrap_or(1.0)
}

/// Whether a rate some mod declared links `symbol`.
pub(crate) fn wanted(symbol: Symbol) -> bool {
    LINKS
        .iter()
        .any(|&(rate, symbols)| symbols.contains(&symbol) && RATES.declared(rate))
}

/// Seals the rates mods declared and links the functions they need; the
/// comfort rate rides on the comfort link of [`crate::knowledge::progress`], which
/// links after this.
pub(crate) fn link(loader: &'static Loader) {
    RATES.seal();
    let slot = SlotFields::reflect(&loader.reflection);
    let _first = LINKED.set(Linked {
        experience: experience_rates(|id| {
            crate::engine::catalogue::constant(id, &loader.reflection)
        }),
        slot: slot.as_ref().ok().copied(),
    });
    let loot = match &slot {
        Ok(_) => link_one(
            loader,
            &LOOT_FUSE,
            pinned::loot::STACK_ROLL,
            &STACK_NEXT,
            stack_roll,
        ),
        Err(why) => {
            wanted(pinned::loot::STACK_ROLL)
                && loader.fuse_off(&LOOT_FUSE, why.entry(), &why.to_string())
        }
    };
    let linked = [
        link_one(
            loader,
            &DROPS_FUSE,
            pinned::settings::RESOURCE_DROPS,
            &DROPS_NEXT,
            resource_drops,
        ),
        link_one(
            loader,
            &FACTORY_FUSE,
            pinned::settings::FACTORY_SPEED,
            &FACTORY_NEXT,
            factory_speed,
        ),
        link_one(
            loader,
            &EXPERIENCE_FUSE,
            pinned::experience::SOURCE_FACTOR,
            &SOURCE_NEXT,
            source_factor,
        ),
        link_one(
            loader,
            &KILL_FUSE,
            pinned::experience::KILL,
            &KILL_NEXT,
            kill,
        ),
        link_one(
            loader,
            &POINTS_FUSE,
            pinned::skill::POINTS_PER_LEVEL,
            &POINTS_NEXT,
            skill_points,
        ),
        loot,
    ]
    .into_iter()
    .filter(|&linked| linked)
    .count();
    if linked > 0 {
        crate::log!(INFO, "game rates: {linked} game functions scaled by mods");
    }
}

/// Links `detour` of the rate `feature` on `symbol` when a declared rate
/// needs it and the code there is what the table expects; whether it
/// linked.
fn link_one<F: Copy>(
    loader: &Loader,
    feature: &'static Fuse,
    symbol: Symbol,
    next: &'static Next<F>,
    detour: F,
) -> bool {
    wanted(symbol)
        && loader.link_fused(
            feature,
            Target::Pinned(symbol),
            next,
            detour,
            HookFlags::NONE,
        )
}

crate::engine::chains::chain_detour! {
    /// `f32 (data)`, the table's `settings.resource_drops`: a factor getter
    /// of the game's data object, in `xmm0` (the rates engine report,
    /// servers 387 and 399, clients 410 and 426).
    fn resource_drops(data: *const u8) -> f32;
    next DROPS_NEXT;
    fuse &DROPS_FUSE, "the resource drop rate";
    |onward| { onward.call() * factor(RateId::RESOURCE_DROP_STACK_AMOUNT) }
}

crate::engine::chains::chain_detour! {
    /// `f32 (data)`, the table's `settings.factory_speed`: how long
    /// production takes per unit of time, the inverse of its speed setting
    /// (the rates engine report, servers 387 and 399, clients 410 and 426).
    fn factory_speed(data: *const u8) -> f32;
    next FACTORY_NEXT;
    fuse &FACTORY_FUSE, "the workshop speed rate";
    |onward| { workshop_duration(onward.call(), factor(RateId::FACTORY_PRODUCTION_SPEED)) }
}

crate::engine::chains::chain_detour! {
    /// `f32 (data, source)`, the table's `experience.source_factor`: the
    /// experience factor for an experience source (the rates engine
    /// report, servers 387 and 399, clients 410 and 426).
    fn source_factor(data: *const u8, source: u8) -> f32;
    next SOURCE_NEXT;
    fuse &EXPERIENCE_FUSE, "the experience rates";
    |onward| {
        let vanilla = onward.call();
        LINKED
            .get()
            .and_then(|linked| linked.experience[usize::from(source)])
            .map_or(vanilla, |rate| vanilla * factor(rate))
    }
}

crate::engine::chains::chain_detour! {
    /// `i64 (level, other level, xp mod, group size, data)`, the table's
    /// `experience.kill`: the rounded experience of a kill, with the combat
    /// factor already in it (the rates engine report, servers 387 and 399,
    /// clients 410 and 426).
    fn kill(level: u32, other: *const u8, xp_mod: f32, group: i32, data: *const u8) -> i64;
    next KILL_NEXT;
    fuse &KILL_FUSE, "the combat experience rate";
    |onward| { scale_whole(onward.call(), factor(RateId::EXPERIENCE_COMBAT)) }
}

crate::engine::chains::chain_detour! {
    /// `u32 (data)`, the table's `skill.points_per_level`: the skill points
    /// a character gains per level, in `eax` (the rates engine report,
    /// servers 387 and 399, clients 410 and 426).
    fn skill_points(data: *const u8) -> u32;
    next POINTS_NEXT;
    fuse &POINTS_FUSE, "the skill point rate";
    |onward| { scale_points(onward.call(), factor(RateId::SKILL_POINTS)) }
}

crate::engine::chains::chain_detour! {
    /// `void (out slot, item id, context, rarity)`, the table's
    /// `loot.stack_roll`: rolls one stack of random loot into `out`; every
    /// loot roll of the game goes through it (the rates engine report,
    /// servers 387 and 399, clients 410 and 426).
    fn stack_roll(out: *mut u8, item: *const u32, context: *mut u8, rarity: *const u8) -> ();
    next STACK_NEXT;
    fuse &LOOT_FUSE, "the loot rate";
    |onward| {
        onward.call();
        let rate = factor(RateId::LOOT);
        if !out.is_null() && rate.to_bits() != 1.0_f32.to_bits() {
            // SAFETY: `out` is the game's non-null slot (checked first),
            // which the roll just wrote and its caller reads only after this
            // returns.
            unsafe { scale_slot(out, rate) };
        }
    }
}

/// Scales the count of the stack the game rolled into `slot`, held to the
/// item's stack size; leaves it as rolled when the item is not found.
///
/// # Safety
///
/// - `slot` points at a `LootSlotInfo` the game rolled, writable for its
///   reflected size, which no other thread touches during the call.
#[cold]
unsafe fn scale_slot(slot: *mut u8, rate: f32) {
    let Some((loader, fields)) =
        crate::engine::loader::loader().zip(LINKED.get().and_then(|linked| linked.slot))
    else {
        return;
    };
    let slot = slot.addr();
    // SAFETY: the caller's slot holds the item id, a `u32`, at the reflected
    // `itemId`, which `SlotFields::checked` keeps inside the slot.
    let item: u32 = unsafe { crate::engine::memory::read(slot, fields.item) };
    // SAFETY: the caller's slot holds the stack's count, a `u16`, at the
    // reflected `stackSize`, which `SlotFields::checked` keeps inside the
    // slot.
    let count: u16 = unsafe { crate::engine::memory::read(slot, fields.count) };
    let Ok(info) = crate::inventory::transactions::item_info(loader, item) else {
        return;
    };
    // SAFETY: the caller's slot is writable at the reflected `stackSize`, a
    // `u16` inside the slot.
    unsafe {
        crate::engine::memory::write(slot, fields.count, scale_stack(count, rate, info.max_stack));
    };
}

/// Whether some mod declared a recipe output rate.
pub(crate) fn recipes_rated() -> bool {
    RATES.any(crate::content::rates::is_recipe_output)
}

/// The recipe output rates' state: where a recipe sits in the registry on
/// the running build, and their own switch, so a fault in the recipe patch
/// leaves the other rates running.
#[derive(Debug)]
struct RecipeRates {
    layout: OnceLock<Option<RecipeLayout>>,
    fuse: Fuse,
}

static RECIPE_RATES: RecipeRates = RecipeRates {
    layout: OnceLock::new(),
    fuse: Fuse::new("recipe output rates"),
};

/// The loader's patch of the recipe registry for the recipe output rates,
/// when a mod declared one and the build reflects the recipe layout.
pub(crate) fn recipe_patch(loader: &Loader) -> Option<enshwick_core::abi::ResourcePatchFn> {
    if !recipes_rated() {
        return None;
    }
    let layout = RECIPE_RATES.layout.get_or_init(|| {
        RecipeLayout::resolve(&crate::engine::catalogue::Reflected(&loader.reflection))
            .map_err(|off| {
                let why = format!("does not resolve: {off}");
                loader.fuse_off(
                    &RECIPE_RATES.fuse,
                    crate::content::recipes::RECIPE_LAYOUT,
                    &why,
                )
            })
            .ok()
    });
    layout
        .is_some()
        .then_some(scale_recipes as enshwick_core::abi::ResourcePatchFn)
}

/// Builds the recipe registry again with the factors in force, once every
/// mod is initialised: on a world load, when a dedicated server knows the
/// kinds of the items, and when a recipe output factor in force changes.
pub(crate) fn rebuild_recipes() {
    if !RATES.is_sealed() || !recipes_rated() {
        return;
    }
    let status = crate::content::served::rebuild(
        crate::engine::mod_index::ModIndex::LOADER,
        crate::content::served::recipe_registry(),
    );
    if status != Status::OK {
        crate::log!(
            WARN,
            "recipe output rates: the recipes were not rebuilt ({})",
            status.0
        );
    }
}

/// Follows the world: a loaded world's items tell the recipes' kinds.
pub(crate) fn world_event(event: Event) {
    if event == Event::WORLD_LOADED {
        rebuild_recipes();
    }
}

/// Scales what each recipe of the registry at `base` makes by the recipe
/// output rate of its kind; a recipe whose item the game has not loaded
/// yet keeps its count until a rebuild. Hands back the registry unchanged
/// when the recipe rates' switch is off or a fault turns it off, so the
/// game never loses its recipes and a rebuild keeps this patch in its run.
///
/// # Safety
///
/// - `base` is valid for `len` bytes for the call, and `out` and `out_len`
///   are writable, as [`enshwick_core::abi::ResourcePatchFn`] says.
unsafe extern "system" fn scale_recipes(
    base: *const u8,
    len: usize,
    _user: *mut core::ffi::c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    if base.is_null() || out.is_null() || out_len.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: the loader's resource chain passes `len` readable bytes at
    // the non-null `base` (checked above) for the call.
    let registry = unsafe { core::slice::from_raw_parts(base, len) };
    let built = RECIPE_RATES
        .layout
        .get()
        .and_then(Option::as_ref)
        .and_then(|layout| {
            RECIPE_RATES.fuse.run("the recipe output patch", || {
                crate::content::rates::scale_recipes_in(
                    layout,
                    registry,
                    &|view| view.kind(),
                    &|kind| factor(RateId::recipe_output(kind.code())),
                )
            })
        })
        .unwrap_or_else(|| registry.to_vec());
    // SAFETY: `out` and `out_len` are non-null (checked above) and writable
    // for the call, as `ResourcePatchFn` says.
    unsafe { crate::content::served::hand_out(built, out, out_len) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_no_mod_declared_leaves_the_games_value_as_it_is() {
        assert!(!RATES.declared(RateId::LOOT));
        assert_eq!(factor(RateId::LOOT).to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn a_panic_in_the_loot_rate_turns_off_the_loot_rate_alone() {
        let ran = LOOT_FUSE.run("the loot rate", || panic!("broken roll"));
        assert!(ran.is_none());
        assert!(LOOT_FUSE.is_off());
        for other in [
            &DROPS_FUSE,
            &FACTORY_FUSE,
            &EXPERIENCE_FUSE,
            &KILL_FUSE,
            &POINTS_FUSE,
            &RATES_FUSE,
        ] {
            assert!(!other.is_off(), "{} keeps running", other.feature());
        }
        assert!(
            crate::engine::diag::parts_off()
                .iter()
                .any(|p| p.part == "the loot rate" && p.reason.contains("broken roll")),
            "the loot rate shows off by its own name"
        );
    }

    #[test]
    fn a_function_no_declared_rate_needs_is_never_linked_nor_its_rate_turned_off() {
        static UNWANTED: Fuse = Fuse::new("test unwanted rate");
        let loader = crate::start::tests::bare_loader();
        let symbol = pinned::text::LOOKUP;
        assert!(!wanted(symbol), "no rate links the text lookup");
        assert!(!link_one(
            &loader,
            &UNWANTED,
            symbol,
            &DROPS_NEXT,
            resource_drops
        ));
        assert!(!UNWANTED.is_off(), "nothing tried to link it");
    }

    #[test]
    fn every_linked_symbol_has_its_table_id() {
        for (rate, symbols) in LINKS {
            for symbol in *symbols {
                assert!(
                    crate::parts::table_ids()
                        .iter()
                        .any(|(_, ids)| ids.contains(&symbol.id())),
                    "{rate:?}: {symbol}"
                );
            }
        }
    }
}
