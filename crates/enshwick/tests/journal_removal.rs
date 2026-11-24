//! Mods remove a vanilla quest and a vanilla lore page from the journal,
//! steps and entries with them.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::journal::{EntryId, JOURNAL_REGISTRY};
    use enshwick::content::{EngineFacts, Id};
    use enshwick::knowledge::KnowledgeId;
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{FakeHost, VANILLA_QUEST};
    use enshwick::{Load, Mod, Off};

    /// The fake journal's one lore page and its quest's one step.
    const PAGE: EntryId = EntryId::new(0x0000_0507);
    const STEP: u32 = 0x0000_0501;

    struct Removal;

    impl Mod for Removal {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
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

    #[test]
    fn a_removed_quest_and_page_leave_no_entry_id_behind() {
        let loader = FakeHost::new().progression().world().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Removal)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let host = enshwick::host().unwrap();
        let key = JOURNAL_REGISTRY.key_by_hash(host.hash("JournalRegistryResource").unwrap());
        let base = host.game_resource(key).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        let journal = Registry::game()
            .unwrap()
            .decode("demo::JournalRegistryResource", &bytes)
            .unwrap();
        let mut held = Vec::new();
        ids(&journal, &mut held);
        for gone in [VANILLA_QUEST, STEP, PAGE.get()] {
            assert!(!held.contains(&gone), "{gone:#x} in {held:?}");
        }
    }
}
