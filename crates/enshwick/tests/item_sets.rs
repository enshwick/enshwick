//! Mods add item sets of their own on queries the loader adds, change a
//! vanilla set with its pay row, and read every set back.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::content::journal::{
        ItemSetCategory, ItemSetChange, ItemSetEntry, ItemSetId, NewItemSet,
    };
    use enshwick::content::knowledge::{ExperienceValue, GameQuery, KnowledgeQuery, KnowledgeTest};
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::knowledge::KnowledgeId;
    use enshwick::quests::{ItemSetInfo, Quests};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{
        FakeHost, ITEM_SET_CATEGORY, QUERY_DATABASE, VANILLA_ITEM_SET, VANILLA_ITEM_SET_REWARD,
        VANILLA_PLAYER_QUERY,
    };
    use enshwick::{ItemId, Load, Mod, Off};

    const SHELLS: Id = Id::new("you.sets:shells");

    static QUESTS: OnceLock<Quests> = OnceLock::new();

    struct Sets;

    impl Mod for Sets {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_item_set({
                let mut value = NewItemSet::new(
                    SHELLS,
                    ITEM_SET_CATEGORY,
                    Guid::named("you.sets:shells.name"),
                    vec![
                        ItemSetEntry::new(ItemId::new(0x51)),
                        ItemSetEntry::new(ItemId::new(0x52)),
                    ],
                );
                value.experience = Some(ExperienceValue::new(0x0e05));
                value
            })?;
            assert!(
                load.add_item_set(NewItemSet::new(
                    Id::new("you.sets:lost"),
                    ItemSetCategory::new(9),
                    Guid([8; 16]),
                    vec![ItemSetEntry::new(ItemId::new(0x53))],
                ))
                .is_err(),
                "a category the game's journal lists no set under"
            );
            let vanilla = ItemSetId::new(VANILLA_ITEM_SET);
            assert!(
                load.change_item_set(vanilla, {
                    let mut value = ItemSetChange::default();
                    value.entries = Some(vec![ItemSetEntry::new(ItemId::new(0x54))]);
                    value
                })
                .is_err(),
                "new entries without done_when"
            );
            assert!(
                load.change_item_set(vanilla, {
                    let mut value = ItemSetChange::default();
                    value.entries = Some(Vec::new());
                    value.done_when =
                        Some(KnowledgeQuery::game(GameQuery::new(VANILLA_PLAYER_QUERY)));
                    value
                })
                .is_err(),
                "a set without entries, as add_item_set refuses"
            );
            load.change_item_set(vanilla, {
                let mut value = ItemSetChange::default();
                value.experience = Some(ExperienceValue::new(0x0e05));
                value
            })?;
            assert!(load.remove_item_set(ItemSetId::new(1)).is_err());
            let _ = QUESTS.set(load.quests());
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Sets,
        r#"
        id = "you.sets"
        side = "both"
        "#
    );

    fn sets() -> (enshwick::test::Installed, Vec<ItemSetInfo>) {
        let loader = FakeHost::new().progression().world().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Sets)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let sets = QUESTS.get().unwrap().item_sets();
        (loader, sets)
    }

    #[test]
    fn a_mod_s_set_completes_on_an_all_of_its_pickups_and_shows_on_one() {
        let (_loader, sets) = sets();
        let set = sets
            .iter()
            .find(|set| set.id == ItemSetId::new(SHELLS.guid().fnv1a()))
            .unwrap_or_else(|| panic!("{sets:?}"));
        assert_eq!(set.entries.len(), 2);
        assert_eq!(
            set.entries[0].done_when,
            Some(KnowledgeQuery::new(
                KnowledgeId::new(0x51),
                enshwick::knowledge::Scope::Player,
                KnowledgeTest::HasFlag(1)
            ))
        );
        let query = |part| GameQuery::new(SHELLS.part(part).guid().fnv1a());
        assert_eq!(set.done_when, Some(KnowledgeQuery::game(query("done"))));
        assert_eq!(set.shown_when, Some(KnowledgeQuery::game(query("shown"))));
        assert_eq!(
            set.rewarded,
            Some(KnowledgeId::from_guid(SHELLS.part("rewarded").guid()))
        );
        assert_eq!(set.experience, Some(ExperienceValue::new(0x0e05)));
    }

    #[test]
    fn the_set_s_queries_join_the_database_as_an_all_of_and_an_any_of_its_entries() {
        let (loader, _sets) = sets();
        let host = enshwick::host().unwrap();
        let key = QUERY_DATABASE.key_by_hash(host.hash("GameKnowledgeQueryResourceDb").unwrap());
        let base = host.game_resource(key).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        let db = Registry::game()
            .unwrap()
            .decode("demo::GameKnowledgeQueryResourceDb", &bytes)
            .unwrap();
        let Some(Value::List(queries)) = db.field("queries") else {
            panic!("{db:?}")
        };
        let form = |part: &'static str| {
            let id = u64::from(SHELLS.part(part).guid().fnv1a());
            queries
                .iter()
                .find(|q| {
                    q.field("queryId")
                        .and_then(|q| q.field("id"))
                        .and_then(|q| q.field("value"))
                        == Some(&Value::Uint(id))
                })
                .and_then(|q| q.field("type"))
                .cloned()
        };
        assert_eq!(form("done"), Some(Value::Name("AND".into())));
        assert_eq!(form("shown"), Some(Value::Name("OR".into())));
    }

    #[test]
    fn a_changed_vanilla_set_keeps_its_queries_and_pay_row() {
        let (_loader, sets) = sets();
        let vanilla = sets
            .iter()
            .find(|set| set.id == ItemSetId::new(VANILLA_ITEM_SET))
            .unwrap_or_else(|| panic!("{sets:?}"));
        assert_eq!(vanilla.experience, Some(ExperienceValue::new(0x0e05)));
        assert_eq!(
            vanilla.done_when,
            Some(KnowledgeQuery::game(GameQuery::new(VANILLA_PLAYER_QUERY)))
        );
        assert_eq!(
            vanilla.rewarded,
            Some(KnowledgeId::new(VANILLA_ITEM_SET_REWARD))
        );
    }
}
