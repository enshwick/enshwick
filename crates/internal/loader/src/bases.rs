//! The game's bases past 16 altar zones, and the altars each Flame level
//! allows.
//!
//! The game puts zones that touch into one base. Its grouping tests only the
//! first 16 zones and starts every later zone as a base of its own, and its
//! array of bases holds 16 with no check: a 17th base writes over the zone
//! list. Its members masks are 32 bits wide. Raised, found by byte patterns:
//!
//! - the members mask the grouping starts with is built with a 64-bit shift,
//!   so 32 zones give 32 bits instead of none;
//! - the grouping and the three lookups that walk a base's members walk 32
//!   zones instead of 16;
//! - the new base call refuses a 17th base: that zone still allows building,
//!   but is part of no base;
//! - the placement cursor refuses an altar that would start a 17th base, with
//!   the game's own "too many altars" answer.
//!
//! The patches and the links are one change: the patches need the links
//! armed with them, and the links act only once the patches are written, so
//! either both are in or the game keeps its 16 zones.
//!
//! The client runs one grouping for its own world and the server it hosts, so
//! the same patches and links serve both.

use crate::mod_index::ModIndex;
use crate::pinned;
use crate::pinned::Constant;

mod limit;

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use enshwick::abi::{AltarLimit, AltarLimitFilterFn, HookFlags, Status};
use enshwick::raw::{read_at, read_ptr_at, write_at};

use self::limit::{Aabb, MAX_BASES, MAX_ZONES, VANILLA_ZONES, Zone, Zones};
use crate::chains::Next;
use crate::code::{CodePatch, Verdict};
use crate::host::{Loader, Registered};
use crate::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::bases::COUNT.id(),
    pinned::bases::MEMBERS.id(),
    pinned::bases::STRIDE.id(),
    pinned::bases::ZONE_HIGH.id(),
    pinned::bases::ZONE_LIST.id(),
    pinned::bases::ZONE_LIST_LEN.id(),
    pinned::bases::ZONE_SIZE.id(),
    pinned::building::MAX_ALTARS.id(),
];

/// The grouping's first bytes.
const GROUPING: &str = "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 41 54 41 55 41 56 41 57 \
                        48 83 EC 50 45 33 E4 4C 8B F2 4C 8B E9 41 8B EC 44 39 61 24";
/// The new base call's first bytes: the count at +0x24, bases 0x130 apart.
const NEW_BASE: &str = "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 41 56 41 57 48 83 EC 20 \
                        44 8B 49 24 48 8B F1 49 69 D9 30 01 00 00 41 8D 41 01";
/// In the grouping: the call that starts base 0, then `mov ecx, ebx; mov
/// edi, 1; shl edi, cl; mov r14, r12; dec edi; and edi, -2; je`.
const MASK: &str =
    "49 8B CD E8 ?? ?? ?? ?? 8B CB BF 01 00 00 00 D3 E7 4D 8B F4 FF CF 83 E7 FE 0F 84";
const MASK_AT: usize = 8;
const MASK_VANILLA: [u8; 17] = [
    0x8B, 0xCB, 0xBF, 0x01, 0x00, 0x00, 0x00, 0xD3, 0xE7, 0x4D, 0x8B, 0xF4, 0xFF, 0xCF, 0x83, 0xE7,
    0xFE,
];
/// `mov ecx, ebx; nop; xor edi, edi; bts rdi, rcx; mov r14, r12; dec edi;
/// and edi, -2`: 32 zones set the low 32 bits.
const MASK_WIDE: [u8; 17] = [
    0x8B, 0xCB, 0x90, 0x31, 0xFF, 0x48, 0x0F, 0xAB, 0xCF, 0x4D, 0x8B, 0xF4, 0xFF, 0xCF, 0x83, 0xE7,
    0xFE,
];
/// The grouping's member walk (`cmp r9, 0x200`: 16 zones of 0x20 bytes) and
/// candidate walk (`cmp r11d, 0x10`).
const GROUPING_LOOPS: &str = "49 83 C1 20 49 81 F9 00 02 00 00 72 ?? 41 FF C3 41 83 FB 10 0F 82";
const CANDIDATES_AT: usize = 19;
/// The member walks of the lookups: whether a point is in base i, which base
/// holds a point, which zone holds a point.
const MEMBER_WALKS: [&str; 3] = [
    "48 83 C2 20 48 81 FA 00 02 00 00 72 ?? 32 C0 48 83 C4 18 C3",
    "48 83 C2 20 48 81 FA 00 02 00 00 72 ?? 49 FF C3 49 81 C0 30 01 00 00",
    "48 83 C2 20 48 81 FA 00 02 00 00 72 ?? 48 FF C3 49 81 C1 30 01 00 00",
];
/// The high byte of a member walk's end, `0x200` becoming `0x400`.
const WALK_END_AT: usize = 8;
/// The placement cursor's first bytes, up to `and dword [rcx], 0xfffc9581`.
const CURSOR: &str = "48 8B C4 4C 89 48 20 4C 89 40 18 55 56 57 41 55 41 57 48 8D A8 88 FE FF FF \
                      48 81 EC 50 02 00 00 81 21 81 95 FC FF";
/// In the cursor: an altar (`item+0x43`) is refused when `max_altars(ctx
/// balancing, flame level)` is not above the altar count.
const CURSOR_ALTAR: &str = "80 7F 43 00 0F B6 9D A0 01 00 00 74 ?? F6 C3 10 75 ?? 41 81 0F 00 40 00 00 \
                            48 8B 96 ?? ?? ?? ?? 48 8B 8E ?? ?? ?? ?? 8B 52 14 E8";
const CURSOR_BALANCING_AT: usize = 35;
const CURSOR_SPAN: usize = 0x400;
/// The cursor's "too many altars" bit.
/// Found on the 1.0 build 1269579, server and client.
const MAX_ALTARS_REACHED: u32 = 0x40;
/// The item flag that marks an altar.
/// Found on the 1.0 build 1269579, server and client.
const ITEM_ALTAR: usize = 0x43;

/// The bases object's fields, from the build table: the count, each base
/// `stride` apart with its members mask at `members`, and the zone boxes in
/// a list of `zone_size` bytes each, the low corner first and the high one
/// at `zone_high`.
#[derive(Debug, Clone, Copy)]
struct Fields {
    count: usize,
    stride: usize,
    members: usize,
    zone_list: usize,
    zone_list_len: usize,
    zone_size: usize,
    zone_high: usize,
}

impl Fields {
    fn read(loader: &Loader) -> Result<Self, String> {
        let f = |id: Constant| {
            loader
                .offset(id)
                .ok_or(format!("{id} is not in this build"))
        };
        Ok(Self {
            count: f(pinned::bases::COUNT)?,
            stride: f(pinned::bases::STRIDE)?,
            members: f(pinned::bases::MEMBERS)?,
            zone_list: f(pinned::bases::ZONE_LIST)?,
            zone_list_len: f(pinned::bases::ZONE_LIST_LEN)?,
            zone_size: f(pinned::bases::ZONE_SIZE)?,
            zone_high: f(pinned::bases::ZONE_HIGH)?,
        })
    }
}
/// The game lists at most 255 zones.
/// Found on the 1.0 build 1269579, server and client.
const MAX_LISTED: usize = 0xff;

#[derive(Debug, Clone)]
struct Sites {
    grouping: u64,
    new_base: u64,
    cursor: u64,
    balancing: usize,
    fields: Fields,
    /// The first byte of every patch.
    patches: Box<[usize]>,
}

type LimitFilter = Registered<AltarLimitFilterFn>;

static SITES: OnceLock<Sites> = OnceLock::new();
/// The mods' altar limit filters, in load order.
static LIMITS: Registry<LimitFilter> = Registry::new();
static ZONES: OnceLock<ArcSwap<Zones>> = OnceLock::new();
/// How many zones the world has, `u32::MAX` before the first grouping.
static ZONE_COUNT: AtomicU32 = AtomicU32::new(u32::MAX);
/// Set at arming when every patch is written with the links.
static RAISED: AtomicBool = AtomicBool::new(false);

// SAFETY: `GroupFn` is the signature of the base grouping the `GROUPING`
// pattern finds, and `link` uses this slot only as that function's next.
static GROUP_NEXT: Next<GroupFn> = unsafe { Next::new() };
// SAFETY: `AddBaseFn` is the signature of the new base code the `NEW_BASE`
// pattern finds, and `link` uses this slot only as that function's next.
static ADD_BASE_NEXT: Next<AddBaseFn> = unsafe { Next::new() };
// SAFETY: `CheckFn` is the signature of the placement cursor the `CURSOR`
// pattern finds, and `link` uses this slot only as that function's next.
static CHECK_NEXT: Next<CheckFn> = unsafe { Next::new() };
// SAFETY: `MaxAltarsFn` is the signature of the build table's
// `building.max_altars`, and `link` uses this slot only as that function's
// next.
static MAX_ALTARS_NEXT: Next<MaxAltarsFn> = unsafe { Next::new() };

type GroupFn = unsafe extern "system" fn(bases: *mut u8, world: *mut u8);
type AddBaseFn = unsafe extern "system" fn(bases: *mut u8, zone: *const u8, index: u32);
type CheckFn = unsafe extern "system" fn(
    flags: *mut u32,
    ctx: *const u8,
    transform: *const i64,
    item: *const u8,
    mode: u64,
    extra: u64,
);
type MaxAltarsFn = unsafe extern "system" fn(balancing: u64, flame_level: u32) -> u8;

fn too_late(loader: &Loader) -> bool {
    loader.delivery.get().is_some() || LIMITS.is_sealed()
}

/// Adds mod `owner`'s altar limit filter; during init only.
pub(crate) fn add_limit(
    loader: &Loader,
    owner: ModIndex,
    callback: AltarLimitFilterFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if loader.address(pinned::building::MAX_ALTARS).is_none() {
        return Status::MISSING;
    }
    LIMITS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// The most altar zones and bases the game groups now: its own, or the
/// raised maximum once the patches are in.
pub(crate) fn base_limit() -> (u32, u32) {
    if raised() {
        (u32::try_from(MAX_ZONES).unwrap_or(u32::MAX), MAX_BASES)
    } else {
        (VANILLA_ZONES, MAX_BASES)
    }
}

/// Finds every site and registers the patches as mod `owner`'s, all or
/// none; a miss or a refusal leaves nothing registered.
pub(crate) fn raise(loader: &Loader, owner: ModIndex) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if SITES.get().is_some() {
        return Status::OK;
    }
    if crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    let (Some(text), Some(code)) = (
        crate::code::text(loader.base),
        crate::code::text_bytes(loader.base),
    ) else {
        return Status::MISSING;
    };
    let patches = match Fields::read(loader).and_then(|fields| find_sites(code, fields)) {
        Ok(found) => found,
        Err(reason) => {
            crate::log!(WARN, "bases keep 16 altar zones: {reason}");
            return Status::MISSING;
        }
    };
    let (sites, patches) = patches;
    let start = text.0 as u64;
    let sites = Sites {
        grouping: start + sites.grouping,
        new_base: start + sites.new_base,
        cursor: start + sites.cursor,
        balancing: sites.balancing,
        fields: sites.fields,
        patches: patches.iter().map(|&(at, _, _)| text.0 + at).collect(),
    };
    let needs: Box<[u64]> = Box::new([sites.grouping, sites.new_base, sites.cursor]);
    let patches = patches
        .into_iter()
        .map(|(at, expected, replacement)| CodePatch {
            owner,
            address: text.0 + at,
            expected: expected.into(),
            replacement: replacement.into(),
            needs: needs.clone(),
        })
        .collect();
    let status = crate::code::register_all(patches, text);
    if !status.is_ok() {
        return status;
    }
    let _first = SITES.set(sites);
    let _first = ZONES.set(ArcSwap::from_pointee(Zones::default()));
    Status::OK
}

type Patch = (usize, Vec<u8>, Vec<u8>);

/// The sites as offsets into `code`, and the patches they need.
fn find_sites(code: &[u8], fields: Fields) -> Result<(Sites, Vec<Patch>), String> {
    let find = |pattern: &str| crate::code::find(code, pattern);
    let grouping = find(GROUPING)?;
    let new_base = find(NEW_BASE)?;
    let mask = find(MASK)?;
    let call: [u8; 4] = code
        .get(mask + 4..mask + 8)
        .and_then(|b| b.try_into().ok())
        .ok_or("the mask call is cut")?;
    let target = (mask + MASK_AT).wrapping_add_signed(i32::from_le_bytes(call) as isize);
    if target != new_base || !(grouping..grouping + 0x200).contains(&mask) {
        return Err("the grouping does not call the new base code as expected".into());
    }
    let loops = find(GROUPING_LOOPS)?;
    let mut walks = [0; 3];
    for (walk, pattern) in walks.iter_mut().zip(MEMBER_WALKS) {
        *walk = find(pattern)?;
    }
    let cursor = find(CURSOR)?;
    let altar = find(CURSOR_ALTAR)?;
    if !(cursor..cursor + CURSOR_SPAN).contains(&altar) {
        return Err("the cursor's altar check is not in the cursor".into());
    }
    let offset: [u8; 4] = code
        .get(altar + CURSOR_BALANCING_AT..altar + CURSOR_BALANCING_AT + 4)
        .and_then(|b| b.try_into().ok())
        .ok_or("the cursor is cut")?;
    let balancing = usize::try_from(u32::from_le_bytes(offset)).map_err(|e| e.to_string())?;
    let mut patches: Vec<Patch> = vec![
        (mask + MASK_AT, MASK_VANILLA.to_vec(), MASK_WIDE.to_vec()),
        (loops + WALK_END_AT, vec![0x02], vec![0x04]),
        (loops + CANDIDATES_AT, vec![0x10], vec![0x20]),
    ];
    patches.extend(
        walks
            .iter()
            .map(|&w| (w + WALK_END_AT, vec![0x02], vec![0x04])),
    );
    let sites = Sites {
        grouping: grouping as u64,
        new_base: new_base as u64,
        cursor: cursor as u64,
        balancing,
        fields,
        patches: Box::default(),
    };
    Ok((sites, patches))
}

/// Settles, before arming writes anything, whether the limit is raised:
/// every patch written, which the patches' needs tie to the links.
pub(crate) fn arming(patches: &[CodePatch], verdicts: &[Verdict]) {
    let raised = SITES.get().is_some_and(|sites| {
        sites.patches.iter().all(|&at| {
            patches
                .iter()
                .zip(verdicts)
                .any(|(p, v)| p.address == at && *v == Verdict::Write)
        })
    });
    RAISED.store(raised, Ordering::Release);
}

fn raised() -> bool {
    RAISED.load(Ordering::Acquire)
}

/// The zones and bases of the last grouping.
pub(crate) fn zones() -> Option<(u32, u32)> {
    if !raised() {
        return None;
    }
    let count = ZONE_COUNT.load(Ordering::Relaxed);
    let known = ZONES.get()?;
    (count != u32::MAX).then(|| (count, known.load().bases))
}

pub(crate) fn forget() {
    if let Some(zones) = ZONES.get() {
        zones.store(Arc::new(Zones::default()));
    }
    ZONE_COUNT.store(u32::MAX, Ordering::Relaxed);
}

pub(crate) fn link(loader: &'static Loader) {
    let limits = LIMITS.seal();
    if !limits.is_empty()
        && let Some(target) = loader.address(pinned::building::MAX_ALTARS)
    {
        let status = loader.link(target, &MAX_ALTARS_NEXT, max_altars, HookFlags::NONE);
        if !status.is_ok() {
            crate::log!(WARN, "altar limit: refused ({})", status.0);
        }
    }
    let Some(sites) = SITES.get() else {
        return;
    };
    // A link refused here leaves the patches without a hook they need, so
    // arming keeps them out.
    let refused = [
        (
            "base grouping",
            loader.link(sites.grouping, &GROUP_NEXT, group, HookFlags::NONE),
        ),
        (
            "new base",
            loader.link(sites.new_base, &ADD_BASE_NEXT, add_base, HookFlags::NONE),
        ),
        (
            "placement cursor",
            loader.link(sites.cursor, &CHECK_NEXT, check, HookFlags::NONE),
        ),
    ]
    .into_iter()
    .filter(|(_, status)| !status.is_ok())
    .inspect(|(name, status)| crate::log!(WARN, "{name}: refused ({})", status.0))
    .count();
    if refused == 0 {
        crate::log!(INFO, "bases take {MAX_ZONES} altar zones");
    } else {
        crate::log!(WARN, "bases keep 16 altar zones: a link was refused");
    }
}

/// The raised base count's switch; the cap at the game's 16 bases holds
/// either way.
static BASES_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("more bases");

unsafe extern "system" fn max_altars(balancing: u64, flame_level: u32) -> u8 {
    // SAFETY: this detour runs only as the `building.max_altars` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let vanilla = unsafe { MAX_ALTARS_NEXT.get()(balancing, flame_level) };
    let Some(limits) = LIMITS.sealed() else {
        return vanilla;
    };
    let running = limits.iter().filter(|f| !crate::host::is_off(f.owner));
    let (zones, _) = base_limit();
    chain_limit(flame_level, vanilla, zones, running)
}

/// Runs `filters` in order on the limit of `flame_level`, where the game
/// allows `vanilla`, each on the count the last one left; the answer is
/// kept within `zones`.
fn chain_limit<'a>(
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

unsafe extern "system" fn group(bases: *mut u8, world: *mut u8) {
    // SAFETY: this detour runs only as the base grouping's link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { GROUP_NEXT.get()(bases, world) };
    if raised() && !bases.is_null() {
        // SAFETY: `bases` is non-null by the check before it and is the bases
        // object the grouping has just filled, on the grouping's thread.
        let _done = BASES_FUSE.run("the base grouping", || unsafe { keep(bases) });
    }
}

/// Keeps what the grouping found when that changed.
///
/// # Safety
///
/// - `bases` is the non-null bases object the grouping has just filled,
///   live for the call.
/// - Called on the grouping's thread, right after it, with no writer
///   meanwhile.
unsafe fn keep(bases: *const u8) {
    let (Some(known), Some(sites)) = (ZONES.get(), SITES.get()) else {
        return;
    };
    let fields = sites.fields;
    // SAFETY: the caller guarantees a live bases object; its u32 base count
    // sits at the build table's `bases.count` offset.
    let count = unsafe { read_at::<u32>(bases, fields.count) };
    // SAFETY: the caller guarantees a live bases object; its zone list
    // pointer sits at the build table's `bases.zone_list` offset.
    let list = unsafe { read_ptr_at::<u8>(bases, fields.zone_list) }.cast_const();
    // SAFETY: the caller guarantees a live bases object; its zone list
    // length sits at the build table's `bases.zone_list_len` offset.
    let len = unsafe { read_at::<usize>(bases, fields.zone_list_len) };
    if (list.is_null() && len != 0) || len > MAX_LISTED {
        return;
    }
    let count = count.min(MAX_BASES);
    let mut members = 0u32;
    for base in 0..count as usize {
        // SAFETY: `base` is below the count, capped at the game's 16 bases,
        // so its members mask lies inside the live object's base array of
        // the build table's `bases.stride`, at `bases.members`.
        members |= unsafe { read_at::<u32>(bases, base * fields.stride + fields.members) };
    }
    let zone = |i: usize| -> Zone {
        let at = i * fields.zone_size;
        // SAFETY: callers pass `i` below the list's length, which the check
        // before it bounds and pairs with a non-null list; zone `i` of the build
        // table's `bases.zone_size` starts with its low corner.
        let min = unsafe { read_at::<[f32; 3]>(list, at) };
        // SAFETY: callers pass `i` below the list's length, so zone `i` is in
        // the list; its high corner sits at the build table's
        // `bases.zone_high` offset.
        let max = unsafe { read_at::<[f32; 3]>(list, at + fields.zone_high) };
        Zone {
            bounds: Aabb { min, max },
            based: i < MAX_ZONES && members >> i & 1 == 1,
        }
    };
    ZONE_COUNT.store(
        u32::try_from(len).unwrap_or(u32::MAX - 1),
        Ordering::Relaxed,
    );
    let current = known.load();
    if current.bases == count
        && current.zones.len() == len
        && (0..len).all(|i| current.zones[i] == zone(i))
    {
        return;
    }
    let zones = Zones {
        zones: (0..len).map(zone).collect(),
        bases: count,
    };
    let apart = zones.apart();
    let shape = |z: &Zones| (z.zones.len(), z.bases, z.apart());
    if shape(&current) != shape(&zones) {
        if apart > 0 {
            crate::log!(
                WARN,
                "{len} altar zones in {count} bases: {apart} stand apart from every base, since the game holds {MAX_BASES} bases; they allow building but no base features until they touch a base's zone"
            );
        } else {
            crate::log!(INFO, "{len} altar zones in {count} bases");
        }
    }
    known.store(Arc::new(zones));
}

unsafe extern "system" fn add_base(bases: *mut u8, zone: *const u8, index: u32) {
    let count = SITES.get().map(|sites| sites.fields.count);
    if let Some(count) = count.filter(|_| raised() && !bases.is_null())
        // SAFETY: `bases` is non-null by the filter before it and is the bases
        // object the grouping passes; its u32 base count sits at the build
        // table's `bases.count` offset.
        && unsafe { read_at::<u32>(bases, count) } >= MAX_BASES
    {
        return;
    }
    // SAFETY: this detour runs only as the new base code's link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { ADD_BASE_NEXT.get()(bases, zone, index) };
}

unsafe extern "system" fn check(
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
        refuse_17th(flags, ctx, transform, item);
    });
}

/// Sets the cursor's "too many altars" bit for an altar whose zone would
/// stand apart from every base once the game holds its 16.
///
/// # Safety
///
/// - `flags`, `ctx`, `transform` and `item` are the placement cursor's
///   own, non-null arguments, live for the call.
/// - Called on the cursor's thread after the original ran, which alone
///   writes `flags` meanwhile.
unsafe fn refuse_17th(flags: *mut u32, ctx: *const u8, transform: *const i64, item: *const u8) {
    // SAFETY: the caller guarantees the cursor's live item definition; its
    // u8 altar flag sits at `ITEM_ALTAR` (1.0 build 1269579).
    let altar = unsafe { read_at::<u8>(item, ITEM_ALTAR) } != 0;
    // SAFETY: the caller guarantees the cursor's live u32 flags word.
    if !altar || unsafe { read_at::<u32>(flags.cast_const().cast(), 0) } & MAX_ALTARS_REACHED != 0 {
        return;
    }
    let (Some(sites), Some(known), Some(engine)) =
        (SITES.get(), ZONES.get(), crate::ecs::resolved())
    else {
        return;
    };
    let zones = known.load();
    if zones.bases < MAX_BASES {
        return;
    }
    // SAFETY: the caller guarantees the cursor's live context; its balancing
    // registry word sits at the offset the `CURSOR_ALTAR` pattern read from
    // the cursor's own code.
    let balancing = unsafe { read_at::<u64>(ctx, sites.balancing) };
    if balancing == 0 {
        return;
    }
    // SAFETY: `balancing` is the running cursor's non-zero balancing
    // registry word, used on the cursor's thread.
    let Some(size) = (unsafe { engine.zone_size(balancing, 1) }) else {
        return;
    };
    // SAFETY: the caller guarantees the cursor's live transform, which
    // starts with the position, three i64.
    let fixed = unsafe { read_at::<[i64; 3]>(transform.cast(), 0) };
    let candidate = limit::candidate(enshwick::ecs::metres(fixed), size);
    if zones.refuses(&candidate) {
        // SAFETY: the caller guarantees the cursor's live u32 flags word.
        let refused = unsafe { read_at::<u32>(flags.cast_const().cast(), 0) } | MAX_ALTARS_REACHED;
        // SAFETY: the caller guarantees the cursor's live u32 flags word,
        // which only the cursor's thread writes meanwhile.
        unsafe { write_at(flags.cast(), 0, refused) };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::locks::Relock;

    /// The level each run of `plus_vanilla` saw, in order.
    static SEEN: Mutex<Vec<u32>> = Mutex::new(Vec::new());

    unsafe extern "system" fn plus_vanilla(limit: *mut AltarLimit, _user: *mut c_void) {
        // SAFETY: the loader's limit, writable for the call.
        let limit = unsafe { &mut *limit };
        SEEN.relock().push(limit.flame_level);
        limit.allowed = limit.allowed.saturating_add(limit.vanilla);
        limit.vanilla = 0;
        limit.flame_level = 99;
    }

    #[test]
    fn limit_filters_chain_on_the_count_so_far_within_the_zones() {
        let filter = LimitFilter::new(ModIndex::new(0), plus_vanilla, core::ptr::null_mut());
        assert_eq!(chain_limit(3, 6, 16, core::iter::empty()), 6);
        assert_eq!(chain_limit(3, 6, 32, [&filter].into_iter()), 12);
        SEEN.relock().clear();
        assert_eq!(
            chain_limit(3, 6, 32, [&filter, &filter].into_iter()),
            18,
            "each filter sees the game's own count"
        );
        assert_eq!(*SEEN.relock(), [3, 3], "each filter sees the level");
        assert_eq!(chain_limit(3, 6, 16, [&filter, &filter].into_iter()), 16);
    }
}
