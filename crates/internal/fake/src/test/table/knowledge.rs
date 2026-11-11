//! The fake's knowledge entries: knowledge, quests, game queries, skills,
//! comfort and experience.

use core::ffi::c_void;

use crate::test::table::{Registered, with};
use enshwick_core::abi::{
    ComfortFn, ExperiencePassFn, KnowledgeInfo, KnowledgeInputFn, ModHandle, SkillCheckFn, Status,
};

pub(crate) extern "system" fn skill_override(
    me: ModHandle,
    node: enshwick_core::abi::SkillNode,
    callback: SkillCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.skills.push((node, Registered::new(callback, user, me)));
        Status::OK
    })
}

pub(crate) extern "system" fn comfort_filter(
    me: ModHandle,
    callback: ComfortFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.comfort_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn experience_filter(
    me: ModHandle,
    callback: ExperiencePassFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.experience_filters
            .push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn revoke_knowledge(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    id: enshwick_core::abi::KnowledgeId,
    kind: u32,
    value: u32,
) -> Status {
    with(Status::MISSING, |s| {
        s.revokes
            .push((enshwick_core::Entity::from_raw(entity.0), id, kind, value));
        Status::OK
    })
}

pub(crate) extern "system" fn account_knowledge(
    _me: ModHandle,
    key: enshwick_core::PlayerKey,
    id: enshwick_core::abi::KnowledgeId,
    kind: u32,
    value: u32,
    revoke: u32,
) -> Status {
    with(Status::MISSING, |s| {
        s.account_changes.push((key, id, kind, value, revoke != 0));
        Status::OK
    })
}

pub(crate) extern "system" fn knowledge_input_filter(
    me: ModHandle,
    callback: KnowledgeInputFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.knowledge_input_filters
            .push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn knowledge(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    id: enshwick_core::abi::KnowledgeId,
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
    if unsafe { enshwick_core::abi::write_headed(out, info) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

pub(crate) extern "system" fn game_query(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    query: u32,
    holds: *mut u32,
) -> Status {
    if holds.is_null() {
        return Status::BAD_ARGS;
    }
    let holder = (entity != enshwick_core::Entity(0)).then_some(entity);
    let answer = with(Err(Status::MISSING), |s| {
        // As the loader, which refuses a character with no player record.
        if holder.is_some() && !s.host.players.iter().any(|player| player.entity == entity) {
            return Err(Status::REFUSED);
        }
        s.game_queries
            .iter()
            .find(|(e, q, _)| *q == query && *e == holder)
            .map(|(_, _, held)| *held)
            .ok_or(Status::MISSING)
    });
    let held = match answer {
        Ok(held) => held,
        Err(status) => return status,
    };
    // SAFETY: `holds` is non-null, checked above, and `HostApi::game_query`
    // has the mod pass it writable for the call.
    unsafe { holds.write(u32::from(held)) };
    Status::OK
}

pub(crate) extern "system" fn grant_knowledge(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    id: enshwick_core::abi::KnowledgeId,
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
            .push((enshwick_core::Entity::from_raw(entity.0), id, kind, value));
        Status::OK
    })
}

pub(crate) extern "system" fn quests(
    _me: ModHandle,
    out: *mut enshwick_core::abi::KnowledgeId,
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
