//! Host functions for the inventory domain: inventory requests, items, item
//! data, the rates mods declare, survivors, stats, affixes, damage, drops,
//! spawns and teleports.

use enshwick_core::abi::{DropRequest, ItemFilterFn, RateId, Status, Str};
use enshwick_core::{Entity, abi as api_frame};

use crate::engine::conflicts::Mode;
use crate::host::entries::base::services::exported;
use crate::host::layer::{
    AnyMod, Asker, BytesOut, Callback, Caller, Headed, HeadedOut, HeadedSince, List, ListOut,
    MaybeHeaded, Mod, Out, Text, entries,
};

/// Where every inventory request's required fields end: its op count.
const SUBMIT_REQUIRED: usize = core::mem::offset_of!(api_frame::InventoryRequest, op_count) + 4;

entries! {
    fn drop_items(loader as Caller, request as Headed<DropRequest>, out as Out<Entity>) -> Status {
        // `out` is checked before the drop: after it, a mod told BAD_ARGS
        // would drop again.
        out.put(Entity::from_engine(crate::inventory::drops::drop_items(loader, &request)?))
    }

    fn survivor_lookup(
        me as Mod,
        callback as Callback<api_frame::SurvivorLookupFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "survivor lookup",
            Mode::FirstAnswers,
            crate::inventory::survivors::register_lookup(me.loader, me.index, callback, user),
        )
    }

    fn survivor_bed_check(
        me as Mod,
        callback as Callback<api_frame::BedCheckFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "bed check",
            Mode::Chained,
            crate::inventory::survivors::register_bed_check(me.loader, me.index, callback, user),
        )
    }

    fn summon_filter(
        me as Mod,
        callback as Callback<api_frame::SummonFilterFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "summon filter",
            Mode::Chained,
            crate::inventory::survivors::register_summon(me.loader, me.index, callback, user),
        )
    }

    fn summoned(me as Mod, callback as Callback<api_frame::SummonFilterFn>, user: *mut core::ffi::c_void) -> Status {
        Ok(crate::inventory::survivors::register_summoned(me.loader, me.index, callback, user)?)
    }

    fn define_stat(
        me as Mod,
        id as Text,
        def as MaybeHeaded<api_frame::StatSpec>,
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
        def as HeadedOut<api_frame::StatSpec>,
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
        [modifiers, count] as List<api_frame::StatModifier>,
    ) -> Status {
        crate::inventory::stats::buff(buff, modifiers)?;
        Ok(())
    }

    fn stats_changed(
        me as Mod,
        callback as Callback<api_frame::StatChangeFn>,
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

    fn stat_watch(me as Mod, entity: Entity, watch: u8) -> Status {
        crate::inventory::stats::watch(me.index, entity, watch != 0)?;
        Ok(())
    }

    fn define_affix(
        me as Mod,
        id as Text,
        def as Headed<api_frame::AffixSpec>,
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
        out.put(&crate::inventory::stats::item_lines(pide.get(), item.get())?)
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
        Ok(crate::inventory::survivors::register_item(me.loader, item.get(), survivor.get())?)
    }

    fn survivor_status(
        me as Mod,
        callback as Callback<api_frame::SurvivorStatusFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "survivor status",
            Mode::FirstAnswers,
            crate::inventory::survivors::register_status(me.loader, me.index, callback, user),
        )
    }

    fn damage_filter(
        me as Mod,
        callback as Callback<enshwick_core::abi::DamageFilterFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "damage filter",
            Mode::Chained,
            crate::inventory::damage::register(me.loader, me.index, callback, user),
        )
    }

    fn damage_causes_off(_me as AnyMod, [out, capacity, count] as ListOut<u32>) -> Status {
        let off = crate::inventory::damage::causes_off().ok_or(Status::UNAVAILABLE)?;
        out.put(&off)
    }

    fn request_inventory(loader as Caller) -> Status {
        Ok(crate::inventory::transactions::open(loader)?)
    }

    fn inventory_submit(
        me as Mod,
        request as HeadedSince<api_frame::InventoryRequest, SUBMIT_REQUIRED>,
        done as Callback<api_frame::InventoryResultFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        Ok(crate::inventory::transactions::submit(me.loader, me.index, &request, done, user)?)
    }

    fn item_filter(me as Mod, callback as Callback<ItemFilterFn>, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "item filter",
            Mode::Chained,
            crate::inventory::item_actions::register(me.loader, me.index, callback, user),
        )
    }

    fn container_rule(
        me as Mod,
        item: enshwick_core::ItemId,
        callback as Callback<api_frame::ContainerRuleFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        if item.get() == 0 {
            return Err(Status::BAD_ARGS.into());
        }
        me.claim(
            "container rule",
            Mode::Chained,
            crate::inventory::containers::register(me.loader, me.index, item, callback, user),
        )
    }

    fn show_pickup(loader as Caller, character: Entity, item: enshwick_core::ItemId, count: u32) -> Status {
        if item.get() == 0 || count == 0 {
            return Err(Status::BAD_ARGS.into());
        }
        crate::inventory::containers::show_pickup(loader, character, item, count)?;
        Ok(())
    }


    fn emit(me as Asker, type_hash: u32, [data, size] as List<u8>) -> Status {
        crate::base::emits::emit(me.loader, me.handle, type_hash, data)?;
        Ok(())
    }

    fn teleport(loader as Caller, request as Headed<enshwick_core::abi::TeleportRequest>) -> Status {
        crate::base::emits::teleport(loader, &request, |position| {
            crate::world::scene::admit_teleport(loader, position)
        })?;
        Ok(())
    }


    fn define_item_data(me as Mod, id as Text, out as Out<u32>) -> Status {
        out.put(crate::inventory::item_data::open(me.loader, me.index, id)?)
    }

    fn define_entity_data(
        me as Mod,
        id as Text,
        flags: api_frame::EntityDataFlags,
        out as Out<u32>,
    ) -> Status {
        let sent = flags.0 & api_frame::EntityDataFlags::SENT.0 != 0;
        out.put(crate::inventory::item_data::open_entity(me.loader, me.index, id, sent)?)
    }

    fn item_data_get(
        _loader as Caller,
        kind: u32,
        pide: Entity,
        item: enshwick_core::ItemId,
        [out, capacity, len] as BytesOut,
    ) -> Status {
        out.put(&crate::inventory::item_data::get(kind, pide.get(), item.get())?)
    }

    fn set_item_data(
        _loader as Caller,
        kind: u32,
        pide: Entity,
        item: enshwick_core::ItemId,
        [data, len] as List<u8>,
    ) -> Status {
        crate::inventory::item_data::set(kind, pide.get(), item.get(), data)?;
        Ok(())
    }

    fn rate(me as Mod, rate: RateId) -> Status {
        let links = crate::content::rates::links(rate);
        let available = links.iter().all(|&symbol| me.loader.address(symbol).is_some())
            // The loot rate reads stack sizes from the item registry, which
            // the inventory link finds.
            && (rate != RateId::LOOT || crate::inventory::transactions::open(me.loader).is_ok());
        Ok(crate::content::rates::RATES.declare(
            me.index,
            rate,
            available,
            me.record.joins_when_on(),
        )?)
    }

    fn item_info(
        loader as Caller,
        item: enshwick_core::ItemId,
        out as HeadedOut<api_frame::ItemTypeInfo>,
    ) -> Status {
        out.put(crate::inventory::transactions::item_info(loader, item.get())?)
    }
}
