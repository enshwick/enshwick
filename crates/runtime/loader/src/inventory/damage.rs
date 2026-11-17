//! Damage filters: mods see and change damage before it reaches an entity's
//! health, by cause and, for hits, by damage type.
//!
//! A hit's damage is worked out in the impact node `ApplyDamage` as a vector
//! of one amount per damage type; the loader hooks the point after the
//! target's resistances and before its armour, so the armour, the block and
//! the hit the game raises all follow what the filters leave. Falls, the
//! shroud, drowning and freezing take health without a hit, in
//! `fall_damage_infliction`, `fog_resistance_update`, `oxygen_update` and
//! `bodyheat_depletion`; the loader hooks their writes to the health
//! attribute, and the fall system's own death decision follows the health
//! the filters left ([`decided`]). An administrator's kill zeroes every health attribute at
//! once, so the loader hooks the test before it: of the target's health in
//! `admin_player` for chosen enemies, of the entity's faction in
//! `debug_kill_faction` for whole factions and players, and a refused kill
//! takes the path of an entity the kill does not reach. Every site runs on
//! the game's worker threads, in parallel, so the filters run there too: no
//! allocation, no lock. The sites arm in groups by cause, each under a
//! switch of its own ([`GROUPS`]), so a site a build lacks turns off only the
//! causes it serves.

use core::ffi::c_void;
use std::sync::OnceLock;

use enshwick_core::Entity;
use enshwick_core::abi::{DamageCheck, DamageFilterFn, Header, HookFlags, MidHookFn, Registers};
use enshwick_gamedata::catalogue;

use crate::engine::filters::Filters;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Registered, Target};
use crate::engine::mid::mid_handler;
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned::{self, Constant, Symbol};
use crate::engine::profile::Feature;
use crate::engine::registry::JoinFeatureError;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::damage::ADMIN_KILL.id(),
    pinned::damage::DROWNING.id(),
    pinned::damage::DROWNING_DEATH.id(),
    pinned::damage::FACTION_KILL.id(),
    pinned::damage::FALL.id(),
    pinned::damage::FALL_DEATH.id(),
    pinned::damage::FREEZING.id(),
    pinned::damage::HIT.id(),
    pinned::damage::HIT_SOURCE.id(),
    pinned::damage::HIT_TARGET.id(),
    pinned::damage::HIT_VECTOR.id(),
    pinned::damage::SHROUD.id(),
    pinned::damage::SHROUD_DEATH.id(),
];

/// Damage types in a hit's vector, the values of the game's `DamageTypeNew`;
/// [`types_match`] holds the running build to it.
const TYPES: usize = enshwick_core::abi::DAMAGE_TYPES;
/// [`TYPES`] as the ABI's count.
const TYPE_COUNT: u32 = 13;
const _: () = assert!(TYPE_COUNT as usize == TYPES);
/// Why the filters stay off on a build whose damage types differ.
const OTHER_TYPES: &str = "has another number of values than the loader's hit vector";
/// The healing entry of a hit's vector.
const HEALING: usize = enshwick_core::damage::DamageTypeNew::Healing.index();
/// The carry flag in `rflags`.
const CARRY: u64 = 1;
/// The zero flag in `rflags`.
const ZERO: u64 = 1 << 6;

/// Null, for an admin kill's test of the target's health to read when the
/// filters refuse it.
static NO_HEALTH: u64 = 0;

static FILTERS: Filters<DamageFilterFn> = Filters::new(Feature::Damage);
static LAYOUT: OnceLock<Layout> = OnceLock::new();

/// Where each site keeps what the filters need, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    /// The hit's damage vector below `rbp`.
    vector: usize,
    /// The hit's target and source above `rbp`.
    target: usize,
    source: usize,
}

/// The entry a build lacks and why, for an off line.
type Lacking = (&'static str, &'static str);

/// A check of what a group needs beyond its sites: `Err` names the entry
/// this build lacks and why.
type Needs = fn(&Loader) -> Result<(), Lacking>;

/// The sites of one or more causes, which arm all or none under a switch of
/// their own: a site a build lacks turns off only its group, and the
/// switch's name lists the `DamageCause`s the filters then no longer get.
struct Group {
    fuse: &'static Fuse,
    /// What the group needs beyond its sites, if anything.
    needs: Option<Needs>,
    /// Each site's mid-hook target and handler.
    sites: &'static [(Symbol, MidHookFn)],
}

/// The hits, which stats read too.
const HITS: Group = Group {
    fuse: &HIT_FUSE,
    needs: Some(hit_layout),
    sites: &[(pinned::damage::HIT, hit)],
};

/// The groups. A fall's death site arms with its damage site, as a
/// filtered fall without the game's death decision following it would be
/// wrong both ways.
const GROUPS: [Group; 6] = [
    HITS,
    Group {
        fuse: &FALL_FUSE,
        needs: None,
        sites: &[
            (pinned::damage::FALL, fall),
            (pinned::damage::FALL_DEATH, fall_death),
        ],
    },
    Group {
        fuse: &SHROUD_FUSE,
        needs: None,
        sites: &[
            (pinned::damage::SHROUD, shroud),
            (pinned::damage::SHROUD_DEATH, shroud_death),
        ],
    },
    Group {
        fuse: &DROWNING_FUSE,
        needs: None,
        sites: &[
            (pinned::damage::DROWNING, drowning),
            (pinned::damage::DROWNING_DEATH, drowning_death),
        ],
    },
    Group {
        fuse: &FREEZING_FUSE,
        needs: None,
        sites: &[(pinned::damage::FREEZING, freezing)],
    },
    Group {
        fuse: &KILL_FUSE,
        needs: None,
        sites: &[
            (pinned::damage::ADMIN_KILL, admin_kill),
            (pinned::damage::FACTION_KILL, faction_kill),
        ],
    },
];

/// The switch of the hit site, for the filters and the stats that change
/// hits.
static HIT_FUSE: Fuse = Fuse::new("damage filters and stats of hits (Impact)");
static FALL_FUSE: Fuse = Fuse::new("damage filters of falls (FallDamage)");
static SHROUD_FUSE: Fuse = Fuse::new("damage filters of the shroud (Fog, DeadlyFog)");
static DROWNING_FUSE: Fuse = Fuse::new("damage filters of drowning (Suffocated, DrowningDeath)");
static FREEZING_FUSE: Fuse = Fuse::new("damage filters of freezing (Freezing)");
static KILL_FUSE: Fuse = Fuse::new("damage filters of admin kills (AdminKill)");

/// Registers mod `owner`'s damage filter.
///
/// # Errors
///
/// [`JoinFeatureError`] after init or on a build without the entity reads
/// every site makes; a build that lacks a group's sites turns off only that
/// group when it links.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    callback: DamageFilterFn,
    user: *mut c_void,
) -> Result<(), JoinFeatureError> {
    let available = || crate::engine::ecs::engine(loader).is_some();
    FILTERS.register_when(available, owner, callback, user)
}

/// The hit's layout, or the first of its constants this build lacks.
fn layout(loader: &Loader) -> Result<Layout, Constant> {
    let at = |id: Constant| loader.offset(id).ok_or(id);
    Ok(Layout {
        vector: at(pinned::damage::HIT_VECTOR)?,
        target: at(pinned::damage::HIT_TARGET)?,
        source: at(pinned::damage::HIT_SOURCE)?,
    })
}

/// Whether the running build's `DamageTypeNew`, read from its reflection,
/// has the [`TYPES`] values a hit's vector holds: a build with another
/// count lays the vector out otherwise.
fn types_match(loader: &Loader) -> bool {
    loader
        .reflection
        .type_info(catalogue::type_name(catalogue::ids::DAMAGE_TYPE))
        .is_ok_and(|info| info.enum_values().len() == TYPES)
}

/// Seals the filters before any part links, so [`filtering`] answers every
/// link the same, whatever the parts' order.
pub(crate) fn seal() {
    let _filters = FILTERS.seal();
}

/// Links the sites when a mod filters damage or uses stats, each group all
/// or none.
pub(crate) fn link(loader: &Loader) {
    let filters = FILTERS.sealed().unwrap_or_default();
    link_in(loader, filters, crate::inventory::stats::wanted());
}

/// Links the groups `filters` and the stats, when `stats` are wanted, need.
fn link_in(loader: &Loader, filters: &[Registered<DamageFilterFn>], stats: bool) {
    if filters.is_empty() && !stats {
        return;
    }
    let mut on = Vec::new();
    for group in groups(!filters.is_empty()) {
        if link_group(loader, group) {
            on.push(group.fuse.feature());
        }
    }
    if !on.is_empty() {
        crate::log!(INFO, "damage sites on: {}", on.join("; "));
    }
}

/// Links every site of `group`, after what it needs beyond them; true when
/// all linked. The first that fails turns the group's switch off, and its
/// sites linked before are wired past.
fn link_group(loader: &Loader, group: &Group) -> bool {
    if let Some(Err((id, reason))) = group.needs.map(|needs| needs(loader)) {
        return loader.fuse_off(group.fuse, id, reason);
    }
    group.sites.iter().all(|&(site, handler)| {
        loader.link_mid(group.fuse, Target::Pinned(site), handler, HookFlags::NONE)
    })
}

/// The groups to link: all of them for a mod's filters, else only the hit
/// group stats read, so a site stats never use cannot turn their hits off.
fn groups(filtering: bool) -> &'static [Group] {
    if filtering { &GROUPS } else { &[HITS] }
}

/// What the hit site reads beyond its code: the hit's frame layout, and a
/// build whose damage types fill the vector the loader reads.
fn hit_layout(loader: &Loader) -> Result<(), Lacking> {
    let layout = layout(loader).map_err(|lacking| (lacking.id(), crate::engine::builds::ABSENT))?;
    if !types_match(loader) {
        return Err((
            catalogue::type_name(catalogue::ids::DAMAGE_TYPE),
            OTHER_TYPES,
        ));
    }
    let _first = LAYOUT.set(layout);
    Ok(())
}

/// Whether a mod filters damage; one load, before a site reads anything.
#[inline]
pub(crate) fn filtering() -> bool {
    FILTERS.has_sealed()
}

/// Whether a hit is the loader's to read: a mod filters damage, or a stat
/// changes hits.
#[inline]
fn hits_watched() -> bool {
    filtering() || crate::inventory::stats::changes_hits()
}

/// Runs every live filter on `check`.
fn filter(check: &mut DamageCheck) {
    FILTERS.each(|filter, user| {
        // SAFETY: the filter's mod runs, so its DLL that holds the callback
        // stays loaded; it gets the user pointer its mod registered, and
        // `check` is borrowed for the call.
        unsafe { filter(&raw mut *check, user) };
    });
}

fn blank(cause: u32, target: u32) -> DamageCheck {
    DamageCheck {
        header: Header::of::<DamageCheck>(),
        cause,
        target: Entity(target),
        source: Entity(0),
        attacker: Entity(0),
        total: 0.0,
        amount_count: 0,
        amounts: core::ptr::null_mut(),
    }
}

/// A finite amount the filters left, never below zero.
fn kept(amount: f32) -> f32 {
    if amount.is_finite() {
        amount.max(0.0)
    } else {
        0.0
    }
}

/// The low 32 bits of a register, as the site's signed health value.
fn low(register: u64) -> i32 {
    i32::from_le_bytes(register.to_le_bytes()[..4].try_into().unwrap_or_default())
}

/// A register as an address; registers are 64 bits, as addresses are here.
fn address(register: u64) -> usize {
    usize::try_from(register).unwrap_or(0)
}

/// The entity of the row a system's context `r15` stands on.
///
/// # Safety
///
/// - `r15` is null or the live query context of the system running the
///   site, which runs on this thread.
unsafe fn row_entity(r15: u64) -> u32 {
    let Some(engine) = crate::engine::ecs::resolved() else {
        return 0;
    };
    // SAFETY: the caller guarantees `r15` is null or the live query context
    // of the system running on this thread, all `Engine::entity` asks for.
    unsafe { engine.entity(core::ptr::with_exposed_provenance_mut(address(r15))) }
}

/// The address of the health slot the site writes: `[r8 + rcx * 4]`.
fn health_slot(registers: &Registers) -> usize {
    address(registers.r8).wrapping_add(address(registers.rcx).wrapping_mul(4))
}

/// Damage `damage` after the filters, rounded as the game rounds health.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "health is an i32 the game works out from floats itself"
)]
fn filtered(cause: u32, target: u32, damage: i32) -> i32 {
    let mut check = blank(cause, target);
    check.total = damage as f32;
    filter(&mut check);
    kept(check.total).round().min(i32::MAX as f32) as i32
}

mid_handler! {
    fn hit;
    gate hits_watched();
    fuse HIT_FUSE, "the hit site";
    |registers| {
        let Some(layout) = LAYOUT.get() else {
            return;
        };
        let rbp = address(registers.rbp);
        let vector = rbp.wrapping_sub(layout.vector);
        // SAFETY: at the site the node's frame on this thread's stack holds
        // the hit's `u32` target at the table's `damage.hit_target` offset
        // above `rbp`; 1.0 build 1269579.
        let target: u32 = unsafe { crate::engine::memory::read(rbp, layout.target) };
        // SAFETY: at the site the node's frame on this thread's stack holds
        // the hit's `u32` source at the table's `damage.hit_source` offset
        // above `rbp`; 1.0 build 1269579.
        let source: u32 = unsafe { crate::engine::memory::read(rbp, layout.source) };
        // SAFETY: at the site the node's frame on this thread's stack holds
        // the hit's damage vector, `TYPES` floats, at the table's
        // `damage.hit_vector` offset below `rbp`; 1.0 build 1269579.
        let read: [f32; TYPES] = unsafe { crate::engine::memory::read(vector, 0) };
        let mut check = blank(DamageCheck::IMPACT, target);
        // At the site `r12d` holds the root of the hitting impact, the
        // attacker, worked out before it; 1.0 build 1269579.
        let root = low(registers.r12).cast_unsigned();
        let attacker = if root == 0 { source } else { root };
        check.source = Entity(source);
        check.attacker = Entity(attacker);
        let mut amounts = read;
        crate::inventory::stats::on_hit(&mut amounts, target, attacker);
        check.total = amounts
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != HEALING)
            .map(|(_, a)| *a)
            .sum();
        check.amount_count = TYPE_COUNT;
        check.amounts = amounts.as_mut_ptr();
        filter(&mut check);
        let after = amounts.map(kept);
        if after
            .iter()
            .zip(&read)
            .any(|(a, b)| a.to_bits() != b.to_bits())
        {
            // SAFETY: at the site the node's frame on this thread's stack
            // holds the hit's damage vector, `TYPES` floats, at the table's
            // `damage.hit_vector` offset below `rbp`, which only this thread
            // touches and the node reads after the site; 1.0 build 1269579.
            unsafe { crate::engine::memory::write(vector, 0, after) };
        }
    }
}

mid_handler! {
    fn fall;
    gate filtering();
    fuse FALL_FUSE, "the fall site";
    |registers| {
        // SAFETY: at the site `[r8 + rcx * 4]` is the entity's current `i32`
        // health, about to be overwritten with `r9d` by this thread, whose
        // system writes it; 1.0 build 1269579.
        let current: i32 = unsafe { crate::engine::memory::read(health_slot(registers), 0) };
        let new = low(registers.r9);
        // SAFETY: at the site `r15` holds the live query context of the
        // system running on this thread; 1.0 build 1269579.
        let target = unsafe { row_entity(registers.r15) };
        let kept = fall_kept(Entity(target), current, new, |damage| {
            filtered(DamageCheck::FALL_DAMAGE, target, damage)
        });
        if kept != new {
            registers.r9 = u64::from(kept.cast_unsigned());
        }
    }
}

thread_local! {
    /// The row and the health the fall this thread last wrote, when the
    /// filters changed its damage; every fall write sets it.
    static WRITTEN: core::cell::Cell<Option<(Entity, i32)>> = const { core::cell::Cell::new(None) };
}

/// [`kept_health`] for a fall of `target`, noting for [`fall_death`] the
/// health left whenever the filters changed the damage, even where the
/// game's truncated write is already 0; every fall write sets the note, so
/// none outlives the row it was made for.
fn fall_kept(target: Entity, current: i32, new: i32, filter: impl FnOnce(i32) -> i32) -> i32 {
    let mut changed = false;
    let kept = kept_health(current, new, |damage| {
        let filtered = filter(damage);
        changed = filtered != damage;
        filtered
    });
    WRITTEN.set(changed.then_some((target, kept)));
    kept
}

mid_handler! {
    fn fall_death;
    gate filtering();
    fuse FALL_FUSE, "the fall death site";
    |registers| {
        // `fall_damage_infliction` decides on a death from the health it
        // worked out in `xmm7`, not from the health it wrote: `comiss
        // xmm7,xmm8` against 0, then this `ja` past the death record, with
        // `r15` its context (server 26430253 and client 26430625, the same
        // code).
        let Some(written) = WRITTEN.take() else {
            return;
        };
        // SAFETY: at the site `r15` holds the live query context of the
        // system running on this thread; server 26430253.
        let row = unsafe { row_entity(registers.r15) };
        registers.rflags = decided(Some(written), Entity(row), registers.rflags);
    }
}

/// The flags for the fall's `ja` past its death record that make the game
/// decide from the health the filters left: below for 0, which records the
/// death, above for any other health, which records none. A note `written`
/// for another row than `row` leaves `rflags` as the game set them.
fn decided(written: Option<(Entity, i32)>, row: Entity, rflags: u64) -> u64 {
    match written {
        Some((entity, kept)) if entity == row && kept <= 0 => rflags | CARRY,
        Some((entity, _)) if entity == row => rflags & !(CARRY | ZERO),
        _ => rflags,
    }
}

/// The health a write from `current` to the game's `new` leaves once
/// `filter` turned its damage into what is taken; `new` when the write
/// takes nothing.
fn kept_health(current: i32, new: i32, filter: impl FnOnce(i32) -> i32) -> i32 {
    let damage = current.saturating_sub(new);
    if damage <= 0 {
        return new;
    }
    current.saturating_sub(filter(damage)).max(0)
}

mid_handler! {
    fn shroud;
    gate filtering();
    fuse SHROUD_FUSE, "the shroud site";
    |registers| {
        let damage = low(registers.rax);
        if damage <= 0 {
            return;
        }
        // SAFETY: at the site, `add r8,rbx` before the shroud's health
        // subtraction, `r15` holds the live query context of the system
        // running on this thread and `eax` the damage: seen on 1.0 build
        // 1269579 at the subtraction, and `add r8,rbx` writes neither; the
        // site's route checks this code on 26319703.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::FOG, target, damage);
        registers.rax = u64::from(damage.cast_unsigned());
    }
}

mid_handler! {
    fn shroud_death;
    gate filtering();
    fuse SHROUD_FUSE, "the shroud death site";
    |registers| {
        // SAFETY: at the site `[r8 + rcx * 4]` is the entity's current `i32`
        // health, about to be overwritten with `edx` by this thread, whose
        // system writes it; 1.0 build 1269579.
        let current: i32 = unsafe { crate::engine::memory::read(health_slot(registers), 0) };
        let new = low(registers.rdx);
        let r15 = registers.r15;
        let kept = kept_health(current, new, |damage| {
            // SAFETY: at the site `r15` holds the live query context of the
            // system running on this thread; 1.0 build 1269579.
            let target = unsafe { row_entity(r15) };
            filtered(DamageCheck::DEADLY_FOG, target, damage)
        });
        if kept != new {
            registers.rdx = u64::from(kept.cast_unsigned());
        }
    }
}

mid_handler! {
    fn drowning;
    gate filtering();
    fuse DROWNING_FUSE, "the drowning site";
    |registers| {
        // At the site `eax` is what `oxygen_update` takes from the health
        // at `[r8 + rcx * 4]`; r15 is its context; 1.0 build 1269579.
        let damage = low(registers.rax);
        if damage <= 0 {
            return;
        }
        // SAFETY: at the site `r15` holds the live query context of the
        // system running on this thread; 1.0 build 1269579.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::SUFFOCATED, target, damage);
        registers.rax = u64::from(damage.cast_unsigned());
    }
}

mid_handler! {
    fn drowning_death;
    gate filtering();
    fuse DROWNING_FUSE, "the drowning death site";
    |registers| {
        // SAFETY: at the site `[r8 + rcx * 4]` is a creature's current `i32`
        // health, about to be overwritten with `r14d` (0) by this thread,
        // whose system writes it and loads `r14d` again for each row; 1.0
        // build 1269579.
        let current: i32 = unsafe { crate::engine::memory::read(health_slot(registers), 0) };
        let new = low(registers.r14);
        let r15 = registers.r15;
        let kept = kept_health(current, new, |damage| {
            // SAFETY: at the site `r15` holds the live query context of the
            // system running on this thread; 1.0 build 1269579.
            let target = unsafe { row_entity(r15) };
            filtered(DamageCheck::DROWNING_DEATH, target, damage)
        });
        if kept != new {
            registers.r14 = (registers.r14 & !0xffff_ffff) | u64::from(kept.cast_unsigned());
        }
    }
}

mid_handler! {
    fn freezing;
    gate filtering();
    fuse FREEZING_FUSE, "the freezing site";
    |registers| {
        // At the site `eax` is what `bodyheat_depletion` takes from the
        // health; its context is r14 there; 1.0 build 1269579.
        let damage = low(registers.rax);
        if damage <= 0 {
            return;
        }
        // SAFETY: at the site `r14` holds the live query context of the
        // system running on this thread; 1.0 build 1269579.
        let target = unsafe { row_entity(registers.r14) };
        let damage = filtered(DamageCheck::FREEZING, target, damage);
        registers.rax = u64::from(damage.cast_unsigned());
    }
}

/// Whether the filters let an admin kill of `target` through.
fn kill_allowed(target: u32) -> bool {
    let mut check = blank(DamageCheck::ADMIN_KILL, target);
    check.total = 1.0;
    filter(&mut check);
    kept(check.total) > 0.0
}

mid_handler! {
    fn admin_kill;
    gate filtering();
    fuse KILL_FUSE, "the admin kill site";
    |registers| {
        let at = address(registers.rax);
        if at == 0 {
            return;
        }
        // SAFETY: `at` is non-zero (`at == 0` returns first); at the site `rax`
        // points at the target's health component pointer, which `admin_player`
        // tests next: null skips the kill; `ebx` is the target; 1.0 build
        // 1269579.
        let health: u64 = unsafe { crate::engine::memory::read(at, 0) };
        if health == 0 {
            return;
        }
        if !kill_allowed(low(registers.rbx).cast_unsigned()) {
            registers.rax = (&raw const NO_HEALTH).expose_provenance() as u64;
        }
    }
}

mid_handler! {
    fn faction_kill;
    gate filtering();
    fuse KILL_FUSE, "the faction kill site";
    |registers| {
        // At the site a set carry makes `debug_kill_faction` kill the row's
        // entity, a clear one skips it; r14 is its context; 1.0 build
        // 1269579.
        if registers.rflags & CARRY == 0 {
            return;
        }
        // SAFETY: at the site `r14` holds the live query context of the
        // system running on this thread; 1.0 build 1269579.
        let target = unsafe { row_entity(registers.r14) };
        if !kill_allowed(target) {
            registers.rflags &= !CARRY;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_left_negative_or_not_finite_count_as_none() {
        assert!((kept(-3.0)).abs() < f32::EPSILON);
        assert!((kept(f32::NAN)).abs() < f32::EPSILON);
        assert!((kept(2.5) - 2.5).abs() < f32::EPSILON);
    }

    /// No branch of a site's function lands inside the bytes its hook's
    /// jump replaces, so every site arms, on each executable
    /// `ENSH_SERVER_EXE`, `ENSH_CLIENT_EXE` and the opt-in `ENSH_EXES` path
    /// list name, a server when its file is `enshrouded_server.exe`;
    /// skipped without them.
    #[test]
    fn every_site_arms_on_the_game_executables() {
        use enshwick_gamedata::reflect::{Image, Layout};
        use enshwick_gamedata::{Side, exact};

        let tables = crate::engine::builds::load();
        let list = std::env::var_os("ENSH_EXES");
        let paths = ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(std::path::PathBuf::from)
            .chain(list.iter().flat_map(std::env::split_paths));
        for path in paths {
            let server = path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("enshrouded_server.exe"));
            let side = if server { Side::Server } else { Side::Client };
            let bytes = std::fs::read(&path).unwrap();
            let image = Image::parse(&bytes, Layout::File, None).unwrap();
            let Some(table) = exact(&tables, side, image.fingerprint().unwrap()) else {
                println!(
                    "{}: no table is made for this build; skipped",
                    path.display()
                );
                continue;
            };
            let resolved = table.resolve();
            let pdata = image.range(image.section(".pdata").unwrap());
            let rows = image.read(pdata.start, pdata.end - pdata.start).unwrap();
            let base = image.base();
            for &(symbol, _) in GROUPS.iter().flat_map(|group| group.sites) {
                let Some(rva) = resolved.symbol(symbol.id()) else {
                    continue;
                };
                let (begin, end) = rows
                    .as_chunks::<12>()
                    .0
                    .iter()
                    .map(|row| {
                        let at = |i: usize| {
                            u64::from(u32::from_le_bytes(row[i..i + 4].try_into().unwrap()))
                        };
                        (at(0), at(4))
                    })
                    .find(|&(begin, end)| (begin..end).contains(&rva))
                    .unwrap();
                let code = image.read(base + begin, end - begin).unwrap();
                let branch = crate::engine::mid::branch_into_hook(code, base + begin, base + rva);
                assert_eq!(
                    branch.map(|at| at - base),
                    None,
                    "{}: {symbol} at {rva:#x}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn a_hit_no_filter_or_stat_watches_is_left_unread() {
        let _first = LAYOUT.set(Layout {
            vector: 0x40,
            target: 0x10,
            source: 0x14,
        });
        // No page maps the frame this names: a read of it faults the test.
        let mut registers = Registers {
            rbp: 0x20,
            ..Registers::default()
        };
        // SAFETY: the registers are borrowed for the call, as a stub's are.
        unsafe { hit(&raw mut registers, core::ptr::null_mut()) };
        assert!(!HIT_FUSE.is_off());
    }

    unsafe extern "system" fn keep(_check: *mut DamageCheck, _user: *mut c_void) {}

    /// Filters holding one filter of mod 0, sealed as the seal phase does.
    fn one_filter() -> Filters<DamageFilterFn> {
        let filters = Filters::<DamageFilterFn>::new(Feature::Damage);
        let added = filters.register(ModIndex::new(0), keep, core::ptr::null_mut());
        assert!(added.is_ok());
        let _sealed = filters.seal();
        filters
    }

    /// A server's loader whose build pins every site but `lacking` and,
    /// with `layout`, the hit's frame layout.
    fn loader_without(lacking: Option<Symbol>, layout: bool) -> Loader {
        let sites: Vec<Symbol> = GROUPS
            .iter()
            .flat_map(|group| group.sites)
            .map(|&(site, _)| site)
            .filter(|&site| Some(site) != lacking)
            .collect();
        let constants = [
            pinned::damage::HIT_VECTOR,
            pinned::damage::HIT_TARGET,
            pinned::damage::HIT_SOURCE,
        ]
        .map(|constant| (constant, 0x10));
        crate::start::tests::pinning_loader(
            enshwick_core::abi::Side::SERVER,
            &sites,
            if layout { &constants } else { &[] },
        )
    }

    /// The groups' switches that are off, by name.
    fn off() -> Vec<&'static str> {
        GROUPS
            .iter()
            .filter(|group| group.fuse.is_off())
            .map(|group| group.fuse.feature())
            .collect()
    }

    #[test]
    fn a_build_whose_fall_death_site_does_not_route_turns_off_only_the_falls() {
        let filters = one_filter();
        let loader = loader_without(Some(pinned::damage::FALL_DEATH), true);
        link_in(&loader, filters.sealed().unwrap_or_default(), false);
        let fall = crate::base::off_reason(&FALL_FUSE);
        assert!(
            fall.as_deref()
                .is_some_and(|r| r.starts_with(pinned::damage::FALL_DEATH.id())),
            "{fall:?}"
        );
        assert!(
            FALL_FUSE.feature().contains("(FallDamage)"),
            "the off line names the cause"
        );
        // The bare reflection lacks the damage types, so the hits go for
        // their own reason.
        let hit = crate::base::off_reason(&HIT_FUSE);
        assert!(
            hit.as_deref()
                .is_some_and(|r| r.starts_with(catalogue::type_name(catalogue::ids::DAMAGE_TYPE))),
            "{hit:?}"
        );
        assert_eq!(off(), [HIT_FUSE.feature(), FALL_FUSE.feature()]);
        let shroud = loader.address(pinned::damage::SHROUD).unwrap_or_default();
        let linked = enshwick_base::Relock::relock(&loader.mids).features_at(shroud);
        assert!(
            SHROUD_FUSE.among(&linked),
            "the shroud site links after the falls went off"
        );
    }

    #[test]
    fn a_build_without_the_hit_layout_turns_off_only_the_hits() {
        let filters = one_filter();
        link_in(
            &loader_without(None, false),
            filters.sealed().unwrap_or_default(),
            false,
        );
        let hit = crate::base::off_reason(&HIT_FUSE);
        assert!(
            hit.as_deref()
                .is_some_and(|r| r.starts_with(pinned::damage::HIT_VECTOR.id())),
            "{hit:?}"
        );
        assert_eq!(off(), [HIT_FUSE.feature()]);
    }

    #[test]
    fn a_build_without_any_site_turns_every_group_off_by_its_own_switch() {
        let filters = one_filter();
        link_in(
            &crate::start::tests::bare_loader(),
            filters.sealed().unwrap_or_default(),
            false,
        );
        assert_eq!(off().len(), GROUPS.len());
    }

    #[test]
    fn nothing_filtering_damage_links_no_site_and_leaves_the_filters_on() {
        link_in(&crate::start::tests::bare_loader(), &[], false);
        assert!(off().is_empty());
    }

    #[test]
    fn stats_alone_link_only_the_hit_site_and_filters_link_all_ten() {
        let ids = |filtering| {
            groups(filtering)
                .iter()
                .flat_map(|group| group.sites)
                .map(|(s, _)| s.id())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(false), [pinned::damage::HIT.id()]);
        assert_eq!(ids(true).len(), 10);
    }

    #[test]
    fn every_damage_cause_is_named_by_exactly_one_group() {
        use enshwick_core::damage::DamageCause;
        let named = |group: &Group| {
            let name = group.fuse.feature();
            let causes = name
                .rsplit_once('(')
                .and_then(|(_, rest)| rest.strip_suffix(')'))
                .unwrap_or_default();
            causes.split(", ").map(str::to_owned).collect::<Vec<_>>()
        };
        for cause in DamageCause::NAMED {
            let cause = format!("{cause:?}");
            let groups = GROUPS
                .iter()
                .filter(|group| named(group).contains(&cause))
                .count();
            assert_eq!(groups, 1, "{cause}");
        }
        let all: usize = GROUPS.iter().map(|group| named(group).len()).sum();
        assert_eq!(all, DamageCause::NAMED.len(), "no name but a cause's");
    }

    /// `rflags` after `comiss` of a survivable fall (above) and a lethal
    /// one (zero).
    const ABOVE: u64 = 0x202;
    const LETHAL: u64 = 0x202 | ZERO;

    /// Whether `ja` jumps past the death record under `rflags`.
    fn skips_the_death(rflags: u64) -> bool {
        rflags & (CARRY | ZERO) == 0
    }

    const ROW: Entity = Entity(7);

    #[test]
    fn a_fall_a_filter_makes_lethal_records_the_death() {
        assert_eq!(fall_kept(ROW, 30, 10, |damage| damage * 3), 0);
        let written = WRITTEN.take();
        assert_eq!(written, Some((ROW, 0)));
        assert!(!skips_the_death(decided(written, ROW, ABOVE)));
    }

    #[test]
    fn a_fall_a_filter_makes_lethal_where_the_game_already_wrote_0_records_the_death() {
        // Health 100 and a fall worked out to 0.4: the game writes 0 but
        // its float says alive.
        assert_eq!(fall_kept(ROW, 100, 0, |damage| damage + 50), 0);
        let written = WRITTEN.take();
        assert_eq!(written, Some((ROW, 0)));
        assert!(!skips_the_death(decided(written, ROW, ABOVE)));
    }

    #[test]
    fn a_lethal_fall_a_filter_spares_records_no_death() {
        assert_eq!(fall_kept(ROW, 30, 0, |_| 0), 30);
        let written = WRITTEN.take();
        assert_eq!(written, Some((ROW, 30)));
        assert!(skips_the_death(decided(written, ROW, LETHAL)));
    }

    #[test]
    fn a_fall_the_filters_leave_alone_or_that_takes_nothing_clears_the_note() {
        let _lethal = fall_kept(ROW, 30, 10, |damage| damage * 3);
        assert_eq!(fall_kept(Entity(8), 30, 40, |_| unreachable!()), 40);
        assert_eq!(WRITTEN.take(), None, "took nothing");
        let _lethal = fall_kept(ROW, 30, 10, |damage| damage * 3);
        assert_eq!(fall_kept(Entity(8), 30, 10, |damage| damage), 10);
        assert_eq!(WRITTEN.take(), None, "left alone");
    }

    #[test]
    fn a_note_of_another_row_or_none_leaves_the_game_s_decision() {
        assert_eq!(decided(Some((Entity(8), 30)), ROW, LETHAL), LETHAL);
        assert_eq!(decided(Some((Entity(8), 0)), ROW, ABOVE), ABOVE);
        assert_eq!(decided(None, ROW, LETHAL), LETHAL);
    }

    #[test]
    fn a_filtered_write_keeps_what_the_filter_leaves_and_never_goes_below_zero() {
        assert_eq!(kept_health(30, 10, |damage| damage / 2), 20);
        assert_eq!(kept_health(30, 10, |damage| damage * 3), 0);
        assert_eq!(kept_health(30, 30, |_| unreachable!()), 30, "no damage");
    }

    #[test]
    fn without_filters_damage_stays() {
        assert_eq!(filtered(DamageCheck::FALL_DAMAGE, 1, 40), 40);
        assert!(kill_allowed(1));
    }
}
