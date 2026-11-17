//! Host functions for buffs.

use enshwick_core::Entity;
use enshwick_core::abi::{ActiveBuff, BuffRequest, BuffTypeAdd, BuffTypeId, BuffTypeInfo, Status};

use crate::host::layer::guards::declared_content;
use crate::host::layer::{Caller, Headed, Mod, Out, SizedListOut, entries};

entries! {
    fn buff_types(loader as Caller, [out, entry_size, capacity, count] as SizedListOut<BuffTypeInfo>) -> Status {
        out.put(&crate::world::buffs::types(loader)?)
    }

    fn buffs(
        loader as Caller,
        entity: Entity,
        [out, entry_size, capacity, count] as SizedListOut<ActiveBuff>,
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

    fn buff_type_add(me as Mod, buff as Headed<BuffTypeAdd>, id as Out<BuffTypeId>) -> Status {
        let owner = (me.index, me.record.joins_when_on());
        crate::content::buff_types::admits(owner.1)?;
        declared_content(me.record, buff.guid)?;
        id.put(crate::content::buff_types::add(me.loader, owner, &buff)?)
    }
}
