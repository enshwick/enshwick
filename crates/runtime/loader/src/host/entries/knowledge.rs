//! Host functions for the knowledge domain: knowledge values, quests, game
//! queries, skills, comfort and experience.

use crate::host::memory::{put, put_headed};
use crate::host::{caller, caller_index, entry};
use enshwick_core::abi::{ModHandle, Status};
use enshwick_core::{Entity, abi as api_frame};

pub(crate) extern "system" fn skill_override(
    me: ModHandle,
    node: enshwick_core::abi::SkillNode,
    callback: api_frame::SkillCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("skill_override", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "skill override",
            index,
            crate::engine::conflicts::Mode::FirstAnswers,
            crate::knowledge::progress::register_skill(loader, index, node.get(), callback, user),
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
        Ok((loader, index)) => crate::engine::conflicts::note(
            "comfort filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::knowledge::progress::register_comfort(loader, index, callback, user),
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
        Ok((loader, index)) => crate::engine::conflicts::note(
            "experience filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::knowledge::progress::register_experience(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn knowledge_input_filter(
    me: ModHandle,
    callback: enshwick_core::abi::KnowledgeInputFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("knowledge_input_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "knowledge input filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::knowledge::store::guard::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn knowledge(
    me: ModHandle,
    entity: Entity,
    id: enshwick_core::abi::KnowledgeId,
    out: *mut enshwick_core::abi::KnowledgeInfo,
) -> Status {
    entry("knowledge", || {
        match caller(me).and_then(|loader| crate::knowledge::store::value(loader, entity, id.get()))
        {
            // SAFETY: `HostApi::knowledge` has the mod pass `out` null or
            // pointing at its version of `KnowledgeInfo`, writable for the size
            // its header states, that no other thread touches during the call.
            Ok(info) => unsafe { put_headed(out, info) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn game_query(
    me: ModHandle,
    entity: Entity,
    query: u32,
    holds: *mut u32,
) -> Status {
    entry("game_query", || {
        if holds.is_null() {
            return Status::BAD_ARGS;
        }
        match caller(me)
            .and_then(|loader| crate::knowledge::store::query::holds(loader, entity, query))
        {
            Ok(held) => {
                // SAFETY: `holds` is non-null, checked above, and
                // `HostApi::game_query` has the mod pass it writable for the
                // call.
                unsafe { holds.write(u32::from(held)) };
                Status::OK
            }
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn revoke_knowledge(
    me: ModHandle,
    entity: Entity,
    id: enshwick_core::abi::KnowledgeId,
    kind: u32,
    value: u32,
) -> Status {
    entry("revoke_knowledge", || {
        match caller(me).and_then(|loader| {
            crate::knowledge::store::changes::revoke(loader, entity.0, id.get(), kind, value)
        }) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn account_knowledge(
    me: ModHandle,
    key: enshwick_core::PlayerKey,
    id: enshwick_core::abi::KnowledgeId,
    kind: u32,
    value: u32,
    revoke: u32,
) -> Status {
    entry("account_knowledge", || {
        match caller(me).and_then(|loader| {
            crate::knowledge::store::changes::for_account(
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
    id: enshwick_core::abi::KnowledgeId,
    kind: u32,
    value: u32,
) -> Status {
    entry("grant_knowledge", || {
        match caller(me).and_then(|loader| {
            crate::knowledge::store::grant(loader, entity.0, id.get(), kind, value)
        }) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn quests(
    me: ModHandle,
    out: *mut enshwick_core::abi::KnowledgeId,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("quests", || {
        match caller(me).and_then(crate::knowledge::store::quests) {
            Ok(ids) => {
                if !out.is_null() {
                    for (i, id) in ids.iter().take(capacity as usize).enumerate() {
                        // SAFETY: `out` is non-null, checked just before, and
                        // `i` is below `capacity`; the ABI has the mod pass
                        // `capacity` ids at `out`, so the offset stays inside.
                        let slot = unsafe { out.add(i) };
                        // SAFETY: `slot` is inside the `capacity` writable,
                        // aligned ids the ABI has the mod pass at `out`.
                        unsafe { slot.write(enshwick_core::abi::KnowledgeId::new(*id)) };
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
