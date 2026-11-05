//! Host functions for the world's saves.

use enshwick::abi::{Header, ModHandle, SaveInfo, Status};

use crate::host::memory::put_headed;
use crate::host::{caller, caller_index, entry};

pub(crate) extern "system" fn save_info(me: ModHandle, out: *mut SaveInfo) -> Status {
    entry("save_info", || {
        let info = match caller(me).and_then(crate::saves::info) {
            Ok(Some(info)) => info,
            Ok(None) => return Status::MISSING,
            Err(status) => return status,
        };
        let value = SaveInfo {
            header: Header::of::<SaveInfo>(),
            world: info.world,
            saved_at: u64::from(info.saved_at),
            size: info.size,
            backups: u32::from(info.backups),
            reserved: 0,
        };
        // SAFETY: `HostApi::save_info` has the mod pass `out` null or pointing
        // at its version of `SaveInfo`, starting with its header and writable
        // for the size that header states, touched by no other thread during
        // the call.
        unsafe { put_headed(out, value) }
    })
}

pub(crate) extern "system" fn set_saved(
    me: ModHandle,
    entity: enshwick::Entity,
    saved: u8,
) -> Status {
    entry("set_saved", || {
        if saved > 1 {
            return Status::BAD_ARGS;
        }
        match caller(me).and_then(|loader| crate::entities::set_saved(loader, entity, saved == 1)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn hold_saves(me: ModHandle, held: u8) -> Status {
    entry("hold_saves", || {
        if held > 1 {
            return Status::BAD_ARGS;
        }
        match caller_index(me) {
            Ok((loader, index)) => crate::saves::hold(loader, index, held == 1),
            Err(status) => status,
        }
    })
}
