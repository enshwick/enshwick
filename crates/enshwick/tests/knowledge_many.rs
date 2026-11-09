//! A mod adding many knowledge entries and quests grows the game's
//! knowledge and journal by what it adds, not by a copy per entry: one
//! patch per resource serves all of a mod's.

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use enshwick::abi::Status;
    use enshwick::content::journal::{JOURNAL_REGISTRY, NewQuest, QuestStep};
    use enshwick::content::knowledge::{GAME_KNOWLEDGE, KnowledgeQuery, NewKnowledge};
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::knowledge::{KnowledgeType, Scope};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const ENTRIES: usize = 100;
    const QUESTS: usize = 50;

    fn id(kind: &str, n: usize) -> Id {
        Id::new(Box::leak(format!("you.many:{kind}{n}").into_boxed_str()))
    }

    struct Many;

    impl Mod for Many {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            for n in 0..ENTRIES {
                load.add_knowledge(NewKnowledge::new(
                    id("entry", n),
                    Scope::Player,
                    KnowledgeType::Bool,
                ))?;
            }
            for n in 0..QUESTS {
                let done = KnowledgeQuery::is_set(
                    enshwick::knowledge::KnowledgeId::from_guid(id("entry", n).guid()),
                    Scope::Player,
                );
                let step = QuestStep::new(Guid([1; 16]), Guid([2; 16]), done, done);
                load.add_quest(NewQuest::new(id("quest", n), Guid([3; 16]), vec![step]))?;
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Many,
        r#"
        id = "you.many"
        side = "both"
        "#
    );

    /// The base's bytes, the built bytes and the time the build took.
    fn grown(loader: &enshwick::test::Installed, guid: Guid, id: &str) -> (usize, usize, f64) {
        let host = enshwick::host().unwrap();
        let key = guid.key_by_hash(host.hash(id).unwrap());
        let base = host.game_resource(key).unwrap();
        let start = Instant::now();
        let built = loader.build_resource(&key, &base).unwrap();
        let took = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "{id}: {} -> {} bytes in {took:.2} ms",
            base.len(),
            built.len()
        );
        (base.len(), built.len(), took)
    }

    #[test]
    fn many_entries_and_quests_grow_each_resource_by_what_they_add() {
        let loader = FakeHost::new().progression().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Many)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let knowledge = grown(&loader, GAME_KNOWLEDGE, "GameKnowledgeResource");
        let journal = grown(&loader, JOURNAL_REGISTRY, "JournalRegistryResource");
        let (base, built, _) = knowledge;
        // An entry is a key, a value and the map's arrays moved once.
        let each = (built - base) / (ENTRIES + QUESTS);
        assert!(each < 128, "{each} bytes an entry, {base} -> {built}");
        let (base, built, _) = journal;
        let each = (built - base) / QUESTS;
        assert!(each < 1024, "{each} bytes a quest, {base} -> {built}");
    }
}
