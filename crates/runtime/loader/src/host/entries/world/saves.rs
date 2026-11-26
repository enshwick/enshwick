//! Host functions for the world's saves.

use enshwick_core::Entity;
use enshwick_core::abi::{SaveInfo, Status};

use crate::host::layer::{Caller, HeadedOut, Mod, YesNo, entries};

entries! {
    fn save_info(loader as Caller, out as HeadedOut<SaveInfo>) -> Status {
        let info = crate::base::saves::info(loader)?.ok_or(Status::MISSING)?;
        out.put(enshwick_abi::headed!(SaveInfo {
            world: enshwick_core::content::Guid(info.world),
            saved_at: u64::from(info.saved_at),
            size: info.size,
            backups: u32::from(info.backups),
            reserved: 0,
        }))
    }

    fn hold_saves(me as Mod, held as YesNo) -> Status {
        Ok(crate::base::saves::hold(me.loader, me.index, held)?)
    }

    fn set_saved(loader as Caller, entity: Entity, saved: u8) -> Status {
        crate::world::entities::set_saved(loader, entity, saved != 0)?;
        Ok(())
    }

    fn server_stop(me as Mod) -> Status {
        Ok(crate::base::stop::request(me.loader, &me.record.id)?)
    }
}
