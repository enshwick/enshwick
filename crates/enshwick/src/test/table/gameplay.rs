//! The fake's altars, survivors, buffs, knowledge, game rates and the game's checks.

use core::ffi::c_void;

use crate::abi::{
    ActiveBuff, AltarInfo, AltarLimitFilterFn, AltarZoneFn, BaseLimitInfo, BedCheckFn, BuffRequest,
    BuffTypeInfo, BuildCheckFn, BuildingZoneCheckFn, ComfortFn, CraftCheckFn, DamageFilterFn,
    ExperiencePassFn, Header, InteractionFn, KnowledgeInfo, KnowledgeInputFn, ModHandle, RateId,
    Side, SkillCheckFn, Status, SummonFn, SurvivorLookupFn, SurvivorStatusFn,
};
use crate::test::table::mods::list;
use crate::test::table::{Registration, put, with};

pub(crate) extern "system" fn altars_open(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.altars_opened = true;
        Status::OK
    })
}

pub(crate) extern "system" fn altars(
    _me: ModHandle,
    out: *mut AltarInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Some(altars) = with(None, |s| s.altars_opened.then(|| s.altars.clone())) else {
        return Status::REFUSED;
    };
    list(&altars, out, entry_size, capacity, count)
}

pub(crate) extern "system" fn altar_zone_filter(
    _me: ModHandle,
    callback: AltarZoneFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.altars_opened = true;
        s.zone_control = true;
        s.zone_sizing
            .filters
            .push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn altar_zones_changed(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if !s.zone_control {
            return Status::REFUSED;
        }
        s.zone_sizing.resizes += 1;
        Status::OK
    })
}

pub(crate) extern "system" fn building_zone_check_filter(
    _me: ModHandle,
    callback: BuildingZoneCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.zone_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn survivor_lookup(
    _me: ModHandle,
    callback: SurvivorLookupFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.survivor_lookup.is_some() {
            return Status::REFUSED;
        }
        s.survivor_lookup = Some((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn survivor_bed_check(
    _me: ModHandle,
    callback: BedCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.bed_checks.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn summon_filter(
    _me: ModHandle,
    callback: SummonFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.summons.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn summoned(
    _me: ModHandle,
    callback: SummonFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.summoned.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn summon_item(
    _me: ModHandle,
    item: crate::ItemId,
    survivor: crate::survivors::NpcId,
) -> Status {
    with(Status::MISSING, |s| {
        s.summon_items.push((item, survivor));
        Status::OK
    })
}

pub(crate) extern "system" fn survivor_status(
    _me: ModHandle,
    callback: SurvivorStatusFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.host.side != Side::CLIENT || s.survivor_status.is_some() {
            return Status::REFUSED;
        }
        s.survivor_status = Some((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn interaction_filter(
    _me: ModHandle,
    callback: InteractionFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.interaction_filters
            .push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn skill_override(
    _me: ModHandle,
    node: crate::abi::SkillNode,
    callback: SkillCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.skills.push((node, callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn comfort_filter(
    _me: ModHandle,
    callback: ComfortFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.comfort_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn experience_filter(
    _me: ModHandle,
    callback: ExperiencePassFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.experience_filters
            .push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn altar_limit_filter(
    _me: ModHandle,
    callback: AltarLimitFilterFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.altar_limits.push((callback, user.expose_provenance()));
        Status::OK
    })
}

/// The altar zones and bases the fake's game groups by itself, as the
/// game's.
const VANILLA: (u32, u32) = (16, 16);
/// The most altar zones and bases a mod may ask for, as the loader's.
const MOST: (u32, u32) = (255, 179);
/// What raising the base limit asks for.
const RAISED: (u32, u32) = (32, 16);

/// The zones and bases the fake's game groups, as the loader's: the game's
/// own, or the mod's largest request once its hooks are armed.
pub(crate) fn zones_and_bases() -> (u32, u32) {
    let most = most();
    with(None, |s| {
        s.base_limit.requested.filter(|_| s.base_limit.armed)
    })
    .map_or(VANILLA, |(zones, bases)| (zones, bases.min(most.1)))
}

/// The most a mod may ask for, as the loader's: its own most, or 16 bases
/// once arming finds a build that keeps them.
fn most() -> (u32, u32) {
    let kept = with(false, |s| {
        s.base_limit.armed && s.host.base_code == crate::test::BaseCode::ZonesOnly
    });
    if kept { (MOST.0, VANILLA.1) } else { MOST }
}

/// Takes a request for `zones` and `bases`, as the loader does: refused
/// past the most it takes or after init, kept as the largest of each.
fn request(zones: u32, bases: u32) -> Status {
    if zones > MOST.0 || bases > MOST.1 {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.base_limit.armed {
            return Status::TOO_LATE;
        }
        if s.host.base_code == crate::test::BaseCode::Unknown {
            return Status::MISSING;
        }
        let (old_zones, old_bases) = s.base_limit.requested.unwrap_or(VANILLA);
        s.base_limit.requested = Some((
            zones.max(old_zones).max(VANILLA.0),
            bases.max(old_bases).max(VANILLA.1),
        ));
        Status::OK
    })
}

pub(crate) extern "system" fn base_limit(_me: ModHandle, out: *mut BaseLimitInfo) -> Status {
    let (zones, bases) = zones_and_bases();
    let (max_zones, max_bases) = most();
    let limit = BaseLimitInfo {
        header: Header::of::<BaseLimitInfo>(),
        zones,
        bases,
        vanilla_zones: VANILLA.0,
        reserved: 0,
        vanilla_bases: VANILLA.1,
        max_zones,
        max_bases,
    };
    // SAFETY: the safe layer passes `out` null or a `BaseLimitInfo` with
    // its header set, writable for the call.
    if unsafe { crate::abi::write_headed(out, limit) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

pub(crate) extern "system" fn raise_base_limit(_me: ModHandle) -> Status {
    request(RAISED.0, RAISED.1)
}

pub(crate) extern "system" fn request_base_limits(
    _me: ModHandle,
    zones: u32,
    bases: u32,
) -> Status {
    request(zones, bases)
}

pub(crate) extern "system" fn base_zones(
    _me: ModHandle,
    zones: *mut u32,
    bases: *mut u32,
) -> Status {
    let Some((zone_count, base_count)) = with(None, |s| s.grouping) else {
        return Status::MISSING;
    };
    let status = put(zones, zone_count);
    if !status.is_ok() {
        return status;
    }
    put(bases, base_count)
}

pub(crate) extern "system" fn craft_filter(
    _me: ModHandle,
    callback: CraftCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.craft_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn build_filter(
    _me: ModHandle,
    callback: BuildCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.build_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn revoke_knowledge(
    _me: ModHandle,
    entity: crate::Entity,
    id: crate::abi::KnowledgeId,
    kind: u32,
    value: u32,
) -> Status {
    with(Status::MISSING, |s| {
        s.revokes
            .push((crate::Entity::from_raw(entity.0), id, kind, value));
        Status::OK
    })
}

pub(crate) extern "system" fn account_knowledge(
    _me: ModHandle,
    key: crate::PlayerKey,
    id: crate::abi::KnowledgeId,
    kind: u32,
    value: u32,
    revoke: u32,
) -> Status {
    with(Status::MISSING, |s| {
        s.account_changes.push((key, id, kind, value, revoke != 0));
        Status::OK
    })
}

pub(crate) extern "system" fn damage_filter(
    _me: ModHandle,
    callback: DamageFilterFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.damage_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn knowledge_input_filter(
    _me: ModHandle,
    callback: KnowledgeInputFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.knowledge_input_filters
            .push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn buff_types(
    _me: ModHandle,
    out: *mut BuffTypeInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let types = with(Vec::new(), |s| s.buff_types.clone());
    super::put_headed_list(&types, out, capacity, count)
}

pub(crate) extern "system" fn buffs(
    _me: ModHandle,
    entity: crate::Entity,
    out: *mut ActiveBuff,
    capacity: u32,
    count: *mut u32,
) -> Status {
    match with(None::<Vec<ActiveBuff>>, |s| s.buffs.get(&entity.0).cloned()) {
        Some(on) => super::put_headed_list(&on, out, capacity, count),
        None => Status::MISSING,
    }
}

pub(crate) extern "system" fn apply_buff(_me: ModHandle, request: *const BuffRequest) -> Status {
    let Some(request) = super::get_headed(request) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        let Some(ty) = s.buff_types.iter().find(|ty| ty.id == request.id).copied() else {
            return Status::BAD_ARGS;
        };
        let Some(on) = s.buffs.get_mut(&request.entity.0) else {
            return Status::MISSING;
        };
        let duration = if request.duration == 0 {
            ty.duration
        } else {
            request.duration
        };
        if let Some(buff) = on.iter_mut().find(|buff| buff.id == request.id) {
            buff.duration = duration;
            buff.remaining = duration;
        } else {
            on.push(ActiveBuff {
                header: Header::of::<ActiveBuff>(),
                id: request.id,
                entity: crate::Entity(0x8000 + u32::try_from(on.len()).unwrap_or(0)),
                slot: u32::from(ty.slot),
                level: request.level,
                duration,
                remaining: duration,
            });
        }
        Status::OK
    })
}

pub(crate) extern "system" fn remove_buff(
    _me: ModHandle,
    entity: crate::Entity,
    id: crate::buffs::BuffTypeId,
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

pub(crate) extern "system" fn knowledge(
    _me: ModHandle,
    entity: crate::Entity,
    id: crate::abi::KnowledgeId,
    out: *mut KnowledgeInfo,
) -> Status {
    let (known, found) = with((false, None), |s| {
        let mut values = s.knowledge.iter().filter(|(_, k)| k.id == id);
        let known = values.clone().next().is_some();
        let found = values
            .find(|(e, k)| k.scope == KnowledgeInfo::WORLD || *e == Some(entity))
            .map(|(_, k)| *k);
        (known, found)
    });
    let Some(info) = found else {
        return if known {
            Status::REFUSED
        } else {
            Status::MISSING
        };
    };
    // SAFETY: the safe layer passes a headed struct for the call.
    if unsafe { crate::abi::write_headed(out, info) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

pub(crate) extern "system" fn grant_knowledge(
    _me: ModHandle,
    entity: crate::Entity,
    id: crate::abi::KnowledgeId,
    kind: u32,
    value: u32,
) -> Status {
    with(Status::MISSING, |s| {
        let Some(info) = s
            .knowledge
            .iter()
            .find(|(_, k)| k.id == id)
            .map(|(_, k)| *k)
        else {
            return Status::BAD_ARGS;
        };
        if u32::from(info.kind) != kind {
            return Status::BAD_ARGS;
        }
        s.grants
            .push((crate::Entity::from_raw(entity.0), id, kind, value));
        Status::OK
    })
}

pub(crate) extern "system" fn quests(
    _me: ModHandle,
    out: *mut crate::abi::KnowledgeId,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let ids = with(Vec::new(), |s| s.quests.clone());
    if count.is_null() {
        return Status::BAD_ARGS;
    }
    if !out.is_null() {
        for (i, id) in ids.iter().take(capacity as usize).enumerate() {
            // SAFETY: `i` is below `capacity` and the safe layer passes a
            // non-null `out` with room for `capacity` ids, so the offset
            // stays inside that array.
            let slot = unsafe { out.add(i) };
            // SAFETY: `slot` is inside the `capacity` writable, aligned
            // ids the safe layer passes at `out`.
            unsafe { slot.write(*id) };
        }
    }
    // SAFETY: `count` is non-null, checked at the top; the safe layer passes
    // it pointing at a writable, aligned `u32` for the call.
    unsafe { count.write(u32::try_from(ids.len()).unwrap_or(u32::MAX)) };
    Status::OK
}

pub(crate) extern "system" fn rate(_me: ModHandle, rate: RateId) -> Status {
    with(Status::MISSING, |s| {
        if crate::rates::Rate::from_id(rate).is_none() {
            return Status::BAD_ARGS;
        }
        let joins = s.manifest.as_ref().is_none_or(|manifest| manifest.joins);
        if !crate::rules::may_declare_rate(rate, joins) {
            return Status::REFUSED;
        }
        if s.host.missing_rates.contains(&rate) {
            return Status::MISSING;
        }
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if !s.rates.iter().any(|&(declared, _)| declared == rate) {
            s.rates.push((rate, 1.0));
        }
        Status::OK
    })
}

pub(crate) extern "system" fn set_rate(_me: ModHandle, rate: RateId, factor: f32) -> Status {
    if !crate::rates::is_valid_factor(factor) {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        match s.rates.iter_mut().find(|(declared, _)| *declared == rate) {
            Some((_, set)) => {
                *set = factor;
                Status::OK
            }
            None => Status::BAD_ARGS,
        }
    })
}
