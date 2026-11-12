//! Host functions for buffs.

use crate::host::memory::{put, put_headed_list};
use crate::host::{caller, entry};
use enshwick_core::Entity;
use enshwick_core::abi::{ModHandle, Status};

pub(crate) extern "system" fn buff_types(
    me: ModHandle,
    out: *mut enshwick_core::abi::BuffTypeInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("buff_types", || {
        match caller(me).and_then(crate::world::buffs::types) {
            // SAFETY: `HostApi::buff_types` has the mod pass `out` null or
            // pointing at `capacity` entries of its version of `BuffTypeInfo`,
            // each writable for the size the first entry's header states, and
            // `count` null or pointing at a writable, aligned `u32`, neither
            // touched by another thread during the call.
            Ok(types) => unsafe { put_headed_list(&types, out, capacity, count) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn buffs(
    me: ModHandle,
    entity: Entity,
    out: *mut enshwick_core::abi::ActiveBuff,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("buffs", || {
        match caller(me).and_then(|loader| crate::world::buffs::of(loader, entity)) {
            // SAFETY: `HostApi::buffs` has the mod pass `out` null or pointing
            // at `capacity` entries of its version of `ActiveBuff`, each
            // writable for the size the first entry's header states, and
            // `count` null or pointing at a writable, aligned `u32`, neither
            // touched by another thread during the call.
            Ok(list) => unsafe { put_headed_list(&list, out, capacity, count) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn apply_buff(
    me: ModHandle,
    request: *const enshwick_core::abi::BuffRequest,
) -> Status {
    entry("apply_buff", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe {
            enshwick_core::abi::read_headed(
                request,
                <enshwick_core::abi::BuffRequest as enshwick_core::abi::Headed>::BORN,
            )
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::world::buffs::apply(loader, &request)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn remove_buff(
    me: ModHandle,
    entity: Entity,
    id: enshwick_core::abi::BuffTypeId,
    removed: *mut u8,
) -> Status {
    entry("remove_buff", || {
        match caller(me).and_then(|loader| crate::world::buffs::remove(loader, entity, id.get())) {
            // SAFETY: `HostApi::remove_buff` has the mod pass `removed` null or
            // pointing at a writable, aligned `u8` that no other thread touches
            // during the call.
            Ok(had) => unsafe { put(removed, u8::from(had)) },
            Err(status) => status,
        }
    })
}
