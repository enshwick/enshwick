//! Host functions for the knowledge domain: knowledge values, quests, game
//! queries, skills, comfort, experience and progress.

use enshwick_core::abi::{KnowledgeId, KnowledgeInfo, Status};
use enshwick_core::{Entity, abi as api_frame};

use crate::engine::conflicts::Mode;
use crate::host::layer::{Callback, Caller, Headed, HeadedOut, ListOut, Mod, Out, entries};

entries! {
    fn skill_override(
        me as Mod,
        node: enshwick_core::abi::SkillNode,
        callback as Callback<api_frame::SkillCheckFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "skill override",
            Mode::FirstAnswers,
            crate::knowledge::progress::register_skill(me.loader, me.index, node.get(), callback, user),
        )
    }

    fn comfort_filter(me as Mod, callback as Callback<api_frame::ComfortFilterFn>, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "comfort filter",
            Mode::Chained,
            crate::knowledge::progress::register_comfort(me.loader, me.index, callback, user),
        )
    }

    fn experience_filter(
        me as Mod,
        callback as Callback<api_frame::ExperienceFilterFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "experience filter",
            Mode::Chained,
            crate::knowledge::progress::register_experience(me.loader, me.index, callback, user),
        )
    }

    fn knowledge_input_filter(
        me as Mod,
        callback as Callback<enshwick_core::abi::KnowledgeInputFilterFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "knowledge input filter",
            Mode::Chained,
            crate::knowledge::store::guard::register(me.loader, me.index, callback, user),
        )
    }

    fn quest_reward_filter(
        me as Mod,
        callback as Callback<enshwick_core::abi::QuestRewardFilterFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "quest reward filter",
            Mode::Chained,
            crate::knowledge::quest_pay::register(me.loader, me.index, callback, user),
        )
    }

    fn knowledge(loader as Caller, entity: Entity, id: KnowledgeId, out as HeadedOut<KnowledgeInfo>) -> Status {
        out.put(crate::knowledge::store::value(loader, entity, id.get())?)
    }

    fn game_query(loader as Caller, entity: Entity, query: u32, holds as Out<u32>) -> Status {
        holds.put(u32::from(crate::knowledge::store::query::holds(loader, entity, query)?))
    }

    fn change_knowledge(loader as Caller, change as Headed<api_frame::KnowledgeChange>) -> Status {
        let revoke = match change.op {
            api_frame::KnowledgeChange::GRANT => false,
            api_frame::KnowledgeChange::REVOKE => true,
            _ => return Err(Status::BAD_ARGS.into()),
        };
        let (id, kind, value) = (change.id.get(), u32::from(change.kind), change.value);
        match (change.account.get(), change.entity.0) {
            (0, entity) if revoke => crate::knowledge::store::changes::revoke(loader, entity, id, kind, value)?,
            (0, entity) => crate::knowledge::store::grant(loader, entity, id, kind, value)?,
            (account, 0) => crate::knowledge::store::changes::for_account(loader, account, id, kind, value, revoke)?,
            _ => return Err(Status::BAD_ARGS.into()),
        }
        Ok(())
    }

    fn knowledge_set(loader as Caller, entity: Entity, id: KnowledgeId, value: u32, notice: u8) -> Status {
        use crate::knowledge::store::changes::{Notice, set};
        let notice = match notice {
            api_frame::KnowledgeChange::QUIET => Notice::Quiet,
            api_frame::KnowledgeChange::SHOWN => Notice::Shown,
            _ => return Err(Status::BAD_ARGS.into()),
        };
        set(loader, entity.0, id.get(), value, notice)?;
        Ok(())
    }

    fn quests(loader as Caller, [out, capacity, count] as ListOut<KnowledgeId>) -> Status {
        let ids: Vec<KnowledgeId> = crate::knowledge::store::quests(loader)?
            .into_iter()
            .map(KnowledgeId::new)
            .collect();
        out.put(&ids)
    }

    fn progress_write(me as Mod, entity: Entity, what: u32, value: u32) -> Status {
        crate::knowledge::progress::write(me.loader, me.index, entity.0, what, value)?;
        Ok(())
    }
}
