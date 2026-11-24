//! The fake's knowledge entries: knowledge, quests, game queries, skills,
//! comfort and experience.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use crate::test::table::{Registered, State, with};
use enshwick_core::abi::{
    ComfortFilterFn, ExperienceFilterFn, KnowledgeInfo, KnowledgeInputFilterFn, SkillCheckFn,
    Status,
};

/// What the installed fake's knowledge answers and has recorded.
#[derive(Default)]
pub(crate) struct KnowledgeState {
    pub(crate) knowledge_input_filters: Vec<Registered<KnowledgeInputFilterFn>>,
    /// Knowledge values by `(character or 0, id)`.
    pub(crate) values: Vec<(Option<enshwick_core::Entity>, KnowledgeInfo)>,
    /// Whether each game query holds, by `(character or 0, query)`.
    pub(crate) game_queries: Vec<(Option<enshwick_core::Entity>, u32, bool)>,
    /// The grants the mod asked for: character, id, kind, value.
    pub(crate) grants: Vec<(
        Option<enshwick_core::Entity>,
        enshwick_core::abi::KnowledgeId,
        u32,
        u32,
    )>,
    /// The revokes the mod asked for: character, id, kind, value.
    pub(crate) revokes: Vec<(
        Option<enshwick_core::Entity>,
        enshwick_core::abi::KnowledgeId,
        u32,
        u32,
    )>,
    /// The changes the mod asked for accounts: key, id, kind, value,
    /// whether a revoke.
    pub(crate) account_changes: Vec<(
        enshwick_core::PlayerKey,
        enshwick_core::abi::KnowledgeId,
        u32,
        u32,
        bool,
    )>,
    /// The quest ids the fake knows.
    pub(crate) quests: Vec<enshwick_core::abi::KnowledgeId>,
    pub(crate) skills: Vec<(enshwick_core::abi::SkillNode, Registered<SkillCheckFn>)>,
    pub(crate) comfort_filters: Vec<Registered<ComfortFilterFn>>,
    pub(crate) experience_filters: Vec<Registered<ExperienceFilterFn>>,
}

entries! {
    pub(crate) fn skill_override(
        me as Mod,
        node: enshwick_core::abi::SkillNode,
        callback: Option<SkillCheckFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            s.knowledge
                .skills
                .push((node, Registered::new(callback, user, me)));
            Status::OK
        })
    }

    pub(crate) fn comfort_filter(
        me as Mod,
        callback: Option<ComfortFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            s.knowledge
                .comfort_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn experience_filter(
        me as Mod,
        callback: Option<ExperienceFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            s.knowledge
                .experience_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }
}

/// The scope and expected flags of knowledge `id` when the fake knows it
/// and `kind` is its kind; `BAD_ARGS` otherwise, as the loader answers on
/// the game.
fn definition(
    s: &State,
    id: enshwick_core::abi::KnowledgeId,
    kind: u32,
) -> Result<(u8, u32), Status> {
    s.knowledge
        .values
        .iter()
        .find(|(_, k)| k.id == id)
        .filter(|(_, k)| u32::from(k.kind) == kind)
        .map(|(_, k)| (k.scope, k.flags))
        .ok_or(Status::BAD_ARGS)
}

/// Whether a change of a character's knowledge names the world, which the
/// loader refuses: such knowledge has no world value.
fn names_the_world(scope: u8, entity: enshwick_core::Entity) -> bool {
    scope == KnowledgeInfo::PLAYER && entity.0 == 0
}

/// Whether `value` names a flag the loader cannot set or clear: one past the
/// 32 a value holds, or one outside the definition's `expected` flags when
/// it lists any.
fn flag_outside(kind: u32, value: u32, expected: u32) -> bool {
    kind == u32::from(KnowledgeInfo::FLAGS)
        && (value >= u32::BITS || (expected != 0 && expected & (1 << value) == 0))
}

fn revoke(
    entity: enshwick_core::Entity,
    id: enshwick_core::abi::KnowledgeId,
    kind: u32,
    value: u32,
) -> Status {
    with(Status::MISSING, |s| {
        let scope = match definition(s, id, kind) {
            Ok((scope, _)) => scope,
            Err(status) => return status,
        };
        if names_the_world(scope, entity) {
            return Status::REFUSED;
        }
        // The loader clears any bit of the 32, listed or not.
        if flag_outside(kind, value, 0) {
            return Status::BAD_ARGS;
        }
        s.knowledge
            .revokes
            .push((enshwick_core::Entity::from_raw(entity.0), id, kind, value));
        Status::OK
    })
}

fn for_account(
    key: enshwick_core::PlayerKey,
    id: enshwick_core::abi::KnowledgeId,
    kind: u32,
    value: u32,
    revoke: bool,
) -> Status {
    with(Status::MISSING, |s| {
        // As the loader, which keeps only a character's knowledge per account.
        if definition(s, id, kind).map(|(scope, _)| scope) != Ok(KnowledgeInfo::PLAYER) {
            return Status::BAD_ARGS;
        }
        s.knowledge
            .account_changes
            .push((key, id, kind, value, revoke));
        Status::OK
    })
}

entries! {
    pub(crate) fn knowledge_input_filter(
        me as Mod,
        callback: Option<KnowledgeInputFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            s.knowledge
                .knowledge_input_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn knowledge(
        me as Caller,
        entity: enshwick_core::Entity,
        id: enshwick_core::abi::KnowledgeId,
        out: *mut KnowledgeInfo as args::HeadedOut,
    ) -> Status {
        let (known, found) = with((false, None), |s| {
            let mut values = s.knowledge.values.iter().filter(|(_, k)| k.id == id);
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

    pub(crate) fn game_query(
        me as Caller,
        entity: enshwick_core::Entity,
        query: u32,
        holds: *mut u32 as args::Out,
    ) -> Status {
        if holds.is_null() {
            return Status::BAD_ARGS;
        }
        let holder = (entity != enshwick_core::Entity(0)).then_some(entity);
        let answer = with(Err(Status::MISSING), |s| {
            // As the loader, which refuses a character with no player record.
            if holder.is_some()
                && !s
                    .host
                    .base
                    .players
                    .iter()
                    .any(|player| player.entity == entity)
            {
                return Err(Status::REFUSED);
            }
            s.knowledge
                .game_queries
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

    /// As the loader's `change_knowledge`: a grant or revoke of a character
    /// or the world, or a change of an account.
    pub(crate) fn change_knowledge(
        me as Caller,
        change: *const enshwick_core::abi::KnowledgeChange as args::Headed,
    ) -> Status {
        let Some(change) = super::get_headed(change) else {
            return Status::BAD_ARGS;
        };
        let revoking = match change.op {
            enshwick_core::abi::KnowledgeChange::GRANT => false,
            enshwick_core::abi::KnowledgeChange::REVOKE => true,
            _ => return Status::BAD_ARGS,
        };
        let (id, kind, value) = (change.id, u32::from(change.kind), change.value);
        match (change.account.get(), change.entity) {
            (0, entity) if revoking => revoke(entity, id, kind, value),
            (0, entity) => grant(entity, id, kind, value),
            (_, enshwick_core::Entity(0)) => for_account(change.account, id, kind, value, revoking),
            _ => Status::BAD_ARGS,
        }
    }
}

fn grant(
    entity: enshwick_core::Entity,
    id: enshwick_core::abi::KnowledgeId,
    kind: u32,
    value: u32,
) -> Status {
    with(Status::MISSING, |s| {
        let (scope, expected) = match definition(s, id, kind) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if flag_outside(kind, value, expected) {
            return Status::BAD_ARGS;
        }
        if names_the_world(scope, entity) {
            return Status::REFUSED;
        }
        s.knowledge
            .grants
            .push((enshwick_core::Entity::from_raw(entity.0), id, kind, value));
        Status::OK
    })
}

entries! {
    pub(crate) fn quests(
        me as Caller,
        [out: *mut enshwick_core::abi::KnowledgeId, capacity: u32, count: *mut u32] as args::ListOut,
    ) -> Status {
        let ids = with(Vec::new(), |s| s.knowledge.quests.clone());
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
}
