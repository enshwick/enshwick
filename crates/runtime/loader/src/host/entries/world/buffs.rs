//! Host functions for buffs.

use enshwick_core::Entity;
use enshwick_core::abi::{ActiveBuff, BuffRequest, BuffTypeId, BuffTypeInfo, Status};

use crate::host::layer::{Caller, Headed, HeadedListOut, Out, entries};

entries! {
    fn buff_types(loader as Caller, [out, capacity, count] as HeadedListOut<BuffTypeInfo>) -> Status {
        out.put(&crate::world::buffs::types(loader)?)
    }

    fn buffs(
        loader as Caller,
        entity: Entity,
        [out, capacity, count] as HeadedListOut<ActiveBuff>,
    ) -> Status {
        out.put(&crate::world::buffs::of(loader, entity)?)
    }

    fn apply_buff(loader as Caller, request as Headed<BuffRequest>) -> Status {
        crate::world::buffs::apply(loader, &request)?;
        Ok(())
    }

    fn remove_buff(loader as Caller, entity: Entity, id: BuffTypeId, removed as Out<u8>) -> Status {
        removed.put(u8::from(crate::world::buffs::remove(loader, entity, id.get())?))
    }
}
