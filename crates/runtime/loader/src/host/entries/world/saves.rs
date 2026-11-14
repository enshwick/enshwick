//! Host functions for the world's saves.

use enshwick_core::Entity;
use enshwick_core::abi::{Header, SaveInfo, Status};

use crate::host::layer::{Caller, HeadedOut, Mod, checked, entries};

entries! {
    fn save_info(loader as Caller, out as HeadedOut<SaveInfo>) -> Status {
        let info = crate::base::saves::info(loader)?.ok_or(Status::MISSING)?;
        out.put(SaveInfo {
            header: Header::of::<SaveInfo>(),
            world: info.world,
            saved_at: u64::from(info.saved_at),
            size: info.size,
            backups: u32::from(info.backups),
            reserved: 0,
        })
    }

    fn hold_saves(me as Mod, held: u8) -> Status {
        if held > 1 {
            return Err(Status::BAD_ARGS.into());
        }
        checked(crate::base::saves::hold(me.loader, me.index, held == 1))
    }

    fn set_saved(loader as Caller, entity: Entity, saved: u8) -> Status {
        crate::world::entities::set_saved(loader, entity, saved != 0)?;
        Ok(())
    }

    fn server_stop(me as Mod) -> Status {
        checked(crate::base::stop::request(me.loader, &me.record.id))
    }
}
