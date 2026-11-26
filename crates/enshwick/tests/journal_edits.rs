//! Mods change every journal field of a vanilla quest and lore page, insert
//! steps with entry ids of their own, and point a step at a map marker.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::journal::{
        EntryId, JOURNAL_REGISTRY, PageChange, QuestChange, QuestStep, StepChange, StepEdit,
        StepInsert, StepMove,
    };
    use enshwick::content::knowledge::KnowledgeQuery;
    use enshwick::content::{EngineFacts, Guid};
    use enshwick::knowledge::{KnowledgeId, Scope};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{FakeHost, VANILLA_QUEST};
    use enshwick::{Load, Mod, Off};

    /// The fake journal's one lore page.
    const PAGE: EntryId = EntryId::new(0x0000_0507);

    fn tag(name: &str) -> Guid {
        Guid::named(&format!("you.edits:{name}"))
    }

    fn is(id: u32) -> KnowledgeQuery {
        KnowledgeQuery::is_set(KnowledgeId::new(id), Scope::Player)
    }

    struct Edits;

    impl Mod for Edits {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let vanilla = KnowledgeId::new(VANILLA_QUEST);
            load.change_quest(vanilla, {
                let mut value = QuestChange::default();
                value.name = Some(tag("renamed"));
                value.main = Some(true);
                value.priority = Some(5);
                value.steps = vec![
                    StepEdit::Insert(StepInsert::new(0, {
                        let mut value =
                            QuestStep::new(tag("first"), tag("first.text"), is(1), is(2));
                        value.marker = Some(tag("camp"));
                        value
                    })),
                    StepEdit::Move(StepMove { from: 0, to: 1 }),
                    StepEdit::Change({
                        let mut value = StepChange::default();
                        value.step = 0;
                        value.level = Some(9);
                        value
                    }),
                ];
                value
            })?;
            load.change_journal_page(PAGE, {
                let mut value = PageChange::default();
                value.name = Some(tag("page"));
                value.tutorial = Some(true);
                value.entries = vec![StepEdit::Insert(StepInsert::new(0, {
                    let mut value = QuestStep::owned(tag("entry"), tag("entry.text"));
                    value.shown_when = Some(is(3));
                    value
                }))];
                value
            })?;
            for refused in [
                load.change_journal_page(EntryId::new(1), PageChange::default()),
                load.change_journal_page(PAGE, {
                    let mut value = PageChange::default();
                    value.category = Some(tag("own-category"));
                    value
                }),
                load.change_quest(vanilla, {
                    let mut value = QuestChange::default();
                    value.steps = vec![StepEdit::Remove(3)];
                    value
                }),
            ] {
                assert!(refused.is_err());
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Edits,
        r#"
        id = "you.edits"
        side = "both"
        "#
    );

    fn journal() -> Value {
        let loader = FakeHost::new().progression().world().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Edits)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let host = enshwick::host().unwrap();
        let key = JOURNAL_REGISTRY.key_by_hash(host.hash("JournalRegistryResource").unwrap());
        let base = host.game_resource(key).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        Registry::game()
            .unwrap()
            .decode("demo::JournalRegistryResource", &bytes)
            .unwrap()
    }

    fn key(value: Option<&Value>) -> Option<u32> {
        match value.and_then(|v| v.field("value")) {
            Some(Value::Uint(n)) => u32::try_from(*n).ok(),
            _ => None,
        }
    }

    fn list(value: Option<&Value>) -> &[Value] {
        match value {
            Some(Value::List(items)) => items,
            _ => &[],
        }
    }

    #[test]
    fn a_vanilla_quest_is_renamed_and_gains_a_step_that_points_at_a_map_marker() {
        let journal = journal();
        let quest = &list(journal.field("quests"))[0];
        assert_eq!(key(quest.field("entryId")), Some(VANILLA_QUEST));
        assert_eq!(key(quest.field("name")), Some(tag("renamed").fnv1a()));
        assert_eq!(quest.field("isMainQuest"), Some(&Value::Bool(true)));
        assert_eq!(quest.field("priority"), Some(&Value::Uint(5)));
        let steps = list(quest.field("entries"));
        assert_eq!(steps.len(), 2);
        assert_eq!(
            key(steps[0].field("entryId")),
            Some(0x0000_0501),
            "the game's step first"
        );
        assert_eq!(steps[0].field("recommendedLevel"), Some(&Value::Uint(9)));
        let added = &steps[1];
        assert_eq!(key(added.field("name")), Some(tag("first").fnv1a()));
        assert_eq!(
            key(added.field("mapMarkerReference")),
            Some(tag("camp").fnv1a())
        );
        let id = key(added.field("entryId")).unwrap();
        assert!(id != 0 && id != 0x0000_0501 && id != VANILLA_QUEST);
    }

    #[test]
    fn a_vanilla_page_changes_its_fields_and_gains_an_entry_that_never_completes() {
        let journal = journal();
        let page = &list(journal.field("collections"))[0];
        assert_eq!(key(page.field("entryId")), Some(PAGE.get()));
        assert_eq!(key(page.field("name")), Some(tag("page").fnv1a()));
        assert_eq!(page.field("isTutorial"), Some(&Value::Bool(true)));
        let entries = list(page.field("entries"));
        assert_eq!(entries.len(), 1);
        let done = entries[0].field("completionRequirement").unwrap();
        assert_eq!(done.field("type"), Some(&Value::Name("Extern".into())));
        assert_eq!(key(done.field("knowledgeOrQueryId")), Some(0));
    }
}
