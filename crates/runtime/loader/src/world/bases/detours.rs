//! The detours of the raised limit: the altar limit, the grouping and its
//! new base code, the four zone lookups and the placement cursor.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use enshwick_core::abi::{AltarLimit, HookFlags};
use enshwick_core::raw::read_at;

use super::members::{Game, Side, side_of};
use super::{
    ALTAR_LIMIT, ASKED, Container, Fields, LIMITS, LimitFilter, PLANNED, RAISED_FUSE, RAISED_ZONES,
    SITES, Sites, ZONE_COUNT, ZONES, base_limit, cursor, kind_index, lifecycle, members, past16,
    raised,
};
use crate::engine::fuse::Off;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;
use crate::world::base_groups::grouping::Grouped;
use crate::world::base_groups::limit::{Zone, Zones};
use crate::world::base_groups::window::{self, Kind};

/// Links the altar limit under [`ALTAR_LIMIT`]; true when linked.
pub(super) fn link_limit(loader: &Loader) -> bool {
    let target = Target::Pinned(pinned::building::MAX_ALTARS);
    loader.link_fused(
        &ALTAR_LIMIT,
        target,
        &MAX_ALTARS_NEXT,
        max_altars,
        HookFlags::NONE,
    )
}

/// Links the grouping, its new base code, the four lookups and the cursor,
/// all or none, under [`RAISED_FUSE`]: a site left to the game would keep
/// its limits beside the raised ones. True when every link is made.
pub(super) fn link_raised(loader: &Loader) -> bool {
    let at = Target::Pinned;
    let none = HookFlags::NONE;
    let only = HookFlags::EXCLUSIVE;
    let fuse = &RAISED_FUSE;
    loader.link_fused(fuse, at(pinned::bases::GROUPING), &GROUP_NEXT, group, none)
        && loader.link_fused(
            fuse,
            at(pinned::bases::NEW_BASE),
            &ADD_BASE_NEXT,
            add_base,
            none,
        )
        && loader.link_fused(
            fuse,
            at(pinned::bases::BASE_AT),
            &BASE_AT_NEXT,
            base_at,
            only,
        )
        && loader.link_fused(
            fuse,
            at(pinned::bases::IN_BASE),
            &IN_BASE_NEXT,
            in_base,
            only,
        )
        && loader.link_fused(
            fuse,
            at(pinned::bases::HOLDER_AT),
            &HOLDER_AT_NEXT,
            holder_at,
            only,
        )
        && loader.link_fused(
            fuse,
            at(pinned::bases::ZONES_AT),
            &ZONES_AT_NEXT,
            zones_at,
            only,
        )
        && loader.link_fused(fuse, at(pinned::bases::CURSOR), &CHECK_NEXT, check, none)
}

crate::engine::chains::chain_detour! {
    /// The table's `building.max_altars`: the game's altar limit for a flame
    /// level, through the mods' filters in load order.
    fn max_altars(balancing: u64, flame_level: u32) -> u8;
    next MAX_ALTARS_NEXT;
    fuse &ALTAR_LIMIT, "the altar limit";
    |onward| {
        let vanilla = onward.call();
        let Some(limits) = LIMITS.sealed() else {
            return vanilla;
        };
        let running = limits
            .iter()
            .filter(|f| !crate::engine::loader::is_off(f.owner));
        chain_limit(flame_level, vanilla, base_limit().zones, running)
    }
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
        header: enshwick_core::abi::Header::of::<AltarLimit>(),
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
        crate::engine::profile::call(
            filter.owner,
            crate::engine::profile::Feature::Altars,
            || unsafe {
                (filter.callback)(&raw mut limit, filter.user());
            },
        );
        limit = AltarLimit {
            allowed: limit.allowed,
            ..vanilla_limit
        };
    }
    limit.allowed.min(u8::try_from(zones).unwrap_or(u8::MAX))
}

crate::engine::chains::chain_detour! {
    /// The table's `bases.grouping`: the game's grouping, with its zone list
    /// let grow before it and its bases settled after it while the limit is
    /// raised.
    fn group(bases: *mut u8, world: *mut u8) -> ();
    next GROUP_NEXT;
    fuse &RAISED_FUSE, "the base grouping";
    |onward| {
        let kind = SITES
            .get()
            .filter(|_| !bases.is_null())
            // SAFETY: `bases` is the grouping's own bases object, non-null
            // by the filter.
            .map(|sites| unsafe { side_of(bases, &sites.fields) });
        if let Some(kind) = kind {
            PLANNED[kind_index(kind)].store(window::VANILLA, Ordering::Relaxed);
        }
        if let Some(sites) = SITES.get().filter(|_| raised() && !bases.is_null())
            && let Some(loader) = crate::engine::loader::loader()
            && RAISED_FUSE.verify(&loader.build.name, || {
                let image = usize::try_from(loader.base)
                    .ok()
                    .and_then(crate::engine::live::Image::at);
                super::check::object_holds(bases.addr(), &sites.fields, image)
            })
        {
            // SAFETY: `bases` is the grouping's own live object, non-null by
            // the filter, before the game's grouping copies the world's zone
            // list.
            unsafe { members::let_zone_list_grow(bases, sites.fields) };
        }
        onward.call();
        let (Some(sites), Some(kind)) = (SITES.get().filter(|_| raised()), kind) else {
            return;
        };
        // SAFETY: `bases` is non-null, since `kind` is known, and is the
        // bases object the grouping has just filled, on the grouping's
        // thread; the game's new base code is what `ADD_BASE_NEXT` holds past
        // our link.
        if let Some(grouped) = unsafe { regroup(bases, kind, sites) } {
            // SAFETY: `bases` is non-null and is the object the game's
            // grouping and `settle` have just filled, on this thread.
            unsafe { keep(bases, kind, sites.fields, &grouped) };
        } else {
                Side::of(kind).forget();
                let why = "the game's grouping left bases the loader cannot settle";
                RAISED_FUSE.turn_off(&Off::fault(RAISED_ZONES, why));
        }
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
    let listed = crate::world::base_groups::lookup::Bases::listed(&game);
    let zone = |i: usize| Zone {
        bounds: crate::world::base_groups::lookup::Bases::zone(&game, i),
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

crate::engine::chains::chain_detour! {
    /// The table's `bases.new_base`: the game's new base code, held back
    /// past the bases the raised grouping plans.
    fn add_base(bases: *mut u8, zone: *const u8, index: u32) -> ();
    next ADD_BASE_NEXT;
    fuse &RAISED_FUSE, "the base plan";
    |onward| {
        if let Some(fields) = SITES
            .get()
            .filter(|_| raised() && !bases.is_null())
            .map(|sites| sites.fields)
        {
            // SAFETY: `bases` is non-null by the filter before it and is the
            // bases object the grouping passes; its u32 base count sits at
            // the build table's `bases.count`.
            let started = unsafe { read_at::<u32>(bases, fields.count) };
            // SAFETY: `bases` is non-null by the filter and is the live
            // object the grouping passes to its new base code.
            let kind = unsafe { side_of(bases, &fields) };
            let planned = &PLANNED[kind_index(kind)];
            if started == 0 {
                // SAFETY: the game's grouping calls the new base code only
                // after it refilled the object's zone list, on its own
                // thread.
                let plan = unsafe { members::planned(bases, fields, ASKED.bases(kind)) };
                planned.store(plan, Ordering::Relaxed);
            }
            let limit = planned.load(Ordering::Relaxed).min(window::VANILLA);
            if usize::try_from(started).map_or(true, |n| n >= limit) {
                return;
            }
        }
        onward.call();
    }
}

/// The view of `bases` for a lookup, while the raised limit holds.
///
/// # Safety
///
/// - `bases` is a live bases object, read on a thread the grouping does not
///   run beside.
pub(super) unsafe fn view(bases: *const u8) -> Option<Game> {
    let sites = SITES.get().filter(|_| raised())?;
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

crate::engine::chains::chain_detour! {
    /// The table's `bases.base_at`: the base that holds a point, over every
    /// base while the limit is raised.
    fn base_at(out: *mut u8, bases: *const u8, point: *const [f32; 3]) -> *mut u8;
    next BASE_AT_NEXT;
    fuse &RAISED_FUSE, "a base lookup";
    |onward| {
        // SAFETY: the lookup's own point, readable when not null.
        let p = unsafe { point_of(point) };
        // SAFETY: the lookup's own object, live, on a thread beside no
        // grouping.
        let game = unsafe { view(bases) };
        if let (Some(p), Some(game), false) = (p, game, out.is_null()) {
            // SAFETY: the lookup's own answer, writable for 0x10 bytes.
            unsafe { members::base_at(out, &game, p) };
            return out;
        }
        onward.call()
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `bases.in_base`: whether base `i` holds a point, over
    /// every base while the limit is raised.
    fn in_base(bases: *const u8, i: u64, point: *const [f32; 3]) -> bool;
    next IN_BASE_NEXT;
    fuse &RAISED_FUSE, "a base lookup";
    |onward| {
        // SAFETY: the lookup's own point, readable when not null.
        let p = unsafe { point_of(point) };
        // SAFETY: the lookup's own object, live, on a thread beside no
        // grouping.
        let game = unsafe { view(bases) };
        if let (Some(p), Some(game), Ok(i)) = (p, game, usize::try_from(i))
            && game.holds_index(i)
        {
            return members::in_base(&game, i, p);
        }
        onward.call()
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `bases.holder_at`: the box that holds a point, over every
    /// base while the limit is raised.
    fn holder_at(out: *mut u8, bases: *const u8, point: *const [f32; 3]) -> *mut u8;
    next HOLDER_AT_NEXT;
    fuse &RAISED_FUSE, "a zone lookup";
    |onward| {
        // SAFETY: the lookup's own point, readable when not null.
        let p = unsafe { point_of(point) };
        // SAFETY: the lookup's own object, live, on a thread beside no
        // grouping.
        let game = unsafe { view(bases) };
        if let (Some(p), Some(game), false) = (p, game, out.is_null()) {
            // SAFETY: the lookup's own answer, writable for 0x30 bytes.
            unsafe { members::holder_at(out, &game, p) };
            return out;
        }
        onward.call()
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `bases.zones_at`: the zone boxes that hold a point, over
    /// every base while the limit is raised.
    fn zones_at(
        out: *mut u8,
        list: *mut u8,
        bases: *const u8,
        point: *const [f32; 3],
    ) -> *mut u8;
    next ZONES_AT_NEXT;
    fuse &RAISED_FUSE, "a zone list lookup";
    |onward| {
        // SAFETY: the lookup's own point, readable when not null.
        let p = unsafe { point_of(point) };
        // SAFETY: the lookup's own object, live, on a thread beside no
        // grouping.
        let game = unsafe { view(bases) };
        if let (Some(p), Some(game), false) = (p, game, out.is_null() || list.is_null()) {
            // SAFETY: the game's live zone box container, used on this
            // thread until the guard drops at the end of this lookup.
            let guard = unsafe { ListLen::at(list, game.fields().container) };
            // SAFETY: the lookup's own answer and the game's live zone box
            // container with its own grow callback.
            unsafe { members::zones_at(out, list, &game, p) };
            guard.keep();
            return out;
        }
        onward.call()
    }
}

/// Puts a zone box container's length back when dropped, unless kept, so
/// after a lookup of ours panicked the game's own lookup appends to the
/// boxes it had.
pub(super) struct ListLen {
    list: *mut u8,
    container: Container,
    len: u64,
    kept: bool,
}

impl ListLen {
    /// # Safety
    ///
    /// - `list` is the game's live zone box container, laid out as
    ///   `container`, used on this thread until the guard drops.
    pub(super) unsafe fn at(list: *mut u8, container: Container) -> Self {
        Self {
            list,
            container,
            // SAFETY: the caller guarantees the live container.
            len: unsafe { members::list_len(list, &container) },
            kept: false,
        }
    }

    /// Leaves the container as the lookup filled it.
    pub(super) fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for ListLen {
    fn drop(&mut self) {
        if !self.kept {
            // SAFETY: `at`'s caller guarantees the live container until this
            // drop; it held `len` boxes then, which the lookup only appended
            // to.
            unsafe { members::set_list_len(self.list, &self.container, self.len) };
        }
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `bases.cursor`: the game's placement check, then an altar
    /// past the raised limit refused.
    fn check(
        flags: *mut u32,
        ctx: *const u8,
        transform: *const i64,
        item: *const u8,
        mode: u64,
        extra: u64,
    ) -> ();
    next CHECK_NEXT;
    fuse &RAISED_FUSE, "the placement check";
    |onward| {
        onward.call();
        if !raised() || flags.is_null() || ctx.is_null() || transform.is_null() || item.is_null() {
            return;
        }
        // SAFETY: the cursor's own arguments, each non-null by the check
        // before it, on the cursor's thread after the original ran.
        unsafe { cursor::refuse_past_limit(flags, ctx, transform, item) };
    }
}
