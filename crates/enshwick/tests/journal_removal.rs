//! Mods remove a vanilla quest and a vanilla lore page from the journal,
//! steps and entries with them, and keep a quest another mod's step or lore
//! page entry asks for through a game query.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Level, Status};
    use enshwick::content::journal::{EntryId, JOURNAL_REGISTRY};
    use enshwick::content::knowledge::{KnowledgeQuery, NewQuery, QueryForm};
    use enshwick::content::{EngineFacts, Id};
    use enshwick::knowledge::{KnowledgeId, Scope};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{FakeHost, VANILLA_QUEST};
    use enshwick::{Load, Mod, Off};

    /// The fake journal's one lore page and its quest's one step.
    const PAGE: EntryId = EntryId::new(0x0000_0507);
    const STEP: u32 = 0x0000_0501;
    /// A query of the mod's own that reads the vanilla quest.
    const ASKS: Id = Id::new("you.removal:asks");
    /// The quest of another mod, earlier in load order, and its step.
    const OTHER: u32 = 0x0000_0a00;
    const OTHER_STEP: u32 = 0x0000_0a01;
    const JOURNAL: &str = "demo::JournalRegistryResource";

    struct Removal;

    impl Mod for Removal {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_query(NewQuery::new(
                ASKS,
                Scope::Player,
                QueryForm::All,
                vec![KnowledgeQuery::is_set(
                    KnowledgeId::new(VANILLA_QUEST),
                    Scope::Player,
                )],
            ))?;
            load.remove_quest(KnowledgeId::new(VANILLA_QUEST))?;
            load.remove_journal_page(PAGE)?;
            assert!(load.remove_quest(KnowledgeId::new(1)).is_err());
            assert!(load.remove_journal_page(EntryId::new(1)).is_err());
            let own = KnowledgeId::from_guid(Id::new("you.removal:own").guid());
            assert!(load.remove_quest(own).is_err(), "only the game's quests");
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Removal,
        r#"
        id = "you.removal"
        side = "both"
        "#
    );

    fn ids(value: &Value, out: &mut Vec<u32>) {
        match value {
            Value::Struct(fields) => {
                for (name, field) in fields {
                    if name == "entryId"
                        && let Some(Value::Uint(id)) = field.field("value")
                    {
                        out.push(u32::try_from(*id).unwrap());
                    }
                    ids(field, out);
                }
            }
            Value::List(items) => items.iter().for_each(|item| ids(item, out)),
            _ => {}
        }
    }

    fn set(value: &mut Value, path: &[&str], new: Value) {
        let mut at = value;
        for name in path {
            at = at.field_mut(name).unwrap();
        }
        *at = new;
    }

    fn list<'a>(value: &'a mut Value, name: &str) -> &'a mut Vec<Value> {
        match value.field_mut(name) {
            Some(Value::List(items)) => items,
            other => panic!("{name}: {other:?}"),
        }
    }

    /// Starts the mod and builds the journal on the game's, as `edit` leaves
    /// it; returns the entry ids it holds and the loader's log.
    fn journal(edit: impl FnOnce(&mut Value)) -> (Vec<u32>, Vec<(Level, String)>) {
        let loader = FakeHost::new().progression().world().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Removal)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let host = enshwick::host().unwrap();
        let key = JOURNAL_REGISTRY.key_by_hash(host.hash("JournalRegistryResource").unwrap());
        let game = host.game_resource(key).unwrap();
        let registry = Registry::game().unwrap();
        let mut base = registry.decode(JOURNAL, &game).unwrap();
        edit(&mut base);
        let base = registry.encode(JOURNAL, &base, Some(&game)).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        let mut held = Vec::new();
        ids(&registry.decode(JOURNAL, &bytes).unwrap(), &mut held);
        (held, loader.logs())
    }

    #[test]
    fn a_removed_quest_and_page_leave_no_entry_id_behind() {
        let (held, _) = journal(|_| {});
        for gone in [VANILLA_QUEST, STEP, PAGE.get()] {
            assert!(!held.contains(&gone), "{gone:#x} in {held:?}");
        }
    }

    #[test]
    fn a_quest_another_mod_s_step_asks_for_through_a_mod_query_stays() {
        let (held, logs) = journal(|base| {
            let mut other = list(base, "quests")[0].clone();
            set(&mut other, &["entryId", "value"], Value::Uint(OTHER.into()));
            let step = &mut list(&mut other, "entries")[0];
            set(step, &["entryId", "value"], Value::Uint(OTHER_STEP.into()));
            set(
                step,
                &["knowledgeRequirement", "knowledgeOrQueryId", "value"],
                Value::Uint(ASKS.guid().fnv1a().into()),
            );
            set(
                step,
                &["knowledgeRequirement", "type"],
                Value::Name("Extern".into()),
            );
            list(base, "quests").push(other);
        });
        assert!(held.contains(&VANILLA_QUEST), "{held:?}");
        assert!(held.contains(&OTHER), "{held:?}");
        assert!(!held.contains(&PAGE.get()), "the page still goes");
        assert!(
            logs.iter().any(|(level, line)| *level == Level::WARN
                && line.contains(&format!(
                    "another mod's step in the quest {OTHER} asks for it"
                ))),
            "{logs:?}"
        );
    }

    #[test]
    fn a_quest_another_mod_s_lore_page_entry_asks_for_through_a_mod_query_stays() {
        let (held, logs) = journal(|base| {
            let mut entry = list(&mut list(base, "quests")[0], "entries")[0].clone();
            set(
                &mut entry,
                &["entryId", "value"],
                Value::Uint(OTHER_STEP.into()),
            );
            set(
                &mut entry,
                &["knowledgeRequirement", "knowledgeOrQueryId", "value"],
                Value::Uint(ASKS.guid().fnv1a().into()),
            );
            set(
                &mut entry,
                &["knowledgeRequirement", "type"],
                Value::Name("Extern".into()),
            );
            let mut other = list(base, "collections")[0].clone();
            set(&mut other, &["entryId", "value"], Value::Uint(OTHER.into()));
            list(&mut other, "entries").push(entry);
            list(base, "collections").push(other);
        });
        assert!(held.contains(&VANILLA_QUEST), "{held:?}");
        assert!(held.contains(&OTHER), "{held:?}");
        assert!(
            logs.iter().any(|(level, line)| *level == Level::WARN
                && line.contains(&format!(
                    "another mod's entry in the journal page {OTHER} asks for it"
                ))),
            "{logs:?}"
        );
    }
}
