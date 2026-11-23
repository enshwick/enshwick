//! Quest steps with objectives: each objective gets a counter the journal
//! shows, and the events the mod already gets raise it while its step is
//! open, up to its count.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{BuildingData, Header, KnowledgeInfo, PlayerInfo, Status};
    use enshwick::content::journal::{NewQuest, Objective, QuestStep};
    use enshwick::content::knowledge::KnowledgeQuery;
    use enshwick::content::{Guid, Id};
    use enshwick::events::BuildingPlaced;
    use enshwick::knowledge::{KnowledgeId, Scope};
    use enshwick::quests::{Holder, Quests};
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId, Load, Mod, Off, PlayerKey};

    const BEACONS: Id = Id::new("you.beacon:beacons");
    const SIGNAL: Id = Id::new("you.beacon:lit");
    const WALL: ItemId = ItemId::new(0x0000_0a11);
    const PLAYER: Entity = Entity(4096);

    static QUESTS: OnceLock<Quests> = OnceLock::new();

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
                    QuestStep {
                        objectives: vec![Objective::Custom {
                            signal: SIGNAL,
                            count: 3,
                        }],
                        ..QuestStep::owned(tag("1"), tag("1.text"))
                    },
                    QuestStep {
                        objectives: vec![
                            Objective::Build {
                                item: WALL,
                                count: 2,
                            },
                            Objective::Custom {
                                signal: SIGNAL,
                                count: 1,
                            },
                        ],
                        ..QuestStep::owned(tag("2"), tag("2.text"))
                    },
                ],
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
        let info = KnowledgeInfo {
            header: Header::of::<KnowledgeInfo>(),
            id,
            kind,
            scope: KnowledgeInfo::PLAYER,
            reserved: 0,
            value,
            flags: 0,
        };
        (Some(PLAYER), info)
    }

    /// The fake started with the player's entries of the quest, each at the
    /// value `set` gives it, else 0.
    fn start(set: &[(&'static str, u32)]) -> (enshwick::test::Installed, &'static Quests) {
        let loader = FakeHost::new()
            .progression()
            .world()
            .player(PlayerInfo {
                header: Header::of::<PlayerInfo>(),
                slot: 0,
                entity: PLAYER,
                key: PlayerKey::new(7),
                permissions: 0,
                name_len: 0,
                name: [0; PlayerInfo::NAME_MAX],
            })
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Beacon)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
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
        loader.set_knowledge(&values, &[]);
        (loader, QUESTS.get().unwrap())
    }

    fn added(id: KnowledgeId, amount: u32) -> (Option<Entity>, KnowledgeId, u32, u32) {
        (Some(PLAYER), id, u32::from(KnowledgeInfo::COUNTER), amount)
    }

    fn placed(item: ItemId) -> BuildingData {
        BuildingData {
            header: Header::of::<BuildingData>(),
            entity: Entity(77),
            character: PLAYER,
            item,
            kind: BuildingData::PLACED,
            replay: 0,
            template: Guid([0; 16]),
            position: Default::default(),
        }
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
        quests.signal(Holder::Player(PLAYER), SIGNAL, 1);
        assert!(loader.grants().is_empty(), "not started");
        drop(loader);
        let (loader, quests) = start(&[("started", 1), ("step-0.obj-0", 1)]);
        quests.signal(Holder::Player(PLAYER), SIGNAL, 5);
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
        let _replayed = loader.raise::<BuildingPlaced>(&BuildingData {
            replay: 1,
            ..placed(WALL)
        });
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
}
