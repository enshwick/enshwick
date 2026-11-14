//! Host functions for the knowledge domain: knowledge values, quests, game
//! queries, skills, comfort and experience.

use enshwick_core::abi::{KnowledgeId, KnowledgeInfo, Status};
use enshwick_core::{Entity, abi as api_frame};

use crate::engine::conflicts::Mode;
use crate::host::layer::{Caller, HeadedOut, ListOut, Mod, Out, checked, entries};

entries! {
    fn skill_override(
        me as Mod,
        node: enshwick_core::abi::SkillNode,
        callback: api_frame::SkillCheckFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "skill override",
            Mode::FirstAnswers,
            checked(crate::knowledge::progress::register_skill(
                me.loader,
                me.index,
                node.get(),
                callback,
                user,
            )),
        )
    }

    fn comfort_filter(me as Mod, callback: api_frame::ComfortFn, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "comfort filter",
            Mode::Chained,
            checked(crate::knowledge::progress::register_comfort(me.loader, me.index, callback, user)),
        )
    }

    fn experience_filter(
        me as Mod,
        callback: api_frame::ExperiencePassFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "experience filter",
            Mode::Chained,
            checked(crate::knowledge::progress::register_experience(
                me.loader, me.index, callback, user,
            )),
        )
    }

    fn knowledge_input_filter(
        me as Mod,
        callback: enshwick_core::abi::KnowledgeInputFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "knowledge input filter",
            Mode::Chained,
            checked(crate::knowledge::store::guard::register(me.loader, me.index, callback, user)),
        )
    }

    fn knowledge(loader as Caller, entity: Entity, id: KnowledgeId, out as HeadedOut<KnowledgeInfo>) -> Status {
        out.put(crate::knowledge::store::value(loader, entity, id.get())?)
    }

    fn game_query(loader as Caller, entity: Entity, query: u32, holds as Out<u32>) -> Status {
        holds.put(u32::from(crate::knowledge::store::query::holds(loader, entity, query)?))
    }

    fn revoke_knowledge(loader as Caller, entity: Entity, id: KnowledgeId, kind: u32, value: u32) -> Status {
        crate::knowledge::store::changes::revoke(loader, entity.0, id.get(), kind, value)?;
        Ok(())
    }

    fn account_knowledge(
        loader as Caller,
        key: enshwick_core::PlayerKey,
        id: KnowledgeId,
        kind: u32,
        value: u32,
        revoke: u32,
    ) -> Status {
        crate::knowledge::store::changes::for_account(
            loader,
            key.get(),
            id.get(),
            kind,
            value,
            revoke != 0,
        )?;
        Ok(())
    }

    fn grant_knowledge(loader as Caller, entity: Entity, id: KnowledgeId, kind: u32, value: u32) -> Status {
        crate::knowledge::store::grant(loader, entity.0, id.get(), kind, value)?;
        Ok(())
    }

    fn quests(loader as Caller, [out, capacity, count] as ListOut<KnowledgeId>) -> Status {
        let ids: Vec<KnowledgeId> = crate::knowledge::store::quests(loader)?
            .into_iter()
            .map(KnowledgeId::new)
            .collect();
        out.put(&ids)
    }
}
