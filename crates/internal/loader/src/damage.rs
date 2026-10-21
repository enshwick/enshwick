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
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{DamageCheck, DamageFilterFn, Header, HookFlags, Registers, Status};

use crate::chains::LOADER;
use crate::host::{Loader, Registered, take};
use crate::mid::MidLink;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "damage.admin_kill",
    "damage.drowning",
    "damage.drowning_death",
    "damage.faction_kill",
    "damage.fall",
    "damage.freezing",
    "damage.hit",
    "damage.hit_source",
    "damage.hit_target",
    "damage.hit_vector",
    "damage.shroud",
    "damage.shroud_death",
];

/// Damage types in a hit's vector.
const TYPES: usize = 13;
/// The healing entry of a hit's vector.
const HEALING: usize = 7;
/// The carry flag in `rflags`.
const CARRY: u64 = 1;

/// Null, for an admin kill's test of the target's health to read when the
/// filters refuse it.
static NO_HEALTH: u64 = 0;

static FILTERS: Mutex<Vec<Registered<DamageFilterFn>>> = Mutex::new(Vec::new());
static SEALED: OnceLock<Box<[Registered<DamageFilterFn>]>> = OnceLock::new();
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

/// The sites and what each hands the filters.
#[derive(Debug, Clone, Copy)]
enum Site {
    Hit,
    Fall,
    Shroud,
    ShroudDeath,
    Drowning,
    DrowningDeath,
    Freezing,
    AdminKill,
    FactionKill,
}

/// Registers mod `owner`'s damage filter.
pub(crate) fn register(
    loader: &Loader,
    owner: usize,
    callback: DamageFilterFn,
    user: *mut c_void,
) -> Status {
    if SEALED.get().is_some() {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() || crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    FILTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered::new(owner, callback, user));
    Status::OK
}

fn layout(loader: &Loader) -> Option<Layout> {
    Some(Layout {
        vector: loader.offset("damage.hit_vector")?,
        target: loader.offset("damage.hit_target")?,
        source: loader.offset("damage.hit_source")?,
    })
}

/// Seals the filters and adds the sites when any mod asked.
pub(crate) fn link(loader: &Loader) {
    let filters = take(&FILTERS);
    let wanted = !filters.is_empty() || crate::stats::wanted();
    let _first = SEALED.set(filters.into_boxed_slice());
    if !wanted {
        return;
    }
    let Some(layout) = layout(loader) else {
        return;
    };
    let _first = LAYOUT.set(layout);
    let sites: [(&str, Site, MidFn); 9] = [
        ("damage.hit", Site::Hit, hit),
        ("damage.fall", Site::Fall, fall),
        ("damage.shroud", Site::Shroud, shroud),
        ("damage.shroud_death", Site::ShroudDeath, shroud_death),
        ("damage.drowning", Site::Drowning, drowning),
        ("damage.drowning_death", Site::DrowningDeath, drowning_death),
        ("damage.freezing", Site::Freezing, freezing),
        ("damage.admin_kill", Site::AdminKill, admin_kill),
        ("damage.faction_kill", Site::FactionKill, faction_kill),
    ];
    let mut linked = 0;
    for (name, site, handler) in sites {
        let Some(address) = loader.address(name) else {
            crate::log!(WARN, "damage filters: {site:?} off, not in this build");
            continue;
        };
        let link = MidLink {
            owner: LOADER,
            handler,
            user: 0,
            priority: i32::MIN,
            flags: HookFlags::NONE,
        };
        let status = loader
            .mids
            .lock()
            .map_or(Status::REFUSED, |mut mids| mids.add(address, link));
        if status.is_ok() {
            linked += 1;
        } else {
            crate::log!(WARN, "damage filters: {site:?} off, refused ({})", status.0);
        }
    }
    crate::log!(INFO, "damage filters on: {linked} of {} sites", sites.len());
}

type MidFn = unsafe extern "system" fn(registers: *mut Registers, user: *mut c_void);

/// Runs every live filter on `check`.
fn filter(check: &mut DamageCheck) {
    let Some(filters) = SEALED.get() else {
        return;
    };
    for f in filters.iter().filter(|f| f.live()) {
        // SAFETY: the callback lives in the mod's DLL, which stays loaded,
        // and takes the pointer it registered; the check is valid for it.
        crate::profile::call(f.owner, crate::profile::Feature::Damage, || unsafe {
            (f.callback)(&raw mut *check, f.user());
        });
    }
}

fn blank(cause: u32, target: u32) -> DamageCheck {
    DamageCheck {
        header: Header::of::<DamageCheck>(),
        cause,
        target,
        source: 0,
        attacker: 0,
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
/// `r15` holds the context of the system running the site.
unsafe fn row_entity(r15: u64) -> u32 {
    let Some(engine) = crate::ecs::resolved() else {
        return 0;
    };
    // SAFETY: forwarded from the caller.
    unsafe { engine.entity(core::ptr::with_exposed_provenance_mut(address(r15))) }
}

/// The health slot the site writes: `[r8 + rcx * 4]`.
fn health_slot(registers: &Registers) -> *mut i32 {
    core::ptr::with_exposed_provenance_mut::<i32>(
        address(registers.r8).wrapping_add(address(registers.rcx).wrapping_mul(4)),
    )
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
static DAMAGE_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("damage filters");

unsafe extern "system" fn hit(registers: *mut Registers, _user: *mut c_void) {
    let _caught = DAMAGE_FUSE.run("the hit site", || {
        // SAFETY: the stub's saved registers, valid for the call.
        let Some(registers) = (unsafe { registers.as_ref() }) else {
            return;
        };
        let Some(layout) = LAYOUT.get() else {
            return;
        };
        let rbp = address(registers.rbp);
        let vector =
            core::ptr::with_exposed_provenance_mut::<[f32; TYPES]>(rbp.wrapping_sub(layout.vector));
        // SAFETY: at the site the node's frame holds its damage vector below
        // `rbp` and the target and source above it, on 1.0 build 1269579.
        let target: u32 = unsafe { crate::world::read(rbp, layout.target) };
        // SAFETY: as above.
        let source: u32 = unsafe { crate::world::read(rbp, layout.source) };
        // SAFETY: as above.
        let amounts = unsafe { vector.read_unaligned() };
        let mut check = blank(DamageCheck::HIT, target);
        // At the site `r12d` holds the root of the hitting impact, the
        // attacker, worked out before it; 1.0 build 1269579.
        let root = low(registers.r12).cast_unsigned();
        let attacker = if root == 0 { source } else { root };
        check.source = source;
        check.attacker = attacker;
        check.amounts = amounts;
        crate::stats::on_hit(&mut check.amounts, target, attacker);
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
            // SAFETY: as above; the node reads the vector after the site.
            unsafe { vector.write_unaligned(after) };
        }
    });
}

unsafe extern "system" fn fall(registers: *mut Registers, _user: *mut c_void) {
    let _caught = DAMAGE_FUSE.run("the fall site", || {
        // SAFETY: the stub's saved registers, valid for the call.
        let Some(registers) = (unsafe { registers.as_mut() }) else {
            return;
        };
        // SAFETY: at the site `[r8 + rcx * 4]` is the entity's current
        // health, about to be overwritten with `r9d`; 1.0 build 1269579.
        let current = unsafe { health_slot(registers).read_unaligned() };
        let new = low(registers.r9);
        let damage = current.saturating_sub(new);
        if damage <= 0 {
            return;
        }
        // SAFETY: r15 is the system's context at the site.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::FALL, target, damage);
        registers.r9 = u64::from(current.saturating_sub(damage).max(0).cast_unsigned());
    });
}

unsafe extern "system" fn shroud(registers: *mut Registers, _user: *mut c_void) {
    let _caught = DAMAGE_FUSE.run("the shroud site", || {
        // SAFETY: the stub's saved registers, valid for the call.
        let Some(registers) = (unsafe { registers.as_mut() }) else {
            return;
        };
        let damage = low(registers.rax);
        if damage <= 0 {
            return;
        }
        // SAFETY: r15 is the system's context at the site.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::SHROUD, target, damage);
        registers.rax = u64::from(damage.cast_unsigned());
    });
}

unsafe extern "system" fn shroud_death(registers: *mut Registers, _user: *mut c_void) {
    let _caught = DAMAGE_FUSE.run("the shroud death site", || {
        // SAFETY: the stub's saved registers, valid for the call.
        let Some(registers) = (unsafe { registers.as_mut() }) else {
            return;
        };
        // SAFETY: at the site `[r8 + rcx * 4]` is the entity's current
        // health, about to be overwritten with `edx`; 1.0 build 1269579.
        let current = unsafe { health_slot(registers).read_unaligned() };
        let new = low(registers.rdx);
        let damage = current.saturating_sub(new);
        if damage <= 0 {
            return;
        }
        // SAFETY: r15 is the system's context at the site.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::SHROUD_DEATH, target, damage);
        registers.rdx = u64::from(current.saturating_sub(damage).max(0).cast_unsigned());
    });
}

unsafe extern "system" fn drowning(registers: *mut Registers, _user: *mut c_void) {
    let _caught = DAMAGE_FUSE.run("the drowning site", || {
        // SAFETY: the stub's saved registers, valid for the call.
        let Some(registers) = (unsafe { registers.as_mut() }) else {
            return;
        };
        // At the site `eax` is what `oxygen_update` takes from the health
        // at `[r8 + rcx * 4]`; r15 is its context; 1.0 build 1269579.
        let damage = low(registers.rax);
        if damage <= 0 {
            return;
        }
        // SAFETY: r15 is the system's context at the site.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::DROWNING, target, damage);
        registers.rax = u64::from(damage.cast_unsigned());
    });
}

unsafe extern "system" fn drowning_death(registers: *mut Registers, _user: *mut c_void) {
    let _caught = DAMAGE_FUSE.run("the drowning death site", || {
        // SAFETY: the stub's saved registers, valid for the call.
        let Some(registers) = (unsafe { registers.as_mut() }) else {
            return;
        };
        // SAFETY: at the site `[r8 + rcx * 4]` is a creature's current
        // health, about to be overwritten with `r14d` (0), which the system
        // loads again for each row; 1.0 build 1269579.
        let current = unsafe { health_slot(registers).read_unaligned() };
        let new = low(registers.r14);
        let damage = current.saturating_sub(new);
        if damage <= 0 {
            return;
        }
        // SAFETY: r15 is the system's context at the site.
        let target = unsafe { row_entity(registers.r15) };
        let damage = filtered(DamageCheck::DROWNING_DEATH, target, damage);
        let kept = current.saturating_sub(damage).max(0).cast_unsigned();
        registers.r14 = (registers.r14 & !0xffff_ffff) | u64::from(kept);
    });
}

unsafe extern "system" fn freezing(registers: *mut Registers, _user: *mut c_void) {
    let _caught = DAMAGE_FUSE.run("the freezing site", || {
        // SAFETY: the stub's saved registers, valid for the call.
        let Some(registers) = (unsafe { registers.as_mut() }) else {
            return;
        };
        // At the site `eax` is what `bodyheat_depletion` takes from the
        // health; its context is r14 there; 1.0 build 1269579.
        let damage = low(registers.rax);
        if damage <= 0 {
            return;
        }
        // SAFETY: r14 is the system's context at the site.
        let target = unsafe { row_entity(registers.r14) };
        let damage = filtered(DamageCheck::FREEZING, target, damage);
        registers.rax = u64::from(damage.cast_unsigned());
    });
}

/// Whether the filters let an admin kill of `target` through.
fn kill_allowed(target: u32) -> bool {
    let mut check = blank(DamageCheck::ADMIN_KILL, target);
    check.total = 1.0;
    filter(&mut check);
    kept(check.total) > 0.0
}

unsafe extern "system" fn admin_kill(registers: *mut Registers, _user: *mut c_void) {
    let _caught = DAMAGE_FUSE.run("the admin kill site", || {
        // SAFETY: the stub's saved registers, valid for the call.
        let Some(registers) = (unsafe { registers.as_mut() }) else {
            return;
        };
        let at = address(registers.rax);
        if at == 0 {
            return;
        }
        // SAFETY: at the site `rax` points at the target's health component
        // pointer, which `admin_player` tests next: null skips the kill;
        // `ebx` is the target; 1.0 build 1269579.
        let health: u64 = unsafe { crate::world::read(at, 0) };
        if health == 0 {
            return;
        }
        if !kill_allowed(low(registers.rbx).cast_unsigned()) {
            registers.rax = (&raw const NO_HEALTH).addr() as u64;
        }
    });
}

unsafe extern "system" fn faction_kill(registers: *mut Registers, _user: *mut c_void) {
    let _caught = DAMAGE_FUSE.run("the faction kill site", || {
        // SAFETY: the stub's saved registers, valid for the call.
        let Some(registers) = (unsafe { registers.as_mut() }) else {
            return;
        };
        // At the site a set carry makes `debug_kill_faction` kill the row's
        // entity, a clear one skips it; r14 is its context; 1.0 build
        // 1269579.
        if registers.rflags & CARRY == 0 {
            return;
        }
        // SAFETY: r14 is the system's context at the site.
        let target = unsafe { row_entity(registers.r14) };
        if !kill_allowed(target) {
            registers.rflags &= !CARRY;
        }
    });
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

    #[test]
    fn without_filters_damage_stays() {
        assert_eq!(filtered(DamageCheck::FALL, 1, 40), 40);
        assert!(kill_allowed(1));
    }
}
