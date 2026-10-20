//! Altars past 16: the game's base grouping, patched to take 32 zones, never
//! with more than 16 bases.
//!
//! The game puts zones that touch into one base. Its grouping tests only the
//! first 16 zones and starts every later zone as a base of its own, and its
//! array of bases holds 16, with no check: a 17th base writes over the zone
//! list. Its masks of members are 32 bits wide. On both binaries, by byte
//! patterns:
//!
//! - the members mask the grouping starts with is built with a 64-bit shift,
//!   so 32 zones give 32 bits instead of none;
//! - the grouping and the three lookups that walk a base's members walk 32
//!   zones instead of 16;
//! - a hook on the new base call refuses a 17th base: that zone still allows
//!   building, but is part of no base;
//! - a hook on the placement cursor refuses an altar that would start a 17th
//!   base, with the game's own "too many altars" answer;
//! - a hook after the grouping keeps what it found, for the cursor and the
//!   log, and on the dedicated server holds the game on a world past 16
//!   zones, which the vanilla grouping cannot open safely.
//!
//! The client runs one grouping for its own world and the server it hosts,
//! so the same patches and hooks serve both.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use arc_swap::ArcSwap;
use enshwick_api::abi::Side;
use enshwick_api::state::Hold;
use enshwick_api::{Load, Next, Off};

use crate::limit::{self, Aabb, MAX_BASES, MAX_ZONES, VANILLA_ZONES, Zone, Zones};

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
/// Where the mask code starts, from [`MASK`].
const MASK_AT: usize = 8;
/// The mask code as vanilla has it.
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
/// The candidate count in [`GROUPING_LOOPS`].
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
/// The balancing's offset in the cursor context, from [`CURSOR_ALTAR`].
const CURSOR_BALANCING_AT: usize = 35;
/// How far into the cursor [`CURSOR_ALTAR`] may stand.
const CURSOR_SPAN: u64 = 0x400;
/// The cursor's "too many altars" bit.
const MAX_ALTARS_REACHED: u32 = 0x40;
/// The item flag that marks an altar.
const ITEM_ALTAR: usize = 0x43;

/// The bases object: the count, each base 0x130 apart with its members mask
/// at +0x140 and the zone boxes at +0x1330, 0x20 bytes each with the low
/// corner at 0 and the high one at 0x10.
const BASE_COUNT: usize = 0x24;
const BASE_STRIDE: usize = 0x130;
const BASE_MEMBERS: usize = 0x140;
const ZONE_LIST: usize = 0x1330;
const ZONE_LIST_LEN: usize = 0x1338;
const ZONE_SIZE: usize = 0x20;
const ZONE_HIGH: usize = 0x10;
/// The most zones the game lists.
const MAX_LISTED: usize = 0xff;
/// Engine positions are 32.32 fixed point.
const FIXED_PER_METRE: f64 = 4_294_967_296.0;
/// How often the dedicated server checks whether its hold must change.
const HOLD_EVERY: Duration = Duration::from_secs(5);

type ZoneSize = unsafe extern "system" fn(out: *mut f32, balancing: u64, level: u64) -> u64;

/// What the hooks need from the game.
#[derive(Debug)]
struct Engine {
    zone_size: ZoneSize,
    cursor_balancing: usize,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
/// The zones and bases the grouping last left.
static ZONES: OnceLock<ArcSwap<Zones>> = OnceLock::new();
/// How many zones the world has, `u32::MAX` before the first grouping.
static ZONE_COUNT: AtomicU32 = AtomicU32::new(u32::MAX);

enshwick_api::detour! {
    static GROUP: unsafe extern "system" fn(bases: *mut u8, world: *mut u8) = group;
}
enshwick_api::detour! {
    static ADD_BASE: unsafe extern "system" fn(bases: *mut u8, zone: *const u8, index: u32) = add_base;
}
enshwick_api::detour! {
    static CHECK: unsafe extern "system" fn(
        flags: *mut u32,
        ctx: *const u8,
        transform: *const i64,
        item: *const u8,
        mode: u64,
        extra: u64,
    ) = check;
}

/// Reads a `T` at `base + offset`.
///
/// # Safety
///
/// `base + offset` is readable for a `T`.
unsafe fn read_at<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

/// Every site, found before anything is registered, so a miss leaves the
/// game untouched.
#[derive(Debug)]
struct Sites {
    grouping: u64,
    new_base: u64,
    mask: u64,
    loops: u64,
    walks: [u64; 3],
    cursor: u64,
    balancing: usize,
}

impl Sites {
    fn find(load: &Load<'_>) -> Result<Self, Off> {
        let code = load.code()?;
        let start = code.as_ptr().addr() as u64;
        let bytes = |at: u64, len: usize| -> Result<&[u8], Off> {
            let from = usize::try_from(at.wrapping_sub(start))
                .map_err(|_| Off::new("site out of range"))?;
            code.get(from..from + len)
                .ok_or_else(|| Off::new("site out of range"))
        };
        let grouping = load.find_code(GROUPING)?;
        let new_base = load.find_code(NEW_BASE)?;
        let mask = load.find_code(MASK)?;
        // The call before the mask code must start base 0 through `new_base`.
        let call: [u8; 4] = bytes(mask + 4, 4)?
            .try_into()
            .map_err(|_| Off::new("call"))?;
        let target =
            (mask + MASK_AT as u64).wrapping_add_signed(i64::from(i32::from_le_bytes(call)));
        if target != new_base || !(grouping..grouping + 0x200).contains(&mask) {
            return Err(Off::new(
                "the grouping does not call the new base code as expected",
            ));
        }
        let loops = load.find_code(GROUPING_LOOPS)?;
        let mut walks = [0; 3];
        for (walk, pattern) in walks.iter_mut().zip(MEMBER_WALKS) {
            *walk = load.find_code(pattern)?;
        }
        let cursor = load.find_code(CURSOR)?;
        let altar = load.find_code(CURSOR_ALTAR)?;
        if !(cursor..cursor + CURSOR_SPAN).contains(&altar) {
            return Err(Off::new("the cursor's altar check is not in the cursor"));
        }
        let offset: [u8; 4] = bytes(altar + CURSOR_BALANCING_AT as u64, 4)?
            .try_into()
            .map_err(|_| Off::new("cursor"))?;
        let balancing =
            usize::try_from(u32::from_le_bytes(offset)).map_err(|_| Off::new("cursor"))?;
        Ok(Self {
            grouping,
            new_base,
            mask,
            loops,
            walks,
            cursor,
            balancing,
        })
    }
}

/// Finds every site; `Ok(false)`, logged, when one is missing and the game
/// keeps its limit of 16. Registers the patches and hooks once all are found.
///
/// # Errors
///
/// [`Off`] when a patch or hook found is refused: the loader then writes
/// none of the mod's patches.
pub(crate) fn link(load: &mut Load<'_>) -> Result<bool, Off> {
    let sites = match Sites::find(load) {
        Ok(sites) => sites,
        Err(off) => {
            enshwick_api::warn!("the game keeps 16 altars: {off}");
            return Ok(false);
        }
    };
    let zone_size = usize::try_from(load.symbol("balancing.zone_size")?)
        .map_err(|_| Off::new("balancing.zone_size out of range"))?;
    let _first = ENGINE.set(Engine {
        // SAFETY: the loader pins this id to the zone size lookup, a function
        // of type `ZoneSize`.
        zone_size: unsafe { core::mem::transmute_copy::<usize, ZoneSize>(&zone_size) },
        cursor_balancing: sites.balancing,
    });
    let _zones = ZONES.set(ArcSwap::from_pointee(Zones::default()));
    load.patch_code(sites.mask + MASK_AT as u64, &MASK_VANILLA, &MASK_WIDE)?;
    load.patch_code(sites.loops + WALK_END_AT as u64, &[0x02], &[0x04])?;
    load.patch_code(sites.loops + CANDIDATES_AT as u64, &[0x10], &[0x20])?;
    for walk in sites.walks {
        load.patch_code(walk + WALK_END_AT as u64, &[0x02], &[0x04])?;
    }
    load.hook(&GROUP, sites.grouping)?;
    load.hook(&ADD_BASE, sites.new_base)?;
    load.hook(&CHECK, sites.cursor)?;
    if load.host().build().side == Side::SERVER {
        let hold = Hold::new(&load.data_dir()?);
        drop(
            std::thread::Builder::new()
                .name("flame-altar-count-hold".into())
                .spawn(move || keep_hold(&hold)),
        );
    }
    Ok(true)
}

/// Sets the hold while the world has more zones than vanilla groups, and
/// clears it when it has no more; runs for the process.
fn keep_hold(hold: &Hold) {
    let mut set = hold.reason().is_some();
    loop {
        std::thread::sleep(HOLD_EVERY);
        let zones = ZONE_COUNT.load(Ordering::Relaxed);
        if zones == u32::MAX {
            continue;
        }
        let past = zones > VANILLA_ZONES;
        if past == set {
            continue;
        }
        let done = if past {
            hold.set(&format!(
                "the world has {zones} altar zones; the vanilla game groups only {VANILLA_ZONES}"
            ))
        } else {
            hold.clear()
        };
        match done {
            Ok(()) => {
                set = past;
                enshwick_api::info!(
                    "hold {}: {zones} altar zones",
                    if past { "set" } else { "released" }
                );
            }
            Err(off) => enshwick_api::warn_every!(Duration::from_secs(600), "hold: {off}"),
        }
    }
}

/// Runs the grouping, then keeps what it found when that changed.
fn group(next: Next<unsafe extern "system" fn(*mut u8, *mut u8)>, bases: *mut u8, world: *mut u8) {
    // SAFETY: the original takes the arguments the game passed.
    unsafe { next.get()(bases, world) };
    if bases.is_null() {
        return;
    }
    let Some(known) = ZONES.get() else {
        return;
    };
    // SAFETY: the grouping just filled the bases object.
    let count = unsafe { read_at::<u32>(bases, BASE_COUNT) };
    // SAFETY: as above.
    let list = unsafe { read_at::<*const u8>(bases, ZONE_LIST) };
    // SAFETY: as above.
    let len = unsafe { read_at::<usize>(bases, ZONE_LIST_LEN) };
    // The game's zone list holds at most 255.
    if (list.is_null() && len != 0) || len > MAX_LISTED {
        return;
    }
    let count = count.min(MAX_BASES);
    let mut members = 0u32;
    for base in 0..count as usize {
        // SAFETY: base `base` is below the count, inside the array of 16.
        members |= unsafe { read_at::<u32>(bases, base * BASE_STRIDE + BASE_MEMBERS) };
    }
    let zone = |i: usize| -> Zone {
        let at = i * ZONE_SIZE;
        // SAFETY: zone `i` is below the list's length.
        let min = unsafe { read_at::<[f32; 3]>(list, at) };
        // SAFETY: as above.
        let max = unsafe { read_at::<[f32; 3]>(list, at + ZONE_HIGH) };
        Zone {
            bounds: Aabb { min, max },
            based: i < MAX_ZONES && members >> i & 1 == 1,
        }
    };
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
    ZONE_COUNT.store(
        u32::try_from(len).unwrap_or(u32::MAX - 1),
        Ordering::Relaxed,
    );
    let apart = zones.apart();
    let shape = |z: &Zones| (z.zones.len(), z.bases, z.apart());
    if shape(&current) == shape(&zones) {
        // Only the boxes moved, as when zones grow.
    } else if apart > 0 {
        enshwick_api::warn!(
            "{len} altar zones in {count} bases: {apart} stand apart from every base, since the game holds {MAX_BASES} bases; they allow building but no base features until they touch a base's zone"
        );
    } else {
        enshwick_api::info!("{len} altar zones in {count} bases");
    }
    known.store(Arc::new(zones));
}

/// Starts a base, unless the array of 16 is full.
fn add_base(
    next: Next<unsafe extern "system" fn(*mut u8, *const u8, u32)>,
    bases: *mut u8,
    zone: *const u8,
    index: u32,
) {
    // SAFETY: the grouping passes its bases object.
    if !bases.is_null() && unsafe { read_at::<u32>(bases, BASE_COUNT) } >= MAX_BASES {
        return;
    }
    // SAFETY: the original takes the arguments the game passed.
    unsafe { next.get()(bases, zone, index) };
}

/// Runs the cursor check, then refuses an altar that would start a 17th base.
#[expect(
    clippy::cast_precision_loss,
    reason = "world positions are far below 2^53 fixed-point units"
)]
fn check(
    next: Next<unsafe extern "system" fn(*mut u32, *const u8, *const i64, *const u8, u64, u64)>,
    flags: *mut u32,
    ctx: *const u8,
    transform: *const i64,
    item: *const u8,
    mode: u64,
    extra: u64,
) {
    // SAFETY: the original takes the arguments the game passed.
    unsafe { next.get()(flags, ctx, transform, item, mode, extra) };
    if flags.is_null() || ctx.is_null() || transform.is_null() || item.is_null() {
        return;
    }
    // SAFETY: the cursor's item definition.
    let altar = unsafe { read_at::<u8>(item, ITEM_ALTAR) } != 0;
    // SAFETY: the cursor's flags word.
    if !altar || unsafe { flags.read() } & MAX_ALTARS_REACHED != 0 {
        return;
    }
    let (Some(engine), Some(known)) = (ENGINE.get(), ZONES.get()) else {
        return;
    };
    let zones = known.load();
    if zones.bases < MAX_BASES {
        return;
    }
    // SAFETY: the cursor context holds the balancing registry there.
    let balancing = unsafe { read_at::<u64>(ctx, engine.cursor_balancing) };
    if balancing == 0 {
        return;
    }
    let mut out = [0f32; 4];
    // SAFETY: the balancing registry; `out` has room for three floats and
    // the valid byte.
    unsafe { (engine.zone_size)(out.as_mut_ptr(), balancing, 1) };
    // SAFETY: the valid byte at 0xc of `out`.
    if unsafe { read_at::<u8>(out.as_ptr().cast(), 0xc) } == 0 {
        return;
    }
    // SAFETY: the cursor's transform: the position first, three i64.
    let position = unsafe { transform.cast::<[i64; 3]>().read_unaligned() };
    let candidate = limit::candidate(
        position.map(|v| v as f64 / FIXED_PER_METRE),
        [out[0], out[1], out[2]],
    );
    if zones.refuses(&candidate) {
        // SAFETY: as above.
        let refused = unsafe { flags.read() } | MAX_ALTARS_REACHED;
        // SAFETY: as above.
        unsafe { flags.write(refused) };
    }
}
