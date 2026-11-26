//! Mods start, advance, complete and reset quests for a player or the
//! world: each call grants or revokes exactly the knowledge the steps ask
//! for, and refuses a step only the game's query moves.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{KnowledgeInfo, PlayerInfo, Status};
    use enshwick::content::journal::{NewQuest, QuestStep};
    use enshwick::content::knowledge::{KnowledgeQuery, KnowledgeTest, NewKnowledge};
    use enshwick::content::{Guid, Id};
    use enshwick::knowledge::{KnowledgeId, KnowledgeType, Scope};
    use enshwick::quests::{Holder, MoveQuestError, Quests};
    use enshwick::test::{FakeHost, VANILLA_PLAYER_KNOWLEDGE, VANILLA_QUEST};
    use enshwick::typed::QuestType;
    use enshwick::{Entity, Load, Mod, Off, PlayerKey, WorldError};

    const HUNT: Id = Id::new("you.hunt:hunt");
    const WATCH: Id = Id::new("you.hunt:watch");
    const PLAYER: Entity = Entity::from_engine(4096);

    static QUESTS: OnceLock<Quests> = OnceLock::new();
    static WOOD: OnceLock<KnowledgeId> = OnceLock::new();

    fn tag(name: &str) -> Guid {
        Guid::named(&format!("you.hunt:{name}"))
    }

    fn wood() -> KnowledgeId {
        KnowledgeId::from_guid(Id::new("you.hunt:wood").guid())
    }

    fn owned(id: Id, part: &'static str) -> KnowledgeId {
        KnowledgeId::from_guid(id.part(part).guid())
    }

    struct Hunt;

    impl Mod for Hunt {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let wood = load.add_knowledge(NewKnowledge::new(
                Id::new("you.hunt:wood"),
                Scope::Player,
                KnowledgeType::Counter,
            ))?;
            load.add_quest(NewQuest::new(
                HUNT,
                tag("hunt.name"),
                vec![
                    QuestStep::owned(tag("1"), tag("1.text")),
                    {
                        let mut value = QuestStep::owned(tag("2"), tag("2.text"));
                        value.done_when = Some(KnowledgeQuery::at_least(wood, Scope::Player, 3));
                        value
                    },
                    QuestStep::owned(tag("3"), tag("3.text")),
                ],
            ))?;
            load.add_quest({
                let mut value = NewQuest::new(
                    WATCH,
                    tag("watch.name"),
                    vec![QuestStep::owned(tag("w"), tag("w.text"))],
                );
                value.kind = QuestType::WorldQuest;
                value
            })?;
            let _ = WOOD.set(wood);
            let _ = QUESTS.set(load.quests());
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Hunt,
        r#"
        id = "you.hunt"
        side = "both"
        "#
    );

    fn info(id: KnowledgeId, kind: u8, scope: u8, value: u32) -> KnowledgeInfo {
        enshwick::abi::headed!(KnowledgeInfo {
            id,
            kind,
            scope,
            reserved: 0,
            value,
            flags: 0,
        })
    }

    fn mine(id: KnowledgeId, kind: u8, value: u32) -> (Option<Entity>, KnowledgeInfo) {
        (Some(PLAYER), info(id, kind, KnowledgeInfo::PLAYER, value))
    }

    fn world(id: KnowledgeId, value: u32) -> (Option<Entity>, KnowledgeInfo) {
        (
            None,
            info(id, KnowledgeInfo::BOOL, KnowledgeInfo::WORLD, value),
        )
    }

    /// The fake started, every entry of the two quests unset but those
    /// `set` names with their values.
    fn start(set: &[(KnowledgeId, u32)]) -> (enshwick::test::Installed, &'static Quests) {
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
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Hunt)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let value = |id| {
            set.iter()
                .find(|(held, _)| *held == id)
                .map_or(0, |(_, v)| *v)
        };

        let mut values = vec![
            mine(KnowledgeId::from_guid(HUNT.guid()), KnowledgeInfo::BOOL, 0),
            mine(wood(), KnowledgeInfo::COUNTER, value(wood())),
            mine(
                KnowledgeId::new(VANILLA_PLAYER_KNOWLEDGE),
                KnowledgeInfo::BOOL,
                value(KnowledgeId::new(VANILLA_PLAYER_KNOWLEDGE)),
            ),
        ];
        for part in ["started", "step-0.done", "step-2.done"] {
            let id = owned(HUNT, part);
            values.push(mine(id, KnowledgeInfo::BOOL, value(id)));
        }
        for part in ["started", "step-0.done"] {
            let id = owned(WATCH, part);
            values.push(world(id, value(id)));
        }
        loader.set_knowledge(&values, &[]);
        (loader, QUESTS.get().unwrap())
    }

    fn bool(id: KnowledgeId) -> (Option<Entity>, KnowledgeId, u32, u32) {
        (Some(PLAYER), id, u32::from(KnowledgeInfo::BOOL), 0)
    }

    #[test]
    fn a_step_left_to_the_loader_gets_its_own_bool_and_shows_once_the_step_before_is_done() {
        let (_loader, quests) = start(&[]);
        let hunt = quests.quest(KnowledgeId::from_guid(HUNT.guid())).unwrap();
        let is = |part| Some(KnowledgeQuery::is_set(owned(HUNT, part), Scope::Player));
        let counted = KnowledgeQuery::at_least(wood(), Scope::Player, 3);
        let steps: Vec<_> = hunt
            .steps
            .iter()
            .map(|s| (s.shown_when, s.done_when))
            .collect();
        assert_eq!(
            steps,
            [
                (is("started"), is("step-0.done")),
                (is("step-0.done"), Some(counted)),
                (Some(counted), is("step-2.done")),
            ]
        );
        let watch = quests.quest(KnowledgeId::from_guid(WATCH.guid())).unwrap();
        assert_eq!(
            watch.steps[0].done_when.map(|q| q.scope),
            Some(Scope::World),
            "a world quest's bools are the world's"
        );
    }

    #[test]
    fn start_grants_the_first_step_s_bool_once() {
        let (loader, quests) = start(&[]);
        let hunt = KnowledgeId::from_guid(HUNT.guid());
        quests.start(Holder::player(PLAYER), hunt).unwrap();
        assert_eq!(loader.grants(), [bool(owned(HUNT, "started"))]);
        drop(loader);
        let (loader, quests) = start(&[(owned(HUNT, "started"), 1)]);
        quests.start(Holder::player(PLAYER), hunt).unwrap();
        assert!(loader.grants().is_empty(), "started already");
    }

    #[test]
    fn advance_finishes_the_step_the_player_stands_at_and_grants_a_shared_entry_once() {
        let (loader, quests) = start(&[(owned(HUNT, "started"), 1), (wood(), 1)]);
        let hunt = KnowledgeId::from_guid(HUNT.guid());
        assert_eq!(quests.step(Holder::player(PLAYER), hunt), Some(0));
        assert_eq!(quests.advance(Holder::player(PLAYER), hunt), Ok(1));
        assert_eq!(
            loader.grants(),
            [bool(owned(HUNT, "step-0.done"))],
            "step 1 shows on the same bool, granted once"
        );
        drop(loader);
        let (loader, quests) = start(&[
            (owned(HUNT, "started"), 1),
            (owned(HUNT, "step-0.done"), 1),
            (wood(), 1),
        ]);
        assert_eq!(quests.advance(Holder::player(PLAYER), hunt), Ok(2));
        assert_eq!(
            loader.grants(),
            [(Some(PLAYER), wood(), u32::from(KnowledgeInfo::COUNTER), 2)],
            "the counter raised to the 3 the step asks, once"
        );
    }

    #[test]
    fn complete_grants_every_step_left_and_a_finished_quest_stays() {
        let (loader, quests) = start(&[]);
        let hunt = KnowledgeId::from_guid(HUNT.guid());
        quests.complete(Holder::player(PLAYER), hunt).unwrap();
        assert_eq!(
            loader.grants(),
            [
                bool(owned(HUNT, "started")),
                bool(owned(HUNT, "step-0.done")),
                (Some(PLAYER), wood(), u32::from(KnowledgeInfo::COUNTER), 3),
                bool(owned(HUNT, "step-2.done")),
            ]
        );
        drop(loader);
        let (loader, quests) = start(&[
            (owned(HUNT, "started"), 1),
            (owned(HUNT, "step-0.done"), 1),
            (wood(), 3),
            (owned(HUNT, "step-2.done"), 1),
        ]);
        assert_eq!(quests.step(Holder::player(PLAYER), hunt), Some(3));
        assert_eq!(quests.advance(Holder::player(PLAYER), hunt), Ok(3));
        quests.complete(Holder::player(PLAYER), hunt).unwrap();
        assert!(loader.grants().is_empty());
    }

    #[test]
    fn a_world_quest_moves_on_the_world_s_entries() {
        let (loader, quests) = start(&[]);
        let watch = KnowledgeId::from_guid(WATCH.guid());
        quests.complete(Holder::World, watch).unwrap();
        let world_bool = |id| (None, id, u32::from(KnowledgeInfo::BOOL), 0);
        assert_eq!(
            loader.grants(),
            [
                world_bool(owned(WATCH, "started")),
                world_bool(owned(WATCH, "step-0.done"))
            ]
        );
        assert_eq!(
            quests.start(Holder::World, KnowledgeId::from_guid(HUNT.guid())),
            Err(MoveQuestError::World(WorldError::Refused)),
            "a player's entry does not read for the world"
        );
    }

    #[test]
    fn a_vanilla_quest_starts_on_its_bool_and_its_game_query_step_is_refused() {
        let (loader, quests) = start(&[]);
        let vanilla = KnowledgeId::new(VANILLA_QUEST);
        quests.start(Holder::player(PLAYER), vanilla).unwrap();
        assert_eq!(
            loader.grants(),
            [bool(KnowledgeId::new(VANILLA_PLAYER_KNOWLEDGE))]
        );
        drop(loader);
        let (_loader, quests) = start(&[(KnowledgeId::new(VANILLA_PLAYER_KNOWLEDGE), 1)]);
        let refused = quests.complete(Holder::player(PLAYER), vanilla);
        assert!(
            matches!(
                refused,
                Err(MoveQuestError::Unwritable { step: 0, query }) if query.test == KnowledgeTest::Holds
            ),
            "{refused:?}"
        );
        assert_eq!(
            quests.start(Holder::player(PLAYER), KnowledgeId::new(1)),
            Err(MoveQuestError::NoQuest(KnowledgeId::new(1)))
        );
        assert_eq!(
            quests.step(Holder::player(PLAYER), KnowledgeId::new(1)),
            None
        );
    }

    #[test]
    fn reset_clears_the_player_s_entries_and_the_quest_s_bool_once_each() {
        let hunt = KnowledgeId::from_guid(HUNT.guid());
        let (loader, quests) = start(&[
            (owned(HUNT, "started"), 1),
            (owned(HUNT, "step-0.done"), 1),
            (wood(), 4),
        ]);
        loader.set_knowledge(
            &[
                mine(hunt, KnowledgeInfo::BOOL, 1),
                mine(owned(HUNT, "started"), KnowledgeInfo::BOOL, 1),
                mine(owned(HUNT, "step-0.done"), KnowledgeInfo::BOOL, 1),
                mine(owned(HUNT, "step-2.done"), KnowledgeInfo::BOOL, 0),
                mine(wood(), KnowledgeInfo::COUNTER, 4),
            ],
            &[],
        );
        quests.reset(Holder::player(PLAYER), hunt).unwrap();
        let mut revokes = loader.revokes();
        revokes.sort_unstable_by_key(|(_, id, ..)| id.get());
        let mut expected = vec![
            bool(hunt),
            bool(owned(HUNT, "started")),
            bool(owned(HUNT, "step-0.done")),
            (Some(PLAYER), wood(), u32::from(KnowledgeInfo::COUNTER), 4),
        ];
        expected.sort_unstable_by_key(|(_, id, ..)| id.get());
        assert_eq!(revokes, expected, "the unset step-2 bool is left");
        assert!(loader.grants().is_empty());
    }

    #[test]
    fn a_world_reset_leaves_the_players_entries_alone() {
        let watch = KnowledgeId::from_guid(WATCH.guid());
        let (loader, quests) = start(&[(owned(WATCH, "started"), 1)]);
        quests.reset(Holder::World, watch).unwrap();
        assert_eq!(
            loader.revokes(),
            [(
                None,
                owned(WATCH, "started"),
                u32::from(KnowledgeInfo::BOOL),
                0
            )],
            "no quest bool: that is each player's"
        );
    }
}
