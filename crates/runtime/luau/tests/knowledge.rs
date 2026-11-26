//! `enshwick.knowledge`: the input filter a script links at start, whose
//! nil keeps the verdict so far.

#[cfg(test)]
mod tests {
    use enshwick::Entity;
    use enshwick::abi::{KnowledgeInfo, KnowledgeInput};
    use enshwick::knowledge::KnowledgeId;
    use enshwick::test::FakeHost;

    use crate::harness::run;

    fn input(value: u32, allowed: bool) -> KnowledgeInput {
        enshwick::abi::headed!(KnowledgeInput {
            character: Entity::from_engine(2),
            id: KnowledgeId::new(0x1d),
            kind: KnowledgeInfo::BOOL,
            scope: KnowledgeInfo::PLAYER,
            reserved: 0,
            value,
            refusal: KnowledgeInput::ALLOWED,
            allowed: u8::from(allowed),
            reserved_tail: [0; 3],
        })
    }

    #[test]
    fn a_knowledge_input_filter_allows_refuses_or_keeps_the_verdict_with_nil() {
        let loader = run(
            FakeHost::new(),
            r"
            enshwick.knowledge.on_knowledge_input(function(request)
                if request.value == 1 then
                    return true
                elseif request.value == 2 then
                    return false
                end
                return nil
            end)
            ",
        );
        assert!(loader.knowledge_input(input(1, false)), "true allows");
        assert!(!loader.knowledge_input(input(2, true)), "false refuses");
        assert!(loader.knowledge_input(input(3, true)), "nil keeps an allow");
        assert!(
            !loader.knowledge_input(input(3, false)),
            "nil keeps a refusal"
        );
    }

    #[test]
    fn a_script_sets_exact_knowledge_quietly_or_shown() {
        let loader = run(
            FakeHost::new(),
            r#"
            enshwick.on("Ticked", function()
                enshwick.knowledge.set(2, 0x1d, 3, "Quiet")
                enshwick.knowledge.set_world(0x77, 1, "Shown")
            end)
            "#,
        );
        let known = |id, kind, scope| {
            enshwick::abi::headed!(KnowledgeInfo {
                id: KnowledgeId::new(id),
                kind,
                scope,
                reserved: 0,
                value: 0,
                flags: 0,
            })
        };
        loader.set_knowledge(
            &[
                (
                    Some(Entity::from_engine(2)),
                    known(0x1d, KnowledgeInfo::FLAGS, KnowledgeInfo::PLAYER),
                ),
                (None, known(0x77, KnowledgeInfo::BOOL, KnowledgeInfo::WORLD)),
            ],
            &[],
        );
        let _ran = loader.raise::<enshwick::events::Ticked>(&enshwick::abi::TickData::new(1));
        assert_eq!(
            loader.knowledge_sets(),
            [
                (
                    Some(Entity::from_engine(2)),
                    KnowledgeId::new(0x1d),
                    3,
                    enshwick::abi::KnowledgeChange::QUIET
                ),
                (
                    None,
                    KnowledgeId::new(0x77),
                    1,
                    enshwick::abi::KnowledgeChange::SHOWN
                ),
            ],
            "{:?}",
            loader.logs()
        );
    }

    fn payment(quest: u32) -> enshwick::abi::QuestPayment {
        enshwick::abi::headed!(enshwick::abi::QuestPayment {
            entity: Entity::from_engine(2),
            quest: KnowledgeId::new(quest),
            count: 1,
            limit: 3,
            allowed: 1,
            reserved: [0; 3],
            experience: [0x77; enshwick::abi::QuestPayment::MAX],
        })
    }

    #[test]
    fn a_quest_reward_filter_pays_a_list_refuses_with_false_and_keeps_with_nil() {
        let loader = run(
            FakeHost::new(),
            r"
            enshwick.quests.on_quest_reward(function(paid)
                if paid.quest == 0x51 then
                    return { paid.experience[1], paid.experience[1] }
                elseif paid.quest == 0x52 then
                    return false
                end
                return nil
            end)
            ",
        );
        let doubled = loader.pay_quest(payment(0x51));
        assert_eq!(
            (doubled.count, &doubled.experience[..2]),
            (2, &[0x77, 0x77][..])
        );
        assert_eq!(loader.pay_quest(payment(0x52)).allowed, 0, "false refuses");
        assert_eq!(loader.pay_quest(payment(0x53)), payment(0x53), "nil keeps");
    }
}
