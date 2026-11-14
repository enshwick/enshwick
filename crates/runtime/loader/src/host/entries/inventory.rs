//! Host functions for the inventory domain: inventory requests, items,
//! survivors, stats, affixes, damage, drops, spawns and teleports.

use enshwick_core::abi::{DropRequest, ItemActionFn, Status, Str};
use enshwick_core::{Entity, abi as api_frame};

use crate::engine::conflicts::Mode;
use crate::host::entries::base::services::exported;
use crate::host::layer::{
    Caller, Headed, HeadedOut, HeadedSince, List, ListOut, MaybeHeaded, Mod, Out, Text, checked,
    entries,
};

/// Where every inventory request's required fields end: its op count.
const SUBMIT_REQUIRED: usize = core::mem::offset_of!(api_frame::InventoryRequest, op_count) + 4;

entries! {
    fn drop_items(loader as Caller, request as Headed<DropRequest>, out as Out<Entity>) -> Status {
        // `out` is checked before the drop: after it, a mod told BAD_ARGS
        // would drop again.
        out.put(Entity(crate::inventory::drops::drop_items(loader, &request)?))
    }

    fn survivor_lookup(
        me as Mod,
        callback: api_frame::SurvivorLookupFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "survivor lookup",
            Mode::Exclusive,
            checked(crate::inventory::survivors::register_lookup(me.loader, me.index, callback, user)),
        )
    }

    fn survivor_bed_check(
        me as Mod,
        callback: api_frame::BedCheckFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "bed check",
            Mode::Chained,
            checked(crate::inventory::survivors::register_bed_check(
                me.loader, me.index, callback, user,
            )),
        )
    }

    fn summon_filter(
        me as Mod,
        callback: api_frame::SummonFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "summon filter",
            Mode::Chained,
            checked(crate::inventory::survivors::register_summon(me.loader, me.index, callback, user)),
        )
    }

    fn summoned(me as Mod, callback: api_frame::SummonFn, user: *mut core::ffi::c_void) -> Status {
        checked(crate::inventory::survivors::register_summoned(me.loader, me.index, callback, user))
    }

    fn stat_open(
        me as Mod,
        id as Text,
        def as MaybeHeaded<api_frame::StatDefData>,
        out as Out<api_frame::StatRef>,
    ) -> Status {
        let def = def
            .map(|data| enshwick_core::stats::StatDef::from_data(&data).ok_or(Status::BAD_ARGS))
            .transpose()?;
        if def.is_none() {
            exported(me.loader, me.record, crate::base::manifest::Kind::Stat, id)?;
        }
        out.put(crate::inventory::stats::open(me.loader, me.index, id, def)?)
    }

    fn stat_info(
        _loader as Caller,
        stat: u32,
        def as HeadedOut<api_frame::StatDefData>,
        id as Out<Str>,
    ) -> Status {
        let (data, name) = crate::inventory::stats::info(stat)?;
        def.put(data)?;
        id.put(name)
    }

    fn stat_value(_loader as Caller, entity: Entity, stat: u32, out as Out<f32>) -> Status {
        out.put(crate::inventory::stats::value(entity, stat)?)
    }

    fn stat_modify(me as Mod, request as Headed<api_frame::StatModify>) -> Status {
        crate::inventory::stats::modify(me.loader, me.index, &request)?;
        Ok(())
    }

    fn stat_buff(
        _loader as Caller,
        buff: u32,
        [modifiers, count] as List<api_frame::StatModifierData>,
    ) -> Status {
        crate::inventory::stats::buff(buff, modifiers)?;
        Ok(())
    }

    fn stats_changed(
        me as Mod,
        callback: api_frame::StatChangeFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        crate::inventory::stats::on_changed(me.index, callback, user)?;
        Ok(())
    }

    fn stat_sources(
        loader as Caller,
        entity: Entity,
        stat: u32,
        [out, capacity, count] as ListOut<api_frame::StatSource>,
    ) -> Status {
        out.put(&crate::inventory::stats::sources(loader, entity, stat)?)
    }

    fn stat_watch(_loader as Caller, entity: Entity, watch: u8) -> Status {
        crate::inventory::stats::watch(entity, watch != 0)?;
        Ok(())
    }

    fn affix_open(
        me as Mod,
        id as Text,
        def as Headed<api_frame::AffixDefData>,
        out as Out<api_frame::StatRef>,
    ) -> Status {
        out.put(crate::inventory::stats::open_affix(me.loader, me.index, id, &def)?)
    }

    fn affix_roll(loader as Caller, roll as Headed<api_frame::AffixRoll>, count as Out<u32>) -> Status {
        count.put(crate::inventory::stats::roll(loader, &roll)?)
    }

    fn item_lines(
        _loader as Caller,
        pide: Entity,
        item: enshwick_core::ItemId,
        [out, capacity, count] as ListOut<api_frame::ItemLine>,
    ) -> Status {
        out.put(&crate::inventory::stats::item_lines(pide.0, item.get())?)
    }

    fn affix_info(_loader as Caller, affix: u32, text as Out<Str>, rarity as Out<u8>) -> Status {
        let (name, rare) = crate::inventory::stats::affix_info(affix)?;
        text.put(name)?;
        rarity.put(rare)
    }

    fn summon_item(
        me as Mod,
        item: enshwick_core::ItemId,
        survivor: enshwick_core::abi::NpcId,
    ) -> Status {
        checked(crate::inventory::survivors::register_item(me.loader, item.get(), survivor.get()))
    }

    fn survivor_status(
        me as Mod,
        callback: api_frame::SurvivorStatusFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "survivor status",
            Mode::Exclusive,
            checked(crate::inventory::survivors::register_status(me.loader, me.index, callback, user)),
        )
    }

    fn damage_filter(
        me as Mod,
        callback: enshwick_core::abi::DamageFilterFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "damage filter",
            Mode::Chained,
            checked(crate::inventory::damage::register(me.loader, me.index, callback, user)),
        )
    }

    fn inventory_open(loader as Caller) -> Status {
        checked(crate::inventory::transactions::open(loader))
    }

    fn inventory_submit(
        me as Mod,
        request as HeadedSince<api_frame::InventoryRequest, SUBMIT_REQUIRED>,
        done: api_frame::InventoryResultFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        checked(crate::inventory::transactions::submit(me.loader, me.index, &request, done, user))
    }

    fn item_filter(me as Mod, callback: ItemActionFn, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "item filter",
            Mode::Chained,
            checked(crate::inventory::item_actions::register(me.loader, me.index, callback, user)),
        )
    }

    fn emit(loader as Caller, type_hash: u32, [data, size] as List<u8>) -> Status {
        crate::base::emits::emit(loader, type_hash, data)?;
        Ok(())
    }

    fn teleport(loader as Caller, request as Headed<enshwick_core::abi::TeleportRequest>) -> Status {
        crate::base::emits::teleport(loader, &request)?;
        Ok(())
    }

    fn spawn(
        loader as Caller,
        request as Headed<enshwick_core::abi::SpawnRequest>,
        out as Out<Entity>,
    ) -> Status {
        out.put(Entity(crate::inventory::spawns::spawn(loader, &request)?))
    }
}
