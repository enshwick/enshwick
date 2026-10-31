//! Host functions for altars, survivors, stats, affixes, buffs, knowledge, game rates and the game's checks.

use enshwick::Entity;
use enshwick::abi as api_frame;
use enshwick::abi::{Level, ModHandle, RateId, Status, Str};

use crate::base_groups::limit::Limits;
use crate::host::entries::mods::list;
use crate::host::entries::services::unexported;
use crate::host::memory::{get_headed, put, put_headed, put_headed_list, put_list, text};
use crate::host::{caller, caller_index, calling, entry};

pub(crate) extern "system" fn altars_open(me: ModHandle) -> Status {
    entry("altars_open", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::altars::open(loader, index)
    })
}

pub(crate) extern "system" fn altars(
    me: ModHandle,
    out: *mut api_frame::AltarInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("altars", || {
        let (_, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::altars::opened(index) {
            return Status::REFUSED;
        }
        let altars = crate::altars::list();
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { list(&altars, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn altar_zone_filter(
    me: ModHandle,
    callback: api_frame::AltarZoneFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("altar_zone_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "altar zone filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::altars::add_sizer(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn altar_zones_changed(me: ModHandle) -> Status {
    entry("altar_zones_changed", || match caller_index(me) {
        Ok((_, index)) => crate::altars::resize(index),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn building_zone_check_filter(
    me: ModHandle,
    callback: api_frame::BuildingZoneCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("building_zone_check_filter", || {
        let (_, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::conflicts::note(
            "altar zone check",
            index,
            crate::conflicts::Mode::Chained,
            crate::altars::register(index, callback, user),
        )
    })
}

pub(crate) extern "system" fn survivor_lookup(
    me: ModHandle,
    callback: api_frame::SurvivorLookupFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("survivor_lookup", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "survivor lookup",
            index,
            crate::conflicts::Mode::Exclusive,
            crate::survivors::register_lookup(loader, index, callback, user),
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
        Ok((loader, index)) => crate::conflicts::note(
            "bed check",
            index,
            crate::conflicts::Mode::Chained,
            crate::survivors::register_bed_check(loader, index, callback, user),
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
        Ok((loader, index)) => crate::conflicts::note(
            "summon filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::survivors::register_summon(loader, index, callback, user),
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
        Ok((loader, index)) => crate::survivors::register_summoned(loader, index, callback, user),
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
            let Some(data) = (unsafe { get_headed(def, size_of::<api_frame::StatDefData>()) })
            else {
                return Status::BAD_ARGS;
            };
            Some(data)
        };
        let def = match def {
            Some(data) => match enshwick::stats::StatDef::from_data(&data) {
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
            && let Some(reason) = unexported(loader, record, "stat", id)
        {
            crate::log::write(Level::WARN, &record.id, &reason);
            return Status::REFUSED;
        }
        match crate::stats::open(loader, index, id, def) {
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
        match crate::stats::info(stat) {
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
        match crate::stats::value(entity, stat) {
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
        let Some(request) = (unsafe { get_headed(request, size_of::<api_frame::StatModify>()) })
        else {
            return Status::BAD_ARGS;
        };
        match caller_index(me)
            .and_then(|(loader, index)| crate::stats::modify(loader, index, &request))
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
        match crate::stats::buff(buff, list) {
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
        Ok((_, index)) => match crate::stats::on_changed(index, callback, user) {
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
        match crate::stats::sources(loader, entity, stat) {
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
        match crate::stats::watch(entity, watch != 0) {
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
        let Some(def) = (unsafe { get_headed(def, size_of::<api_frame::AffixDefData>()) }) else {
            return Status::BAD_ARGS;
        };
        match caller_index(me)
            .and_then(|(loader, index)| crate::stats::open_affix(loader, index, id, &def))
        {
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
        let Some(roll) = (unsafe { get_headed(roll, size_of::<api_frame::AffixRoll>()) }) else {
            return Status::BAD_ARGS;
        };
        match crate::stats::roll(loader, &roll) {
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
    item: enshwick::ItemId,
    out: *mut api_frame::ItemLine,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("item_lines", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::stats::item_lines(pide.0, item.get()) {
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
        match crate::stats::affix_info(affix) {
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
    item: enshwick::ItemId,
    survivor: enshwick::abi::NpcId,
) -> Status {
    entry("summon_item", || match caller_index(me) {
        Ok((loader, _)) => crate::survivors::register_item(loader, item.get(), survivor.get()),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn survivor_status(
    me: ModHandle,
    callback: api_frame::SurvivorStatusFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("survivor_status", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "survivor status",
            index,
            crate::conflicts::Mode::Exclusive,
            crate::survivors::register_status(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn interaction_filter(
    me: ModHandle,
    callback: api_frame::InteractionFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("interaction_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "interaction filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::interactions::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn skill_override(
    me: ModHandle,
    node: enshwick::abi::SkillNode,
    callback: api_frame::SkillCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("skill_override", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "skill override",
            index,
            crate::conflicts::Mode::FirstAnswers,
            crate::progress::register_skill(loader, index, node.get(), callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn comfort_filter(
    me: ModHandle,
    callback: api_frame::ComfortFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("comfort_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "comfort filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::progress::register_comfort(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn experience_filter(
    me: ModHandle,
    callback: api_frame::ExperiencePassFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("experience_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "experience filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::progress::register_experience(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn altar_limit_filter(
    me: ModHandle,
    callback: api_frame::AltarLimitFilterFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("altar_limit_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "altar limit filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::bases::add_limit(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn base_limit(
    me: ModHandle,
    out: *mut api_frame::BaseLimitInfo,
) -> Status {
    entry("base_limit", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let now = crate::bases::base_limit();
        let (vanilla, most) = (Limits::VANILLA, crate::bases::most());
        // SAFETY: `HostApi::base_limit` has the mod pass `out` null or
        // readable for its header and writable for the size it names, with no
        // other thread touching it during the call.
        unsafe {
            put_headed(
                out,
                api_frame::BaseLimitInfo {
                    header: api_frame::Header::of::<api_frame::BaseLimitInfo>(),
                    zones: now.zones,
                    bases: now.bases,
                    vanilla_zones: vanilla.zones,
                    reserved: 0,
                    vanilla_bases: vanilla.bases,
                    max_zones: most.zones,
                    max_bases: most.bases,
                },
            )
        }
    })
}

pub(crate) extern "system" fn raise_base_limit(me: ModHandle) -> Status {
    entry("raise_base_limit", || match caller_index(me) {
        Ok((loader, _)) => crate::bases::request(loader, Limits::RAISED),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn request_base_limits(me: ModHandle, zones: u32, bases: u32) -> Status {
    entry("request_base_limits", || match caller_index(me) {
        Ok((loader, _)) => crate::bases::request(loader, Limits { zones, bases }),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn base_zones(
    me: ModHandle,
    zones: *mut u32,
    bases: *mut u32,
) -> Status {
    entry("base_zones", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Some((zone_count, base_count)) = crate::bases::zones() else {
            return Status::MISSING;
        };
        // SAFETY: `HostApi::base_zones` has the mod pass `zones` null or
        // pointing at a writable, aligned `u32` that no other thread touches
        // during the call.
        let status = unsafe { put(zones, zone_count) };
        if !status.is_ok() {
            return status;
        }
        // SAFETY: `HostApi::base_zones` has the mod pass `bases` null or
        // pointing at a writable, aligned `u32` that no other thread touches
        // during the call.
        unsafe { put(bases, base_count) }
    })
}

pub(crate) extern "system" fn craft_filter(
    me: ModHandle,
    callback: api_frame::CraftCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("craft_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "crafting veto",
            index,
            crate::conflicts::Mode::Chained,
            crate::vetoes::register_craft(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn build_filter(
    me: ModHandle,
    callback: api_frame::BuildCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("build_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "building veto",
            index,
            crate::conflicts::Mode::Chained,
            crate::vetoes::register_build(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn damage_filter(
    me: ModHandle,
    callback: enshwick::abi::DamageFilterFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("damage_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "damage filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::damage::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn knowledge_input_filter(
    me: ModHandle,
    callback: enshwick::abi::KnowledgeInputFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("knowledge_input_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "knowledge input filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::knowledge::guard::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn buff_types(
    me: ModHandle,
    out: *mut enshwick::abi::BuffTypeInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("buff_types", || {
        match caller(me).and_then(crate::buffs::types) {
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
    out: *mut enshwick::abi::ActiveBuff,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("buffs", || {
        match caller(me).and_then(|loader| crate::buffs::of(loader, entity)) {
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
    request: *const enshwick::abi::BuffRequest,
) -> Status {
    entry("apply_buff", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe {
            enshwick::abi::read_headed(request, size_of::<enshwick::abi::BuffRequest>())
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::buffs::apply(loader, &request)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn remove_buff(
    me: ModHandle,
    entity: Entity,
    id: enshwick::abi::BuffTypeId,
    removed: *mut u8,
) -> Status {
    entry("remove_buff", || {
        match caller(me).and_then(|loader| crate::buffs::remove(loader, entity, id.get())) {
            // SAFETY: `HostApi::remove_buff` has the mod pass `removed` null or
            // pointing at a writable, aligned `u8` that no other thread touches
            // during the call.
            Ok(had) => unsafe { put(removed, u8::from(had)) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn knowledge(
    me: ModHandle,
    entity: Entity,
    id: enshwick::abi::KnowledgeId,
    out: *mut enshwick::abi::KnowledgeInfo,
) -> Status {
    entry("knowledge", || {
        match caller(me).and_then(|loader| crate::knowledge::value(loader, entity, id.get())) {
            // SAFETY: `HostApi::knowledge` has the mod pass `out` null or
            // pointing at its version of `KnowledgeInfo`, writable for the size
            // its header states, that no other thread touches during the call.
            Ok(info) => unsafe { put_headed(out, info) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn revoke_knowledge(
    me: ModHandle,
    entity: Entity,
    id: enshwick::abi::KnowledgeId,
    kind: u32,
    value: u32,
) -> Status {
    entry("revoke_knowledge", || {
        match caller(me).and_then(|loader| {
            crate::knowledge::changes::revoke(loader, entity.0, id.get(), kind, value)
        }) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn account_knowledge(
    me: ModHandle,
    key: enshwick::PlayerKey,
    id: enshwick::abi::KnowledgeId,
    kind: u32,
    value: u32,
    revoke: u32,
) -> Status {
    entry("account_knowledge", || {
        match caller(me).and_then(|loader| {
            crate::knowledge::changes::for_account(
                loader,
                key.get(),
                id.get(),
                kind,
                value,
                revoke != 0,
            )
        }) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn grant_knowledge(
    me: ModHandle,
    entity: Entity,
    id: enshwick::abi::KnowledgeId,
    kind: u32,
    value: u32,
) -> Status {
    entry("grant_knowledge", || {
        match caller(me)
            .and_then(|loader| crate::knowledge::grant(loader, entity.0, id.get(), kind, value))
        {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn quests(
    me: ModHandle,
    out: *mut enshwick::abi::KnowledgeId,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("quests", || {
        match caller(me).and_then(crate::knowledge::quests) {
            Ok(ids) => {
                if !out.is_null() {
                    for (i, id) in ids.iter().take(capacity as usize).enumerate() {
                        // SAFETY: `out` is non-null, checked just before, and
                        // `i` is below `capacity`; the ABI has the mod pass
                        // `capacity` ids at `out`, so the offset stays inside.
                        let slot = unsafe { out.add(i) };
                        // SAFETY: `slot` is inside the `capacity` writable,
                        // aligned ids the ABI has the mod pass at `out`.
                        unsafe { slot.write(enshwick::abi::KnowledgeId::new(*id)) };
                    }
                }
                // SAFETY: `HostApi::quests` has the mod pass `count` null or
                // pointing at a writable, aligned `u32` that no other thread
                // touches during the call.
                unsafe { put(count, u32::try_from(ids.len()).unwrap_or(u32::MAX)) }
            }
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn rate(me: ModHandle, rate: RateId) -> Status {
    entry("rate", || match calling(me) {
        Ok((loader, index, record)) => {
            let links = crate::rates::links(rate);
            let available = links.iter().all(|&symbol| loader.address(symbol).is_some())
                // The loot rate reads stack sizes from the item registry,
                // which the inventory link finds.
                && (rate != RateId::LOOT || crate::inventory::open(loader).is_ok());
            crate::rates::RATES.declare(index, rate, available, record.joins_when_on())
        }
        Err(status) => status,
    })
}

pub(crate) extern "system" fn set_rate(me: ModHandle, rate: RateId, factor: f32) -> Status {
    entry("set_rate", || match caller_index(me) {
        Ok((_, index)) => {
            let recipes = crate::rates::is_recipe_output(rate);
            let before = recipes.then(|| crate::rate_links::factor(rate).to_bits());
            let status = crate::rates::RATES.set(index, rate, factor);
            if status == Status::OK
                && before.is_some_and(|before| before != crate::rate_links::factor(rate).to_bits())
            {
                crate::rate_links::rebuild_recipes();
            }
            status
        }
        Err(status) => status,
    })
}
