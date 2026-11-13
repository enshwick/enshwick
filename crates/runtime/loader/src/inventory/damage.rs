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
//! attribute. An administrator's kill zeroes every health attribute at
//! once, so the loader hooks the test before it: of the target's health in
//! `admin_player` for chosen enemies, of the entity's faction in
//! `debug_kill_faction` for whole factions and players, and a refused kill
//! takes the path of an entity the kill does not reach. Every site runs on
//! the game's worker threads, in parallel, so the filters run there too: no
//! allocation, no lock.

use core::ffi::c_void;
use std::sync::OnceLock;

use enshwick_core::abi::{
    DamageCheck, DamageFilterFn, Header, HookFlags, MidHookFn, Registers, Status,
};

use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Registered, Target};
use crate::engine::mid::mid_handler;
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned::{self, Constant, Symbol};
use crate::engine::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::damage::ADMIN_KILL.id(),
    pinned::damage::DROWNING.id(),
    pinned::damage::DROWNING_DEATH.id(),
    pinned::damage::FACTION_KILL.id(),
    pinned::damage::FALL.id(),
    pinned::damage::FREEZING.id(),
    pinned::damage::HIT.id(),
    pinned::damage::HIT_SOURCE.id(),
    pinned::damage::HIT_TARGET.id(),
    pinned::damage::HIT_VECTOR.id(),
    pinned::damage::SHROUD.id(),
    pinned::damage::SHROUD_DEATH.id(),
];

/// Damage types in a hit's vector.
const TYPES: usize = enshwick_core::abi::DAMAGE_TYPES;
/// The healing entry of a hit's vector.
const HEALING: usize = enshwick_core::damage::DamageTypeNew::Healing.index();
/// The carry flag in `rflags`.
const CARRY: u64 = 1;

/// Null, for an admin kill's test of the target's health to read when the
/// filters refuse it.
static NO_HEALTH: u64 = 0;

static FILTERS: Registry<Registered<DamageFilterFn>> = Registry::new();
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

/// Each site's mid-hook target and handler.
const SITES: [(Symbol, MidHookFn); 9] = [
    (pinned::damage::HIT, hit),
    (pinned::damage::FALL, fall),
    (pinned::damage::SHROUD, shroud),
    (pinned::damage::SHROUD_DEATH, shroud_death),
    (pinned::damage::DROWNING, drowning),
    (pinned::damage::DROWNING_DEATH, drowning_death),
    (pinned::damage::FREEZING, freezing),
    (pinned::damage::ADMIN_KILL, admin_kill),
    (pinned::damage::FACTION_KILL, faction_kill),
];

/// Registers mod `owner`'s damage filter.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    callback: DamageFilterFn,
    user: *mut c_void,
) -> Status {
    if FILTERS.is_sealed() {
        return Status::TOO_LATE;
    }
    if layout(loader).is_err() || crate::engine::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    FILTERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
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

/// Seals the filters and links every site when a mod filters damage or
/// uses stats: the filters arm at all the sites or none.
pub(crate) fn link(loader: &Loader) {
    let filters = FILTERS.seal();
    if filters.is_empty() && !crate::inventory::stats::wanted() {
        return;
    }
    let layout = match layout(loader) {
        Ok(layout) => layout,
        Err(lacking) => {
            let _off = loader.fuse_off(&DAMAGE_FUSE, lacking.id(), crate::engine::builds::ABSENT);
            return;
        }
    };
    let _first = LAYOUT.set(layout);
    let linked = SITES.iter().all(|&(site, handler)| {
        loader.link_mid(&DAMAGE_FUSE, Target::Pinned(site), handler, HookFlags::NONE)
    });
    if linked {
        crate::log!(INFO, "damage filters on: {} sites", SITES.len());
    }
}

/// Whether a mod filters damage; one load, before a site reads anything.
fn filtering() -> bool {
    FILTERS.sealed().is_some_and(|filters| !filters.is_empty())
}

/// Whether a hit is the loader's to read: a mod filters damage, or a stat
/// changes hits.
fn hits_watched() -> bool {
    filtering() || crate::inventory::stats::changes_hits()
}

/// Runs every live filter on `check`.
fn filter(check: &mut DamageCheck) {
    let Some(filters) = FILTERS.sealed() else {
        return;
    };
    for f in filters.iter().filter(|f| f.live()) {
        // SAFETY: `callback` is a `DamageFilterFn` the mod registered, in its
        // DLL, which stays loaded (`Registered`); `user` is the pointer it
        // registered with it, and `check` is borrowed for the call.
        crate::engine::profile::call(
            f.owner,
            crate::engine::profile::Feature::Damage,
            || unsafe {
                (f.callback)(&raw mut *check, f.user());
            },
        );
    }
}

fn blank(cause: u32, target: u32) -> DamageCheck {
    DamageCheck {
        header: Header::of::<DamageCheck>(),
        cause,
        target: enshwick_core::Entity(target),
        source: enshwick_core::Entity(0),
        attacker: enshwick_core::Entity(0),
        amounts: [0.0; TYPES],
        total: 0.0,
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

/// The damage filters' switch.
static DAMAGE_FUSE: Fuse = Fuse::new("damage filters");

mid_handler! {
    fn hit;
    gate hits_watched();
    fuse DAMAGE_FUSE, "the hit site";
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
        let amounts: [f32; TYPES] = unsafe { crate::engine::memory::read(vector, 0) };
        let mut check = blank(DamageCheck::IMPACT, target);
        // At the site `r12d` holds the root of the hitting impact, the
        // attacker, worked out before it; 1.0 build 1269579.
        let root = low(registers.r12).cast_unsigned();
        let attacker = if root == 0 { source } else { root };
        check.source = enshwick_core::Entity(source);
        check.attacker = enshwick_core::Entity(attacker);
        check.amounts = amounts;
        crate::inventory::stats::on_hit(&mut check.amounts, target, attacker);
        check.total = check
            .amounts
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != HEALING)
            .map(|(_, a)| *a)
            .sum();
        filter(&mut check);
        let after = check.amounts.map(kept);
        if after
            .iter()
            .zip(&amounts)
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
    fuse DAMAGE_FUSE, "the fall site";
    |registers| {
        // SAFETY: at the site `[r8 + rcx * 4]` is the entity's current `i32`
        // health, about to be overwritten with `r9d` by this thread, whose
        // system writes it; 1.0 build 1269579.
        let current: i32 = unsafe { crate::engine::memory::read(health_slot(registers), 0) };
        let new = low(registers.r9);
        let damage = current.saturating_sub(new);
        if damage <= 0 {
            return;
        }
        // SAFETY: at the site `r15` holds the live query context of the
        // system running on this thread; 1.0 build 1269579.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::FALL_DAMAGE, target, damage);
        registers.r9 = u64::from(current.saturating_sub(damage).max(0).cast_unsigned());
    }
}

mid_handler! {
    fn shroud;
    gate filtering();
    fuse DAMAGE_FUSE, "the shroud site";
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
    fuse DAMAGE_FUSE, "the shroud death site";
    |registers| {
        // SAFETY: at the site `[r8 + rcx * 4]` is the entity's current `i32`
        // health, about to be overwritten with `edx` by this thread, whose
        // system writes it; 1.0 build 1269579.
        let current: i32 = unsafe { crate::engine::memory::read(health_slot(registers), 0) };
        let new = low(registers.rdx);
        let damage = current.saturating_sub(new);
        if damage <= 0 {
            return;
        }
        // SAFETY: at the site `r15` holds the live query context of the
        // system running on this thread; 1.0 build 1269579.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::DEADLY_FOG, target, damage);
        registers.rdx = u64::from(current.saturating_sub(damage).max(0).cast_unsigned());
    }
}

mid_handler! {
    fn drowning;
    gate filtering();
    fuse DAMAGE_FUSE, "the drowning site";
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
    fuse DAMAGE_FUSE, "the drowning death site";
    |registers| {
        // SAFETY: at the site `[r8 + rcx * 4]` is a creature's current `i32`
        // health, about to be overwritten with `r14d` (0) by this thread,
        // whose system writes it and loads `r14d` again for each row; 1.0
        // build 1269579.
        let current: i32 = unsafe { crate::engine::memory::read(health_slot(registers), 0) };
        let new = low(registers.r14);
        let damage = current.saturating_sub(new);
        if damage <= 0 {
            return;
        }
        // SAFETY: at the site `r15` holds the live query context of the
        // system running on this thread; 1.0 build 1269579.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::DROWNING_DEATH, target, damage);
        let kept = current.saturating_sub(damage).max(0).cast_unsigned();
        registers.r14 = (registers.r14 & !0xffff_ffff) | u64::from(kept);
    }
}

mid_handler! {
    fn freezing;
    gate filtering();
    fuse DAMAGE_FUSE, "the freezing site";
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
    fuse DAMAGE_FUSE, "the admin kill site";
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
    fuse DAMAGE_FUSE, "the faction kill site";
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
            for (symbol, _) in SITES {
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
        assert!(!DAMAGE_FUSE.is_off());
    }

    unsafe extern "system" fn keep(_check: *mut DamageCheck, _user: *mut c_void) {}

    /// Registers one filter of mod 0 and seals them, as the loader does at
    /// linking.
    fn one_filter() {
        let added = FILTERS.add(Registered::new(
            ModIndex::new(0),
            keep,
            core::ptr::null_mut(),
        ));
        assert!(added.is_ok());
    }

    #[test]
    fn a_hit_a_mod_filters_is_the_loader_s_to_read() {
        one_filter();
        let _filters = FILTERS.seal();
        assert!(filtering());
        assert!(hits_watched(), "a filter alone makes a hit watched");
    }

    #[test]
    fn a_build_without_the_hit_layout_turns_the_filters_off_when_a_mod_filters() {
        one_filter();
        link(&crate::start::tests::bare_loader());
        assert!(DAMAGE_FUSE.is_off(), "off by its fuse, not only in the log");
    }

    #[test]
    fn nothing_filtering_damage_links_no_site_and_leaves_the_filters_on() {
        link(&crate::start::tests::bare_loader());
        assert!(!DAMAGE_FUSE.is_off());
    }

    #[test]
    fn without_filters_damage_stays() {
        assert_eq!(filtered(DamageCheck::FALL_DAMAGE, 1, 40), 40);
        assert!(kill_allowed(1));
    }
}
