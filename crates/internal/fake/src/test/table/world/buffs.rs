//! The fake's buff entries: the buff types and what each holder holds.

use crate::test::table::{get_headed, list, with};
use enshwick_core::abi::{ActiveBuff, BuffRequest, BuffTypeInfo, Header, ModHandle, Status};

pub(crate) extern "system" fn buff_types(
    _me: ModHandle,
    out: *mut BuffTypeInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let types = with(Vec::new(), |s| s.buff_types.clone());
    list(&types, out, entry_size, capacity, count)
}

pub(crate) extern "system" fn buffs(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    out: *mut ActiveBuff,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    match with(None::<Vec<ActiveBuff>>, |s| s.buffs.get(&entity.0).cloned()) {
        Some(on) => list(&on, out, entry_size, capacity, count),
        None => Status::MISSING,
    }
}

pub(crate) extern "system" fn apply_buff(_me: ModHandle, request: *const BuffRequest) -> Status {
    let Some(request) = get_headed(request) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        let Some(ty) = s.buff_types.iter().find(|ty| ty.id == request.id).copied() else {
            return Status::BAD_ARGS;
        };
        let Some(on) = s.buffs.get_mut(&request.entity.0) else {
            return Status::MISSING;
        };
        let duration = if request.duration_ns == 0 {
            ty.duration_ns
        } else {
            request.duration_ns
        };
        if let Some(buff) = on.iter_mut().find(|buff| buff.id == request.id) {
            buff.duration_ns = duration;
            buff.remaining_ns = duration;
        } else {
            on.push(ActiveBuff {
                header: Header::of::<ActiveBuff>(),
                id: request.id,
                entity: enshwick_core::Entity(0x8000 + u32::try_from(on.len()).unwrap_or(0)),
                slot: u32::from(ty.slot),
                level: request.level,
                duration_ns: duration,
                remaining_ns: duration,
            });
        }
        Status::OK
    })
}

pub(crate) extern "system" fn remove_buff(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    id: enshwick_core::buffs::BuffTypeId,
    removed: *mut u8,
) -> Status {
    if removed.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        let Some(on) = s.buffs.get_mut(&entity.0) else {
            return Status::MISSING;
        };
        let before = on.len();
        on.retain(|buff| buff.id != id);
        // SAFETY: `removed` is non-null, checked at the top; the safe layer
        // passes it pointing at a writable byte for the call.
        unsafe { removed.write(u8::from(on.len() != before)) };
        Status::OK
    })
}
