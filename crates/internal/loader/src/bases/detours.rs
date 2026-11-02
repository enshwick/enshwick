//! The detours of the raised limit: the altar limit, the grouping and its
//! new base code, the four zone lookups and the placement cursor.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use enshwick::abi::AltarLimit;
use enshwick::raw::read_at;

use super::members::{Game, Side, side_of};
use super::{
    ADD_BASE_NEXT, ASKED, BASE_AT_NEXT, BASES_FUSE, CHECK_NEXT, Fields, GROUP_NEXT, HOLDER_AT_NEXT,
    IN_BASE_NEXT, LIMITS, LimitFilter, MAX_ALTARS_NEXT, PLANNED, SITES, Sites, ZONE_COUNT, ZONES,
    ZONES_AT_NEXT, base_limit, cursor, kind_index, lifecycle, members, past16, raised,
};
use crate::base_groups::grouping::Grouped;
use crate::base_groups::limit::{Zone, Zones};
use crate::base_groups::window::{self, Kind};

pub(super) unsafe extern "system" fn max_altars(balancing: u64, flame_level: u32) -> u8 {
    // SAFETY: this detour runs only as the `building.max_altars` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let vanilla = unsafe { MAX_ALTARS_NEXT.get()(balancing, flame_level) };
    let Some(limits) = LIMITS.sealed() else {
        return vanilla;
    };
    let running = limits.iter().filter(|f| !crate::host::is_off(f.owner));
    chain_limit(flame_level, vanilla, base_limit().zones, running)
}

/// Runs `filters` in order on the limit of `flame_level`, where the game
/// allows `vanilla`, each on the count the last one left; the answer is
/// kept within `zones`.
pub(super) fn chain_limit<'a>(
    flame_level: u32,
    vanilla: u8,
    zones: u32,
    filters: impl Iterator<Item = &'a LimitFilter>,
) -> u8 {
    let vanilla_limit = AltarLimit {
        header: enshwick::abi::Header::of::<AltarLimit>(),
        flame_level,
        vanilla,
        allowed: vanilla,
        reserved: [0; 2],
    };
    let mut limit = vanilla_limit;
    for filter in filters {
        // SAFETY: the owner is not off, so the mod's DLL that holds the
        // filter stays loaded; the filter gets the user pointer its mod
        // registered and a limit valid for the call, and the safe layer
        // catches its panics.
        crate::profile::call(filter.owner, crate::profile::Feature::Altars, || unsafe {
            (filter.callback)(&raw mut limit, filter.user());
        });
        limit = AltarLimit {
            allowed: limit.allowed,
            ..vanilla_limit
        };
    }
    limit.allowed.min(u8::try_from(zones).unwrap_or(u8::MAX))
}

pub(super) unsafe extern "system" fn group(bases: *mut u8, world: *mut u8) {
    // SAFETY: `bases` is the grouping's own bases object when not null.
    let kind = SITES
        .get()
        .filter(|_| !bases.is_null())
        .map(|sites| unsafe { side_of(bases, &sites.fields) });
    if let Some(kind) = kind {
        PLANNED[kind_index(kind)].store(window::VANILLA, Ordering::Relaxed);
    }
    if let Some(sites) = SITES.get().filter(|_| raised() && !bases.is_null()) {
        // SAFETY: `bases` is the grouping's own live object, non-null by the
        // filter, before the game's grouping copies the world's zone list.
        let _done = BASES_FUSE.run("the zone list", || unsafe {
            members::let_zone_list_grow(bases, sites.fields);
        });
    }
    // SAFETY: this detour runs only as the base grouping's link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { GROUP_NEXT.get()(bases, world) };
    let (Some(sites), Some(kind)) = (SITES.get().filter(|_| raised()), kind) else {
        return;
    };
    // SAFETY: `bases` is non-null, since `kind` is known, and is the bases
    // object the grouping has just filled, on the grouping's thread; the
    // game's new base code is what `ADD_BASE_NEXT` holds past our link.
    let settled = BASES_FUSE.run("the base grouping", || unsafe {
        regroup(bases, kind, sites)
    });
    if let Some(grouped) = settled.flatten() {
        // SAFETY: `bases` is non-null and is the object the game's grouping
        // and `settle` have just filled, on this thread.
        unsafe { keep(bases, kind, sites.fields, &grouped) };
    } else {
        Side::of(kind).forget();
        BASES_FUSE.blow("the base grouping's count");
    }
}

/// Releases the window records of `bases`, as the game's grouping has just
/// released its own, and settles our grouping on it.
///
/// # Safety
///
/// - `bases` is the live `kind` object the game's grouping has just filled,
///   on the grouping's thread.
unsafe fn regroup(bases: *mut u8, kind: Kind, sites: &Sites) -> Option<Grouped> {
    let window = past16::sites().map(past16::Sites::code);
    let ready = window.and_then(|code| {
        // SAFETY: the caller guarantees the live object on the grouping's
        // thread, which no reader runs beside.
        unsafe { lifecycle::release(bases, &code) };
        // SAFETY: the same object, live while `settle` uses the answer.
        unsafe { lifecycle::ready(bases, sites.fields.allocator, code) }
    });
    // SAFETY: the caller's guarantees; `ready` is there only for an object
    // whose window `release` has just released.
    unsafe {
        members::settle(
            bases,
            sites.fields,
            ASKED.bases(kind),
            ADD_BASE_NEXT.get(),
            ready.as_ref().map(|f| f as &dyn Fn(usize)),
        )
    }
}

/// Keeps what the grouping found, for the cursor and the API.
///
/// # Safety
///
/// - `bases` is the non-null bases object the grouping has just filled,
///   live for the call, on the grouping's thread.
unsafe fn keep(bases: *const u8, kind: Kind, fields: Fields, grouped: &Grouped) {
    let Some(known) = ZONES.get().map(|zones| &zones[kind_index(kind)]) else {
        return;
    };
    // SAFETY: `keep`'s caller guarantees a live object just grouped, on the
    // grouping's thread, which no reader runs beside.
    let game = unsafe { Game::new(bases, fields) };
    let listed = crate::base_groups::lookup::Bases::listed(&game);
    let zone = |i: usize| Zone {
        bounds: crate::base_groups::lookup::Bases::zone(&game, i),
        based: grouped.iter().any(|base| base.zones.contains(i)),
    };
    let count = u32::try_from(grouped.len()).unwrap_or(u32::MAX);
    ZONE_COUNT[kind_index(kind)].store(
        u32::try_from(listed).unwrap_or(u32::MAX - 1),
        Ordering::Relaxed,
    );
    if grouped.len() > window::VANILLA {
        let entered = members::take_entries_while_lent();
        crate::log!(
            DEBUG,
            "{count} bases grouped; whole-array calls while the count was lent: {entered}"
        );
    }
    let current = known.load();
    if current.bases == count
        && current.zones.len() == listed
        && (0..listed).all(|i| current.zones[i] == zone(i))
    {
        return;
    }
    let zones = Zones {
        zones: (0..listed).map(zone).collect(),
        bases: count,
    };
    let apart = zones.apart();
    let shape = |z: &Zones| (z.zones.len(), z.bases, z.apart());
    if shape(&current) != shape(&zones) {
        if apart > 0 {
            crate::log!(
                WARN,
                "{listed} altar zones in {count} bases: {apart} stand apart from every base, since the game holds {} bases; they allow building but no base features until they touch a base's zone",
                ASKED.bases(kind)
            );
        } else {
            crate::log!(INFO, "{listed} altar zones in {count} bases");
        }
    }
    known.store(Arc::new(zones));
}

pub(super) unsafe extern "system" fn add_base(bases: *mut u8, zone: *const u8, index: u32) {
    if let Some(fields) = SITES
        .get()
        .filter(|_| raised() && !bases.is_null())
        .map(|sites| sites.fields)
    {
        // SAFETY: `bases` is non-null by the filter before it and is the
        // bases object the grouping passes; its u32 base count sits at the
        // build table's `bases.count`.
        let started = unsafe { read_at::<u32>(bases, fields.count) };
        // SAFETY: `bases` is non-null by the filter and is the live object
        // the grouping passes to its new base code.
        let kind = unsafe { side_of(bases, &fields) };
        let planned = &PLANNED[kind_index(kind)];
        if started == 0 {
            // SAFETY: the game's grouping calls the new base code only after
            // it refilled the object's zone list, on its own thread.
            let plan = BASES_FUSE.run("the base plan", || unsafe {
                members::planned(bases, fields, ASKED.bases(kind))
            });
            planned.store(plan.unwrap_or(window::VANILLA), Ordering::Relaxed);
        }
        let limit = planned.load(Ordering::Relaxed).min(window::VANILLA);
        if usize::try_from(started).map_or(true, |n| n >= limit) {
            return;
        }
    }
    // SAFETY: this detour runs only as the new base code's link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { ADD_BASE_NEXT.get()(bases, zone, index) };
}

/// The view of `bases` for a lookup, while the raised limit holds.
///
/// # Safety
///
/// - `bases` is a live bases object, read on a thread the grouping does not
///   run beside.
pub(super) unsafe fn view(bases: *const u8) -> Option<Game> {
    let sites = SITES.get().filter(|_| raised() && !BASES_FUSE.is_off())?;
    // SAFETY: `bases` is non-null by the check, and `view`'s caller
    // guarantees a live object read beside no grouping.
    (!bases.is_null()).then(|| unsafe { Game::new(bases, sites.fields) })
}

/// The point a lookup is asked about; `None` for a null one.
///
/// # Safety
///
/// - `point` is null or readable for three `f32`.
unsafe fn point_of(point: *const [f32; 3]) -> Option<[f32; 3]> {
    // SAFETY: the caller guarantees a readable point when it is not null.
    (!point.is_null()).then(|| unsafe { read_at(point.cast(), 0) })
}

pub(super) unsafe extern "system" fn base_at(
    out: *mut u8,
    bases: *const u8,
    point: *const [f32; 3],
) -> *mut u8 {
    // SAFETY: the lookup's own point, readable when not null.
    let p = unsafe { point_of(point) };
    // SAFETY: the lookup's own object, live, on a thread beside no grouping.
    let game = unsafe { view(bases) };
    if let (Some(p), Some(game), false) = (p, game, out.is_null()) {
        // SAFETY: the lookup's own answer, writable for 0x10 bytes.
        let done = BASES_FUSE.run("a base lookup", || unsafe {
            members::base_at(out, &game, p);
        });
        if done.is_some() {
            return out;
        }
    }
    // SAFETY: this detour runs only as the lookup's link, so the slot holds
    // the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { BASE_AT_NEXT.get()(out, bases, point) }
}

pub(super) unsafe extern "system" fn in_base(
    bases: *const u8,
    i: u64,
    point: *const [f32; 3],
) -> bool {
    // SAFETY: the lookup's own point, readable when not null.
    let p = unsafe { point_of(point) };
    // SAFETY: the lookup's own object, live, on a thread beside no grouping.
    let game = unsafe { view(bases) };
    if let (Some(p), Some(game), Ok(i)) = (p, game, usize::try_from(i))
        && game.holds_index(i)
        && let Some(inside) = BASES_FUSE.run("a base lookup", || members::in_base(&game, i, p))
    {
        return inside;
    }
    // SAFETY: this detour runs only as the lookup's link, so the slot holds
    // the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { IN_BASE_NEXT.get()(bases, i, point) }
}

pub(super) unsafe extern "system" fn holder_at(
    out: *mut u8,
    bases: *const u8,
    point: *const [f32; 3],
) -> *mut u8 {
    // SAFETY: the lookup's own point, readable when not null.
    let p = unsafe { point_of(point) };
    // SAFETY: the lookup's own object, live, on a thread beside no grouping.
    let game = unsafe { view(bases) };
    if let (Some(p), Some(game), false) = (p, game, out.is_null()) {
        // SAFETY: the lookup's own answer, writable for 0x30 bytes.
        let done = BASES_FUSE.run("a zone lookup", || unsafe {
            members::holder_at(out, &game, p);
        });
        if done.is_some() {
            return out;
        }
    }
    // SAFETY: this detour runs only as the lookup's link, so the slot holds
    // the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { HOLDER_AT_NEXT.get()(out, bases, point) }
}

pub(super) unsafe extern "system" fn zones_at(
    out: *mut u8,
    list: *mut u8,
    bases: *const u8,
    point: *const [f32; 3],
) -> *mut u8 {
    // SAFETY: the lookup's own point, readable when not null.
    let p = unsafe { point_of(point) };
    // SAFETY: the lookup's own object, live, on a thread beside no grouping.
    let game = unsafe { view(bases) };
    if let (Some(p), Some(game), false) = (p, game, out.is_null() || list.is_null()) {
        // SAFETY: the game's live zone box container.
        let len = unsafe { members::list_len(list, &game.fields().container) };
        // SAFETY: the lookup's own answer and the game's live zone box
        // container with its own grow callback.
        let done = BASES_FUSE.run("a zone list lookup", || unsafe {
            members::zones_at(out, list, &game, p);
        });
        if done.is_some() {
            return out;
        }
        // SAFETY: the same container, whose boxes past `len` the game's own
        // lookup appends again.
        unsafe { members::set_list_len(list, &game.fields().container, len) };
    }
    // SAFETY: this detour runs only as the lookup's link, so the slot holds
    // the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { ZONES_AT_NEXT.get()(out, list, bases, point) }
}

pub(super) unsafe extern "system" fn check(
    flags: *mut u32,
    ctx: *const u8,
    transform: *const i64,
    item: *const u8,
    mode: u64,
    extra: u64,
) {
    // SAFETY: this detour runs only as the placement cursor's link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    unsafe { CHECK_NEXT.get()(flags, ctx, transform, item, mode, extra) };
    if !raised() || flags.is_null() || ctx.is_null() || transform.is_null() || item.is_null() {
        return;
    }
    // SAFETY: the cursor's own arguments, each non-null by the check before it,
    // on the cursor's thread after the original ran.
    let _done = BASES_FUSE.run("the placement check", || unsafe {
        cursor::refuse_past_limit(flags, ctx, transform, item);
    });
}
