//! Host functions for the inventory domain: inventory requests, items,
//! survivors, stats, affixes, damage, drops, spawns and teleports.

use enshwick_core::abi::{DropRequest, Headed, ItemActionFn, Level, ModHandle, Status, Str};
use enshwick_core::{Entity, abi as api_frame};

use crate::host::entries::base::services::unexported;
use crate::host::memory::{get_headed, put, put_headed, put_list, text};
use crate::host::{caller, caller_index, calling, entry};

pub(crate) extern "system" fn drop_items(
    me: ModHandle,
    request: *const DropRequest,
    out: *mut Entity,
) -> Status {
    entry("drop_items", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) =
            (unsafe { enshwick_core::abi::read_headed(request, DropRequest::BORN) })
        else {
            return Status::BAD_ARGS;
        };
        // Refused before the drop: after it, a mod told BAD_ARGS would drop again.
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        match caller(me).and_then(|loader| crate::inventory::drops::drop_items(loader, &request)) {
            // SAFETY: `out` is non-null (checked above) and `HostApi::drop_items`
            // has the mod pass a writable, aligned `Entity` that no other
            // thread touches during the call.
            Ok(entity) => unsafe { put(out, Entity(entity)) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn survivor_lookup(
    me: ModHandle,
    callback: api_frame::SurvivorLookupFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("survivor_lookup", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "survivor lookup",
            index,
            crate::engine::conflicts::Mode::Exclusive,
            crate::inventory::survivors::register_lookup(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn survivor_bed_check(
    me: ModHandle,
    callback: api_frame::BedCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("survivor_bed_check", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "bed check",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::inventory::survivors::register_bed_check(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn summon_filter(
    me: ModHandle,
    callback: api_frame::SummonFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("summon_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "summon filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::inventory::survivors::register_summon(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn summoned(
    me: ModHandle,
    callback: api_frame::SummonFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("summoned", || match caller_index(me) {
        Ok((loader, index)) => {
            crate::inventory::survivors::register_summoned(loader, index, callback, user)
        }
        Err(status) => status,
    })
}

pub(crate) extern "system" fn stat_open(
    me: ModHandle,
    id: Str,
    def: *const api_frame::StatDefData,
    out: *mut api_frame::StatRef,
) -> Status {
    entry("stat_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        let def = if def.is_null() {
            None
        } else {
            // SAFETY: a mod passes its version of the definition, and all of it
            // is required.
            let Some(data) = (unsafe { get_headed(def, <api_frame::StatDefData as Headed>::BORN) })
            else {
                return Status::BAD_ARGS;
            };
            Some(data)
        };
        let def = match def {
            Some(data) => match enshwick_core::stats::StatDef::from_data(&data) {
                Some(def) => Some(def),
                None => return Status::BAD_ARGS,
            },
            None => None,
        };
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if def.is_none()
            && let Some(reason) = unexported(loader, record, crate::base::manifest::Kind::Stat, id)
        {
            crate::engine::log::write(Level::WARN, &record.id, &reason);
            return Status::REFUSED;
        }
        match crate::inventory::stats::open(loader, index, id, def) {
            // SAFETY: `HostApi::stat_open` has the mod pass `out` null or
            // pointing at a writable, aligned `StatRef` that no other thread
            // touches during the call.
            Ok(stat) => unsafe { put(out, stat) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn stat_info(
    me: ModHandle,
    stat: u32,
    def: *mut api_frame::StatDefData,
    id: *mut Str,
) -> Status {
    entry("stat_info", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::inventory::stats::info(stat) {
            Ok((data, name)) => {
                // SAFETY: `HostApi::stat_info` has the mod pass `def` null or
                // pointing at its version of `StatDefData`, writable for the
                // size its header states, that no other thread touches during
                // the call.
                let status = unsafe { put_headed(def, data) };
                if status.is_ok() {
                    // SAFETY: `HostApi::stat_info` has the mod pass `id` null
                    // or pointing at a writable, aligned `Str` that no other
                    // thread touches during the call.
                    unsafe { put(id, name) }
                } else {
                    status
                }
            }
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn stat_value(
    me: ModHandle,
    entity: Entity,
    stat: u32,
    out: *mut f32,
) -> Status {
    entry("stat_value", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::inventory::stats::value(entity, stat) {
            // SAFETY: `HostApi::stat_value` has the mod pass `out` null or
            // pointing at a writable, aligned `f32` that no other thread
            // touches during the call.
            Ok(value) => unsafe { put(out, value) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn stat_modify(
    me: ModHandle,
    request: *const api_frame::StatModify,
) -> Status {
    entry("stat_modify", || {
        // SAFETY: a mod passes null or its version of the request, and all of
        // it is required.
        let Some(request) =
            (unsafe { get_headed(request, <api_frame::StatModify as Headed>::BORN) })
        else {
            return Status::BAD_ARGS;
        };
        match caller_index(me)
            .and_then(|(loader, index)| crate::inventory::stats::modify(loader, index, &request))
        {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn stat_buff(
    me: ModHandle,
    buff: u32,
    modifiers: *const api_frame::StatModifierData,
    count: u32,
) -> Status {
    entry("stat_buff", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let list = if count == 0 {
            &[][..]
        } else if modifiers.is_null() {
            return Status::BAD_ARGS;
        } else {
            // SAFETY: the caller passes `count` modifiers for the call.
            unsafe { core::slice::from_raw_parts(modifiers, count as usize) }
        };
        match crate::inventory::stats::buff(buff, list) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn stats_changed(
    me: ModHandle,
    callback: api_frame::StatChangeFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("stats_changed", || match caller_index(me) {
        Ok((_, index)) => match crate::inventory::stats::on_changed(index, callback, user) {
            Ok(()) => Status::OK,
            Err(status) => status,
        },
        Err(status) => status,
    })
}

pub(crate) extern "system" fn stat_sources(
    me: ModHandle,
    entity: Entity,
    stat: u32,
    out: *mut api_frame::StatSource,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("stat_sources", || {
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        match crate::inventory::stats::sources(loader, entity, stat) {
            // SAFETY: `HostApi::stat_sources` has the mod pass `out` null or
            // pointing at `capacity` writable, aligned `StatSource`s, and
            // `count` null or pointing at a writable, aligned `u32`, neither
            // touched by another thread during the call.
            Ok(list) => unsafe { put_list(&list, out, capacity, count) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn stat_watch(me: ModHandle, entity: Entity, watch: u8) -> Status {
    entry("stat_watch", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::inventory::stats::watch(entity, watch != 0) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn affix_open(
    me: ModHandle,
    id: Str,
    def: *const api_frame::AffixDefData,
    out: *mut api_frame::StatRef,
) -> Status {
    entry("affix_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: a mod passes null or its version of the definition, and all
        // of it is required.
        let Some(def) = (unsafe { get_headed(def, <api_frame::AffixDefData as Headed>::BORN) })
        else {
            return Status::BAD_ARGS;
        };
        match caller_index(me).and_then(|(loader, index)| {
            crate::inventory::stats::open_affix(loader, index, id, &def)
        }) {
            // SAFETY: `HostApi::affix_open` has the mod pass `out` null or
            // pointing at a writable, aligned `StatRef` that no other thread
            // touches during the call.
            Ok(affix) => unsafe { put(out, affix) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn affix_roll(
    me: ModHandle,
    roll: *const api_frame::AffixRoll,
    count: *mut u32,
) -> Status {
    entry("affix_roll", || {
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        // SAFETY: a mod passes null or its version of the request, and all of
        // it is required.
        let Some(roll) = (unsafe { get_headed(roll, <api_frame::AffixRoll as Headed>::BORN) })
        else {
            return Status::BAD_ARGS;
        };
        match crate::inventory::stats::roll(loader, &roll) {
            // SAFETY: `HostApi::affix_roll` has the mod pass `count` null or
            // pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(rolled) => unsafe { put(count, rolled) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn item_lines(
    me: ModHandle,
    pide: Entity,
    item: enshwick_core::ItemId,
    out: *mut api_frame::ItemLine,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("item_lines", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::inventory::stats::item_lines(pide.0, item.get()) {
            // SAFETY: `HostApi::item_lines` has the mod pass `out` null or
            // pointing at `capacity` writable, aligned `ItemLine`s, and `count`
            // null or pointing at a writable, aligned `u32`, neither touched by
            // another thread during the call.
            Ok(lines) => unsafe { put_list(&lines, out, capacity, count) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn affix_info(
    me: ModHandle,
    affix: u32,
    text: *mut Str,
    rarity: *mut u8,
) -> Status {
    entry("affix_info", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::inventory::stats::affix_info(affix) {
            Ok((name, rare)) => {
                // SAFETY: `HostApi::affix_info` has the mod pass `text` null or
                // pointing at a writable, aligned `Str` that no other thread
                // touches during the call.
                let status = unsafe { put(text, name) };
                if status.is_ok() {
                    // SAFETY: `HostApi::affix_info` has the mod pass `rarity`
                    // null or pointing at a writable, aligned `u8` that no
                    // other thread touches during the call.
                    unsafe { put(rarity, rare) }
                } else {
                    status
                }
            }
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn summon_item(
    me: ModHandle,
    item: enshwick_core::ItemId,
    survivor: enshwick_core::abi::NpcId,
) -> Status {
    entry("summon_item", || match caller_index(me) {
        Ok((loader, _)) => {
            crate::inventory::survivors::register_item(loader, item.get(), survivor.get())
        }
        Err(status) => status,
    })
}

pub(crate) extern "system" fn survivor_status(
    me: ModHandle,
    callback: api_frame::SurvivorStatusFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("survivor_status", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "survivor status",
            index,
            crate::engine::conflicts::Mode::Exclusive,
            crate::inventory::survivors::register_status(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn damage_filter(
    me: ModHandle,
    callback: enshwick_core::abi::DamageFilterFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("damage_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "damage filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::inventory::damage::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn inventory_open(me: ModHandle) -> Status {
    entry("inventory_open", || match caller(me) {
        Ok(loader) => crate::inventory::transactions::open(loader),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn inventory_submit(
    me: ModHandle,
    request: *const api_frame::InventoryRequest,
    done: api_frame::InventoryResultFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("inventory_submit", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let required = core::mem::offset_of!(api_frame::InventoryRequest, op_count) + 4;
        // SAFETY: a non-null request from a mod points at a valid request for the
        // call; an all-zero request is valid.
        let Some(request) = (unsafe { enshwick_core::abi::read_headed(request, required) }) else {
            return Status::BAD_ARGS;
        };
        crate::inventory::transactions::submit(loader, index, &request, done, user)
    })
}

pub(crate) extern "system" fn item_filter(
    me: ModHandle,
    callback: ItemActionFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("item_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "item filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::inventory::item_actions::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn emit(
    me: ModHandle,
    type_hash: u32,
    data: *const u8,
    size: u32,
) -> Status {
    entry("emit", || {
        if data.is_null() {
            return Status::BAD_ARGS;
        }
        let Ok(len) = usize::try_from(size) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the caller passes `size` bytes at `data`, valid for the call.
        let bytes = unsafe { core::slice::from_raw_parts(data, len) };
        match caller(me).and_then(|loader| crate::base::emits::emit(loader, type_hash, bytes)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn teleport(
    me: ModHandle,
    request: *const enshwick_core::abi::TeleportRequest,
) -> Status {
    entry("teleport", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe {
            enshwick_core::abi::read_headed(
                request,
                <enshwick_core::abi::TeleportRequest as Headed>::BORN,
            )
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::base::emits::teleport(loader, &request)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn spawn(
    me: ModHandle,
    request: *const enshwick_core::abi::SpawnRequest,
    out: *mut Entity,
) -> Status {
    entry("spawn", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe {
            enshwick_core::abi::read_headed(
                request,
                <enshwick_core::abi::SpawnRequest as Headed>::BORN,
            )
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::inventory::spawns::spawn(loader, &request)) {
            // SAFETY: `HostApi::spawn` has the mod pass `out` null or pointing
            // at a writable, aligned `Entity` that no other thread touches during
            // the call.
            Ok(entity) => unsafe { put(out, Entity(entity)) },
            Err(status) => status,
        }
    })
}
