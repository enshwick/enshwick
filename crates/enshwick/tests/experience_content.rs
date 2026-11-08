//! Mods add experience values beside the game's in its balancing table, for
//! knowledge entries and quest rewards; the game's own values list by
//! source and gain, a mod's second value of one id replaces its first, and
//! an id the game holds is refused.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::content::knowledge::{ExperienceValue, NewExperience, NewKnowledge};
    use enshwick::content::vanilla::{self, Experience};
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::game_events::ExperienceSourceType;
    use enshwick::knowledge::{KnowledgeType, Scope};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{FakeHost, VANILLA_EXPERIENCE, VANILLA_QUEST_EXPERIENCE};
    use enshwick::{Load, Mod, Off};

    const SHRINE: Id = Id::new("you.lore:shrine-xp");
    const ODD: Id = Id::new("you.lore:odd-xp");

    static ADDED: Mutex<Vec<ExperienceValue>> = Mutex::new(Vec::new());
    static VANILLA: Mutex<Vec<Experience>> = Mutex::new(Vec::new());
    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Lore;

    impl Mod for Lore {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            VANILLA
                .lock()
                .unwrap()
                .extend(vanilla::experience_values().map_err(|e| Off::new(e.to_string()))?);
            let shrine = load.add_experience(NewExperience::new(
                SHRINE,
                ExperienceSourceType::Exploration,
                250,
            ))?;
            let odd =
                load.add_experience(NewExperience::new(ODD, ExperienceSourceType::Other(9), 7))?;
            load.add_knowledge(NewKnowledge {
                experience: Some(shrine),
                ..NewKnowledge::new(
                    Id::new("you.lore:shrine"),
                    Scope::Player,
                    KnowledgeType::Bool,
                )
            })?;
            let again =
                load.add_experience(NewExperience::new(SHRINE, ExperienceSourceType::Quest, 300))?;
            ADDED.lock().unwrap().extend([shrine, odd, again]);
            if let Err(off) = load.add_experience(NewExperience::new(
                Id::new("vanilla:quest-xp"),
                ExperienceSourceType::Quest,
                1,
            )) {
                REFUSED.lock().unwrap().push(off.to_string());
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lore,
        r#"
        id = "you.lore"
        side = "both"
        "#
    );

    fn built() -> Value {
        let loader = FakeHost::new().progression().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Lore)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let host = enshwick::host().unwrap();
        let key = Guid::parse("82706b40-61b1-4b8f-8b23-dcec6971bda1")
            .key_by_hash(host.hash("balancing_table").unwrap());
        let base = host.game_resource(key).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        Registry::game()
            .unwrap()
            .decode("demo::BalancingTable", &bytes)
            .unwrap()
    }

    fn uint(value: Option<&Value>) -> u64 {
        match value {
            Some(Value::Uint(n)) => *n,
            other => panic!("not a number: {other:?}"),
        }
    }

    #[test]
    fn mod_values_follow_the_game_s_and_vanilla_ones_list_by_source_and_gain() {
        let table = built();
        let added = ADDED.lock().unwrap().clone();
        assert_eq!(added[0].get(), SHRINE.guid().fnv1a());
        assert_eq!(added[2], added[0], "the same id, the same value");
        let Some(Value::List(values)) = table.field("experienceValues") else {
            panic!("{table:?}")
        };
        let rows: Vec<(u64, Value, u64)> = values
            .iter()
            .map(|value| {
                (
                    uint(
                        value
                            .field("experienceValueId")
                            .and_then(|id| id.field("value")),
                    ),
                    value.field("source").cloned().unwrap(),
                    uint(value.field("experienceGain")),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (VANILLA_EXPERIENCE.into(), Value::Name("Mining".into()), 5),
                (
                    VANILLA_QUEST_EXPERIENCE.into(),
                    Value::Name("Quest".into()),
                    600
                ),
                (added[0].get().into(), Value::Name("Quest".into()), 300),
                (added[1].get().into(), Value::Uint(9), 7),
            ]
        );

        let vanilla = VANILLA.lock().unwrap().clone();
        assert_eq!(
            vanilla,
            [
                Experience {
                    value: ExperienceValue::new(VANILLA_EXPERIENCE),
                    source: ExperienceSourceType::Mining,
                    gain: 5,
                },
                Experience {
                    value: ExperienceValue::new(VANILLA_QUEST_EXPERIENCE),
                    source: ExperienceSourceType::Quest,
                    gain: 600,
                },
            ]
        );

        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert!(refused[0].contains("the game holds it"), "{}", refused[0]);
    }
}
