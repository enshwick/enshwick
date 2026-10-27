use std::sync::OnceLock;

use enshwick::Entity;
use enshwick::abi::{Header, Summon};
use enshwick::altars::cell_of_fixed;
use enshwick::raw::{read_at, write_at};

use crate::host::Loader;

use super::{LOADED, SEALED, SPAWN_NEXT, SURVIVORS_FUSE, layout};

pub(super) unsafe extern "system" fn spawn_link(ctx: *mut u64, request: *mut u8) -> u8 {
    // SAFETY: the game passes the spawn its non-null (checked first) live
    // request, which the spawn reads only after this link returns.
    let asked = (!request.is_null())
        .then(|| SURVIVORS_FUSE.run("a summon", || unsafe { summon(request) }))
        .flatten()
        .flatten();
    let call = match asked {
        // A refused use spawns, moves and takes nothing, as a spawn the game
        // turns down itself.
        Some(Asked::Refused) => return 0,
        Some(Asked::Use(call)) => Some(call),
        None => None,
    };
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the spawn's signature.
    let done = unsafe { SPAWN_NEXT.get()(ctx, request) };
    if done != 0
        && let Some(call) = call
    {
        let _done = SURVIVORS_FUSE.run("a summon's answer", || summoned(call));
    }
    done
}

/// What the summon filters made of a use.
pub(super) enum Asked {
    /// The use goes on.
    Use(Summon),
    /// A filter refused it.
    Refused,
}

/// The `NetworkFuel` type hash, a used item's meter as clients see it.
pub(super) static FUEL_TYPE: OnceLock<Option<u32>> = OnceLock::new();

/// The meter of the item entity `pide`, `[current, max]`, or zeros.
pub(super) fn meter(loader: &Loader, pide: u32) -> [u16; 2] {
    let ty = FUEL_TYPE.get_or_init(|| {
        let info = loader.reflection.type_info("keen::ecs::NetworkFuel").ok()?;
        crate::world::type_hash(info.va())
    });
    let Some(data) = ty.and_then(|ty| crate::world::component(loader, Entity(pide), ty).ok())
    else {
        return [0; 2];
    };
    // SAFETY: `world::component` returned the item's live `NetworkFuel`, two
    // `u16` that make one four-byte word. Its one writer, the `fuel_network`
    // system, may run on another task worker during this spawn (server 399)
    // and writes the word whole, so the atomic load gives the meter from
    // before or after that write.
    let Some(both) = (unsafe { enshwick::raw::read_u32_atomic_at(data.cast(), 0) }) else {
        return [0; 2];
    };
    let [a, b, c, d] = both.to_le_bytes();
    [u16::from_le_bytes([a, b]), u16::from_le_bytes([c, d])]
}

/// Asks the summon filters and marks the request consuming when one says
/// so; returns the use, for the handlers after a spawn that worked, or that
/// a filter refused it.
///
/// # Safety
///
/// - `request` is the game's live spawn request, with its fields at the
///   table's `survivors.spawn_*` offsets.
/// - The call runs before the spawn reads the request, on its thread, so
///   nothing else reads or writes it during the call.
pub(super) unsafe fn summon(request: *mut u8) -> Option<Asked> {
    let (Some(loader), Some(sealed)) = (LOADED.get(), SEALED.get()) else {
        return None;
    };
    let layout = layout(loader)?;
    let o = &layout.offsets;
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the used item at the table's `survivors.spawn_used_item`.
    let item = unsafe { read_at(request, o.spawn_used_item) };
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the ammo item at the table's `survivors.spawn_ammo_item`.
    let ammo = unsafe { read_at(request, o.spawn_ammo_item) };
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the template at the table's `survivors.spawn_template`.
    let template = unsafe { read_at(request, o.spawn_template) };
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps its 32.32 fixed-point target at the table's
    // `survivors.spawn_target`.
    let target = cell_of_fixed(unsafe { read_at(request, o.spawn_target) });
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the used item's `ItemInstanceInfo` at the table's
    // `survivors.spawn_used_slot`.
    let slot = unsafe { read_at(request, o.spawn_used_slot) };
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the used item's `u32` entity at the table's
    // `survivors.spawn_used_pide`.
    let pide: u32 = unsafe { read_at(request, o.spawn_used_pide) };
    let mut call = Summon {
        header: Header::of::<Summon>(),
        item,
        ammo,
        template,
        target,
        slot,
        pide: Entity(pide),
        meter: if pide == 0 {
            [0; 2]
        } else {
            meter(loader, pide)
        },
        consumed: 0,
        reserved: [0; 3],
    };
    let mut consume = false;
    for r in sealed
        .summons
        .iter()
        .filter(|r| !crate::host::is_off(r.owner))
    {
        let user = core::ptr::with_exposed_provenance_mut(r.user);
        // SAFETY: the registered `SummonFn` lives in the mod's DLL, which
        // stays loaded for the process, and takes the `user` word it
        // registered; `call` outlives the call.
        match crate::profile::call(r.owner, crate::profile::Feature::Survivors, || unsafe {
            (r.callback)(&raw const call, user)
        }) {
            Summon::REFUSE => return Some(Asked::Refused),
            0 => {}
            _ => consume = true,
        }
    }
    if consume {
        // SAFETY: the caller's contract makes `request` the live spawn
        // request, untouched by others during the call, which keeps its
        // one-byte consume flag at the table's `survivors.spawn_consume`.
        unsafe { write_at(request, o.spawn_consume, 1u8) };
    }
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps its one-byte consume flag at the table's
    // `survivors.spawn_consume`; a filter of the game's may have set it too.
    call.consumed = unsafe { read_at(request, o.spawn_consume) };
    Some(Asked::Use(call))
}

/// Tells the handlers that a use spawned or moved a survivor.
pub(super) fn summoned(call: Summon) {
    let Some(sealed) = SEALED.get() else {
        return;
    };
    for r in sealed
        .summoned
        .iter()
        .filter(|r| !crate::host::is_off(r.owner))
    {
        let user = core::ptr::with_exposed_provenance_mut(r.user);
        // SAFETY: the registered `SummonFn` lives in the mod's DLL, which
        // stays loaded for the process, and takes the `user` word it
        // registered; `call` outlives the call.
        crate::profile::call(r.owner, crate::profile::Feature::Survivors, || unsafe {
            (r.callback)(&raw const call, user)
        });
    }
}
