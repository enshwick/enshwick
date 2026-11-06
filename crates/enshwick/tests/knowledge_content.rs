//! Mods add knowledge entries of the world and of each player beside the
//! game's, with skill points, found by the game's own lookup; a collision
//! and an entry that does not fit its kind are refused.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::content::knowledge::{ExperienceValue, GAME_KNOWLEDGE, NewKnowledge};
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::knowledge::{KnowledgeId, KnowledgeType, Scope};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{FakeHost, VANILLA_PLAYER_KNOWLEDGE, VANILLA_WORLD_KNOWLEDGE};
    use enshwick::{Load, Mod, Off};

    const SHRINE: Id = Id::new("you.lore:shrine");
    const KILLS: Id = Id::new("you.lore:kills");
    const MARKS: Id = Id::new("you.lore:marks");

    static ADDED: Mutex<Vec<KnowledgeId>> = Mutex::new(Vec::new());
    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Two content ids of this mod whose game ids are one.
    fn clashing() -> (Id, Id) {
        let mut seen = rustc_hash::FxHashMap::default();
        for i in 0u32.. {
            let name: &'static str = Box::leak(format!("you.lore:n{i}").into_boxed_str());
            if let Some(first) = seen.insert(Guid::named(name).fnv1a(), name) {
                return (Id::new(first), Id::new(name));
            }
        }
        unreachable!("a 32-bit hash repeats")
    }

    fn refused(result: Result<impl Sized, Off>) {
        if let Err(off) = result {
            REFUSED.lock().unwrap().push(off.to_string());
        }
    }

    struct Lore;

    impl Mod for Lore {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let added = [
                load.add_knowledge(NewKnowledge {
                    skill_points: 2,
                    experience: Some(ExperienceValue::new(77)),
                    ..NewKnowledge::new(SHRINE, Scope::Player, KnowledgeType::Bool)
                })?,
                load.add_knowledge(NewKnowledge::new(
                    KILLS,
                    Scope::World,
                    KnowledgeType::Counter,
                ))?,
                load.add_knowledge(NewKnowledge {
                    flags: 0b111,
                    ..NewKnowledge::new(MARKS, Scope::Player, KnowledgeType::Flags)
                })?,
            ];
            ADDED.lock().unwrap().extend(added);
            refused(load.add_knowledge(NewKnowledge {
                skill_points: 1,
                ..NewKnowledge::new(
                    Id::new("you.lore:world-points"),
                    Scope::World,
                    KnowledgeType::Bool,
                )
            }));
            refused(load.add_knowledge(NewKnowledge::new(
                Id::new("you.lore:no-bits"),
                Scope::Player,
                KnowledgeType::Flags,
            )));
            let (first, second) = clashing();
            for id in [first, second] {
                refused(load.add_knowledge(NewKnowledge::new(
                    id,
                    Scope::Player,
                    KnowledgeType::Bool,
                )));
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
        let key = GAME_KNOWLEDGE.key_by_hash(host.hash("GameKnowledgeResource").unwrap());
        let base = host.game_resource(key).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        Registry::game()
            .unwrap()
            .decode("demo::GameKnowledgeResource", &bytes)
            .unwrap()
    }

    fn uint(value: Option<&Value>) -> u64 {
        match value {
            Some(Value::Uint(n)) => *n,
            other => panic!("not a number: {other:?}"),
        }
    }

    fn key(value: &Value) -> u32 {
        u32::try_from(uint(value.field("id").and_then(|id| id.field("value")))).unwrap()
    }

    /// The entry the game's lookup finds for `id` in `map`: the run of its
    /// bucket, the id masked by the bucket count less one.
    fn find<'a>(map: &'a Value, id: u32) -> Option<&'a Value> {
        let (Some(Value::List(entries)), Some(Value::List(keys)), Some(Value::List(values))) =
            (map.field("entries"), map.field("keys"), map.field("values"))
        else {
            panic!("{map:?}")
        };
        let bucket = &entries[id as usize & (entries.len() - 1)];
        let start = usize::try_from(uint(bucket.field("start"))).unwrap();
        let count = usize::try_from(uint(bucket.field("count"))).unwrap();
        (start..start + count)
            .find(|&i| key(&keys[i]) == id)
            .map(|i| &values[i])
    }

    #[test]
    fn mod_entries_join_the_game_s_maps_and_the_game_s_own_lookup_finds_every_one() {
        let knowledge = built();
        let added = ADDED.lock().unwrap().clone();
        assert_eq!(added[0], KnowledgeId::from_guid(SHRINE.guid()));
        let world = knowledge.field("worldKnowledge").unwrap();
        let player = knowledge.field("playerKnowledge").unwrap();
        assert!(
            find(world, VANILLA_WORLD_KNOWLEDGE).is_some(),
            "vanilla kept"
        );
        assert!(
            find(player, VANILLA_PLAYER_KNOWLEDGE).is_some(),
            "vanilla kept"
        );

        let shrine = find(player, added[0].get()).expect("the player's bool");
        assert_eq!(shrine.field("type"), Some(&Value::Name("Bool".into())));
        assert_eq!(shrine.field("isPlayerKnowledge"), Some(&Value::Bool(true)));
        assert_eq!(uint(shrine.field("unlockedSkillPoints")), 2);
        assert_eq!(
            uint(
                shrine
                    .field("experienceGainId")
                    .and_then(|x| x.field("value"))
            ),
            77
        );
        let kills = find(world, added[1].get()).expect("the world's counter");
        assert_eq!(kills.field("type"), Some(&Value::Name("Counter".into())));
        assert!(find(player, added[1].get()).is_none(), "the world's only");
        let marks = find(player, added[2].get()).expect("the player's flags");
        assert_eq!(uint(marks.field("expectedFlags")), 0b111);

        let Some(Value::List(points)) = knowledge.field("skillPoints") else {
            panic!("{knowledge:?}")
        };
        let ids: Vec<u32> = points
            .iter()
            .map(|unlock| key(unlock.field("knowledgeId").unwrap()))
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "ordered by id");
        assert!(ids.contains(&added[0].get()) && ids.contains(&VANILLA_PLAYER_KNOWLEDGE));

        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 3, "{refused:?}");
        assert!(refused[0].contains("only a player's"), "{}", refused[0]);
        assert!(refused[1].contains("flags need"), "{}", refused[1]);
        assert!(
            refused[2].contains("content id"),
            "a collision: {}",
            refused[2]
        );
    }
}
