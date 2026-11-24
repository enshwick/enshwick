//! A mod hears a quest start, a step finish and a part counted, and changes
//! or refuses what the game pays for a quest before it pays it.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{
        Header, QuestData, QuestPayment, QuestProgressData, QuestStepData, Status,
    };
    use enshwick::events::{QuestProgressed, QuestStarted, QuestStepDone};
    use enshwick::knowledge::KnowledgeId;
    use enshwick::quests::QuestPay;
    use enshwick::test::FakeHost;
    use enshwick::{Entity, Load, Mod, Off};

    const PLAYER: Entity = Entity(2);
    const DOUBLED: KnowledgeId = KnowledgeId::new(0x51);
    const REFUSED: KnowledgeId = KnowledgeId::new(0x52);
    const ALLOWED_AGAIN: KnowledgeId = KnowledgeId::new(0x54);

    static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Journal;

    impl Mod for Journal {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let heard = |line: String| HEARD.lock().unwrap().push(line);
            load.on::<QuestStarted>(move |started| heard(format!("started {}", started.quest)))?;
            load.on::<QuestStepDone>(move |done| {
                heard(format!("step {} of {}", done.step, done.quest));
            })?;
            load.on::<QuestProgressed>(move |counted| {
                heard(format!(
                    "part {} at {} of {}",
                    counted.part, counted.value, counted.target
                ));
            })?;
            load.on_quest_reward(|paid| match paid.quest {
                DOUBLED => QuestPay::Pay(
                    paid.experience
                        .iter()
                        .flat_map(|&value| [value, value])
                        .collect(),
                ),
                REFUSED | ALLOWED_AGAIN => QuestPay::Refuse,
                _ => QuestPay::Keep,
            })?;
            load.on_quest_reward(|paid| match paid.quest {
                REFUSED => QuestPay::Pay(paid.experience.clone()),
                ALLOWED_AGAIN if !paid.allowed => QuestPay::Allow,
                _ => QuestPay::Keep,
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Journal,
        r#"
        id = "you.journal"
        side = "both"
        "#
    );

    fn start() -> enshwick::test::Installed {
        HEARD.lock().unwrap().clear();
        let loader = FakeHost::new().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Journal)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn payment(quest: KnowledgeId, experience: &[u32]) -> QuestPayment {
        let mut values = [0; QuestPayment::MAX];
        values[..experience.len()].copy_from_slice(experience);
        QuestPayment {
            header: Header::of::<QuestPayment>(),
            entity: PLAYER,
            quest,
            count: u32::try_from(experience.len()).unwrap(),
            limit: 3,
            allowed: 1,
            reserved: [0; 3],
            experience: values,
        }
    }

    #[test]
    fn a_mod_hears_a_quest_start_a_step_done_and_a_part_counted() {
        let loader = start();
        let quest = KnowledgeId::new(0x51);
        let started = QuestData {
            header: Header::of::<QuestData>(),
            entity: PLAYER,
            quest,
        };
        let done = QuestStepData {
            header: Header::of::<QuestStepData>(),
            entity: PLAYER,
            quest,
            step: 1,
        };
        let counted = QuestProgressData {
            header: Header::of::<QuestProgressData>(),
            entity: Entity(0),
            quest,
            step: 0,
            part: 1,
            value: 3,
            target: 5,
        };
        assert_eq!(loader.raise::<QuestStarted>(&started), 1);
        assert_eq!(loader.raise::<QuestStepDone>(&done), 1);
        assert_eq!(loader.raise::<QuestProgressed>(&counted), 1);
        assert_eq!(
            *HEARD.lock().unwrap(),
            ["started 81", "step 1 of 81", "part 1 at 3 of 5"]
        );
    }

    #[test]
    fn a_filter_pays_a_quest_s_experience_twice_refuses_another_and_keeps_the_rest() {
        let loader = start();
        let doubled = loader.pay_quest(payment(DOUBLED, &[0x77]));
        assert_eq!((doubled.allowed, doubled.count), (1, 2));
        assert_eq!(&doubled.experience[..2], &[0x77, 0x77]);
        let refused = loader.pay_quest(payment(REFUSED, &[0x77]));
        assert_eq!(
            (refused.allowed, refused.count),
            (0, 0),
            "nothing paid though a later filter paid other values, the quest still completes"
        );
        let kept = payment(KnowledgeId::new(0x53), &[0x77]);
        assert_eq!(loader.pay_quest(kept), kept);
    }

    #[test]
    fn a_later_filter_that_sees_a_refusal_may_pay_the_quest_again() {
        let loader = start();
        let paid = loader.pay_quest(payment(ALLOWED_AGAIN, &[0x77]));
        assert_eq!((paid.allowed, paid.count), (1, 1));
    }

    #[test]
    fn a_doubled_payment_is_cut_to_what_the_game_pays() {
        let loader = start();
        let paid = loader.pay_quest(payment(DOUBLED, &[1, 2]));
        assert_eq!(
            (paid.count, &paid.experience[..3]),
            (3, &[1, 1, 2][..]),
            "four asked, three paid"
        );
    }
}
