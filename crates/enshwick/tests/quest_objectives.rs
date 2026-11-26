//! Quest steps with objectives: each objective gets a counter the journal
//! shows, and the events the mod already gets raise it while its step is
//! open, up to its count.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::abi::{BuildingData, KnowledgeInfo, PlayerInfo, Status};
    use enshwick::content::journal::{KillTarget, NewQuest, Objective, QuestStep};
    use enshwick::content::knowledge::{GAME_KNOWLEDGE, KnowledgeQuery};
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::events::BuildingPlaced;
    use enshwick::knowledge::{KnowledgeId, Scope};
    use enshwick::quests::{Holder, Quests};
    use enshwick::test::FakeHost;
    use enshwick::typed::FactionType;
    use enshwick::{Entity, ItemId, Load, Mod, Off, PlayerKey};

    const BEACONS: Id = Id::new("you.beacon:beacons");
    /// A quest refused for its second step's count of 0.
    const BROKEN: Id = Id::new("you.beacon:broken");
    /// A quest whose kills by faction this fake cannot count.
    const HUNTED: Id = Id::new("you.beacon:hunted");
    const SIGNAL: Id = Id::new("you.beacon:lit");
    const WALL: ItemId = ItemId::new(0x0000_0a11);
    const PLAYER: Entity = Entity::from_engine(4096);

    static QUESTS: OnceLock<Quests> = OnceLock::new();
    static BROKEN_REFUSED: OnceLock<bool> = OnceLock::new();

    fn tag(name: &str) -> Guid {
        Guid::named(&format!("you.beacon:{name}"))
    }

    fn owned(part: &'static str) -> KnowledgeId {
        KnowledgeId::from_guid(BEACONS.part(part).guid())
    }

    struct Beacon;

    impl Mod for Beacon {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_quest(NewQuest::new(
                BEACONS,
                tag("name"),
                vec![
                    {
                        let mut value = QuestStep::owned(tag("1"), tag("1.text"));
                        value.objectives = vec![Objective::Custom {
                            signal: SIGNAL,
                            count: 3,
                        }];
                        value
                    },
                    {
                        let mut value = QuestStep::owned(tag("2"), tag("2.text"));
                        value.objectives = vec![
                            Objective::Build {
                                item: WALL,
                                count: 2,
                            },
                            Objective::Custom {
                                signal: SIGNAL,
                                count: 1,
                            },
                        ];
                        value
                    },
                ],
            ))?;
            let custom = |count| {
                let mut value = QuestStep::owned(tag("b"), tag("b.text"));
                value.objectives = vec![Objective::Custom {
                    signal: SIGNAL,
                    count,
                }];
                value
            };
            let broken = load.add_quest(NewQuest::new(
                BROKEN,
                tag("broken"),
                vec![custom(1), custom(0)],
            ));
            let _ = BROKEN_REFUSED.set(broken.is_err());
            load.add_quest(NewQuest::new(
                HUNTED,
                tag("hunted"),
                vec![{
                    let mut value = QuestStep::owned(tag("h"), tag("h.text"));
                    value.objectives = vec![Objective::Kill {
                        target: KillTarget::Faction(FactionType::Vukah),
                        count: 1,
                    }];
                    value
                }],
            ))?;
            let _ = QUESTS.set(load.quests());
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Beacon,
        r#"
        id = "you.beacon"
        side = "both"
        "#
    );

    fn info(id: KnowledgeId, kind: u8, value: u32) -> (Option<Entity>, KnowledgeInfo) {
        let info = enshwick::abi::headed!(KnowledgeInfo {
            id,
            kind,
            scope: KnowledgeInfo::PLAYER,
            reserved: 0,
            value,
            flags: 0,
        });
        (Some(PLAYER), info)
    }

    /// The fake started with the player's entries of the quest, each at the
    /// value `set` gives it, else 0.
    fn start(set: &[(&'static str, u32)]) -> (enshwick::test::Installed, &'static Quests) {
        let loader = FakeHost::new()
            .progression()
            .world()
            .player(enshwick::abi::headed!(PlayerInfo {
                slot: 0,
                entity: PLAYER,
                key: PlayerKey::new(7),
                permissions: 0,
                name_len: 0,
                name: [0; PlayerInfo::NAME_MAX],
            }))
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Beacon)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader.set_knowledge(&values(set), &[]);
        (loader, QUESTS.get().unwrap())
    }

    /// The player's entries of the quest, each at the value `set` gives it,
    /// else 0.
    fn values(set: &[(&'static str, u32)]) -> Vec<(Option<Entity>, KnowledgeInfo)> {
        let value = |part| set.iter().find(|(p, _)| *p == part).map_or(0, |(_, v)| *v);
        let mut values = vec![info(
            KnowledgeId::from_guid(BEACONS.guid()),
            KnowledgeInfo::BOOL,
            0,
        )];
        for part in ["started", "step-1.done"] {
            values.push(info(owned(part), KnowledgeInfo::BOOL, value(part)));
        }
        for part in ["step-0.obj-0", "step-1.obj-0", "step-1.obj-1"] {
            values.push(info(owned(part), KnowledgeInfo::COUNTER, value(part)));
        }
        values
    }

    fn added(id: KnowledgeId, amount: u32) -> (Option<Entity>, KnowledgeId, u32, u32) {
        (Some(PLAYER), id, u32::from(KnowledgeInfo::COUNTER), amount)
    }

    fn placed(item: ItemId) -> BuildingData {
        enshwick::abi::headed!(BuildingData {
            entity: Entity::from_engine(77),
            character: PLAYER,
            item,
            kind: BuildingData::PLACED,
            replay: 0,
            template: Guid([0; 16]),
            position: Default::default(),
        })
    }

    #[test]
    fn each_objective_gets_a_counter_the_journal_counts_and_one_objective_is_the_step_s_end() {
        let (_loader, quests) = start(&[]);
        let quest = quests
            .quest(KnowledgeId::from_guid(BEACONS.guid()))
            .unwrap();
        let first = &quest.steps[0];
        let counted = KnowledgeQuery::at_least(owned("step-0.obj-0"), Scope::Player, 3);
        assert_eq!(first.progress, [counted]);
        assert_eq!(
            first.done_when,
            Some(counted),
            "one objective ends the step"
        );
        let second = &quest.steps[1];
        assert_eq!(second.progress.len(), 2);
        assert_eq!(
            second.done_when,
            Some(KnowledgeQuery::is_set(owned("step-1.done"), Scope::Player)),
            "two objectives end it on its own bool"
        );
        assert_eq!(
            second.shown_when,
            Some(counted),
            "shown once the first is done"
        );
    }

    #[test]
    fn a_signal_counts_only_while_its_step_is_open_and_never_past_its_count() {
        let (loader, quests) = start(&[]);
        quests.signal(Holder::player(PLAYER), SIGNAL, 1);
        assert!(loader.grants().is_empty(), "not started");
        drop(loader);
        let (loader, quests) = start(&[("started", 1), ("step-0.obj-0", 1)]);
        quests.signal(Holder::player(PLAYER), SIGNAL, 5);
        assert_eq!(
            loader.grants(),
            [added(owned("step-0.obj-0"), 2)],
            "up to 3"
        );
        quests.signal(Holder::World, SIGNAL, 1);
        assert_eq!(
            loader.grants().len(),
            1,
            "a player's quest takes no world signal"
        );
    }

    #[test]
    fn a_placed_item_counts_and_the_last_objective_sets_the_step_s_bool() {
        let (loader, _quests) = start(&[
            ("started", 1),
            ("step-0.obj-0", 3),
            ("step-1.obj-0", 1),
            ("step-1.obj-1", 1),
        ]);
        assert_eq!(loader.raise::<BuildingPlaced>(&placed(ItemId::new(9))), 1);
        assert!(loader.grants().is_empty(), "another item");
        let _replayed = loader.raise::<BuildingPlaced>(&enshwick::abi::headed!(BuildingData {
            replay: 1,
            ..placed(WALL)
        }));
        assert!(loader.grants().is_empty(), "an undo replay counts nothing");
        assert_eq!(loader.raise::<BuildingPlaced>(&placed(WALL)), 1);
        assert_eq!(
            loader.grants(),
            [
                added(owned("step-1.obj-0"), 1),
                (
                    Some(PLAYER),
                    owned("step-1.done"),
                    u32::from(KnowledgeInfo::BOOL),
                    0
                ),
            ]
        );
    }

    #[test]
    fn two_counters_raised_in_one_world_step_set_the_step_s_bool_on_the_next_tick() {
        let (loader, quests) = start(&[("started", 1), ("step-0.obj-0", 3), ("step-1.obj-0", 1)]);
        assert_eq!(loader.raise::<BuildingPlaced>(&placed(WALL)), 1);
        quests.signal(Holder::player(PLAYER), SIGNAL, 1);
        assert_eq!(
            loader.grants(),
            [
                added(owned("step-1.obj-0"), 1),
                added(owned("step-1.obj-1"), 1)
            ],
            "each read the other's counter before its grant applied"
        );
        let _ran = loader.tick(Duration::from_secs(1));
        assert_eq!(loader.grants().len(), 2, "the grants have not applied yet");
        let applied = values(&[
            ("started", 1),
            ("step-0.obj-0", 3),
            ("step-1.obj-0", 2),
            ("step-1.obj-1", 1),
        ]);
        loader.set_knowledge(&applied, &[]);
        let _ran = loader.tick(Duration::from_secs(1));
        assert_eq!(
            loader.grants().last(),
            Some(&(
                Some(PLAYER),
                owned("step-1.done"),
                u32::from(KnowledgeInfo::BOOL),
                0
            )),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_kind_of_objective_the_build_cannot_count_is_logged_and_the_quest_stays() {
        let (loader, quests) = start(&[]);
        assert!(
            quests
                .quest(KnowledgeId::from_guid(HUNTED.guid()))
                .is_some()
        );
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("some objectives do not count")),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_refused_quest_serves_no_knowledge_of_its_own() {
        let (loader, _quests) = start(&[]);
        assert_eq!(BROKEN_REFUSED.get(), Some(&true));
        let host = enshwick::host().unwrap();
        let key = GAME_KNOWLEDGE.key_by_hash(host.hash("GameKnowledgeResource").unwrap());
        let base = host.game_resource(key).unwrap();
        let built = loader.build_resource(&key, &base).unwrap();
        let holds = |id: Id, part| {
            let key = KnowledgeId::from_guid(id.part(part).guid()).get();
            built.windows(4).any(|bytes| bytes == key.to_le_bytes())
        };
        assert!(
            holds(BEACONS, "step-0.obj-0"),
            "the search finds a served one"
        );
        for part in ["started", "step-0.obj-0", "step-1.obj-0"] {
            assert!(!holds(BROKEN, part), "{part} left behind");
        }
    }
}
