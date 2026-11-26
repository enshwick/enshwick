//! Each kind of objective counts on its own event through the fake host: a
//! kill the game credits to a player, a craft the player asked for, a
//! pickup up to what the inventory holds, and a place reached on a tick.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::__private::ids;
    use enshwick::abi::{ItemAction, KnowledgeInfo, PlayerInfo, Status, TickData};
    use enshwick::content::journal::{CraftTarget, KillTarget, NewQuest, Objective, QuestStep};
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::events::{Ticked, WorldLoaded};
    use enshwick::knowledge::KnowledgeId;
    use enshwick::quests::Quests;
    use enshwick::test::facts::{self, Fact};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Entity, ItemId, Load, Mod, Off, PlayerKey, RecipeId};

    const PLAYER: Entity = Entity::from_engine(4096);
    const WOLF: Entity = Entity::from_engine(77);
    const COIN: u32 = 0x0c01;
    const RECIPE: u32 = 31;
    const CRAFT: u32 = 0x52;
    const CRAFTED: u32 = 0x53;
    const SUMMIT: [f32; 3] = [100.0, 20.0, 100.0];

    const WOLVES: Id = Id::new("you.hunt:wolves");
    const SMITH: Id = Id::new("you.hunt:smith");
    const COINS: Id = Id::new("you.hunt:coins");
    const CLIMB: Id = Id::new("you.hunt:climb");

    static QUESTS: OnceLock<Quests> = OnceLock::new();

    fn wolf() -> Guid {
        Guid::named("you.hunt:wolf")
    }

    fn quest(id: Id, objective: Objective) -> NewQuest {
        let tag = |part: &'static str| id.part(part).guid();
        NewQuest::new(
            id,
            tag("name"),
            vec![{
                let mut value = QuestStep::owned(tag("1"), tag("1.text"));
                value.objectives = vec![objective];
                value
            }],
        )
    }

    struct Hunt;

    impl Mod for Hunt {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            for new in [
                quest(
                    WOLVES,
                    Objective::Kill {
                        target: KillTarget::Template(wolf()),
                        count: 2,
                    },
                ),
                quest(
                    SMITH,
                    Objective::Craft {
                        target: CraftTarget::Recipe(RecipeId::new(RECIPE)),
                        count: 2,
                    },
                ),
                quest(
                    COINS,
                    Objective::Collect {
                        item: ItemId::new(COIN),
                        count: 5,
                    },
                ),
                quest(
                    CLIMB,
                    Objective::Reach {
                        at: SUMMIT,
                        radius: 5.0,
                    },
                ),
            ] {
                load.add_quest(new)?;
            }
            let _ = QUESTS.set(load.quests());
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Hunt,
        r#"
        id = "you.hunt"
        side = "server"
        "#
    );

    fn counter(quest: Id) -> KnowledgeId {
        KnowledgeId::from_guid(quest.part("step-0.obj-0").guid())
    }

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

    fn put(host: &FakeHost, bytes: &mut [u8], field: Fact, value: &[u8]) {
        let at = host.offset(field.id()).unwrap();
        bytes[at..at + value.len()].copy_from_slice(value);
    }

    /// A player inventory of one sub-inventory holding `coins` coins.
    fn purse(host: FakeHost, coins: u32) -> FakeHost {
        let at = |fact: Fact| host.offset(fact.id()).unwrap();
        let (links, categories, initialized) = (
            at(facts::INVENTORY_SETUP_LINKS),
            at(facts::INVENTORY_SETUP_CATEGORIES),
            at(facts::INVENTORY_SETUP_INITIALIZED),
        );
        let mut setup = vec![0u8; (categories + 16).max(initialized + 1)];
        setup[links..links + 4].copy_from_slice(&41u32.to_le_bytes());
        setup[categories] = 4;
        setup[initialized] = 1;
        let mut stacks = vec![0u8; host.size(facts::ITEM_STACK.id()).unwrap() * 8];
        stacks[..4].copy_from_slice(&COIN.to_le_bytes());
        put(
            &host,
            &mut stacks,
            facts::ITEM_STACK_COUNT,
            &coins.to_le_bytes(),
        );
        let (setup_type, inventory) = (
            host.hash(facts::INVENTORY_SETUP.id()).unwrap(),
            host.hash(facts::INVENTORY.id()).unwrap(),
        );
        host.component(PLAYER, setup_type, &setup).component(
            Entity::from_engine(41),
            inventory,
            &stacks,
        )
    }

    /// The player standing at `position`.
    fn standing(host: FakeHost, position: [f64; 3]) -> FakeHost {
        let at = |fact: Fact| host.offset(fact.id()).unwrap();
        let place = at(facts::CURRENT_TRANSFORM_TRANSFORM) + at(facts::WORLD_TRANSFORM_POSITION);
        let mut transform = vec![0u8; place + 24];
        for (i, axis) in enshwick::ecs::fixed(position).iter().enumerate() {
            let start = place + i * 8;
            transform[start..start + 8].copy_from_slice(&axis.to_le_bytes());
        }
        let current = host.hash(facts::CURRENT_TRANSFORM.id()).unwrap();
        host.placed(current, PLAYER, position)
            .component(PLAYER, current, &transform)
    }

    /// A wolf whose kill credit names the player.
    fn wolf_killed_by_player(host: FakeHost) -> FakeHost {
        let mut credit = vec![0u8; host.size(facts::EXPERIENCE_CREDIT.id()).unwrap()];
        let player = PLAYER.get().to_le_bytes();
        put(
            &host,
            &mut credit,
            facts::EXPERIENCE_CREDIT_LAST_PLAYER,
            &player,
        );
        let credit_type = host.hash(facts::EXPERIENCE_CREDIT.id()).unwrap();
        host.component(WOLF, credit_type, &credit)
            .template(WOLF, wolf())
    }

    /// The mod started with each quest begun for the player, its counter
    /// at 0, the player holding `coins` coins at `position`.
    fn start(coins: u32, position: [f64; 3]) -> Installed {
        let mut host = FakeHost::new()
            .progression()
            .world()
            .game_events()
            .inventory_requests()
            .player(enshwick::abi::headed!(PlayerInfo {
                slot: 0,
                entity: PLAYER,
                key: PlayerKey::new(7),
                permissions: 0,
                name_len: 0,
                name: [0; PlayerInfo::NAME_MAX],
            }));
        for (ty, hash) in [
            (ids::CRAFT_RECIPE_EVENT, CRAFT),
            (ids::CRAFT_RECIPE_SUCCESS_EVENT, CRAFTED),
        ] {
            host = host.type_hash(ty, hash).type_size(ty, 24);
        }
        for (field, at) in [
            (ids::CRAFT_RECIPE_EVENT_CRAFTING_OPERATOR_ID, 8),
            (ids::CRAFT_RECIPE_EVENT_CRAFTING_STATION_ID, 12),
            (ids::CRAFT_RECIPE_EVENT_RECIPE_ID, 16),
            (ids::CRAFT_RECIPE_EVENT_RECIPE_AMOUNT, 20),
            (ids::CRAFT_RECIPE_SUCCESS_EVENT_WORKSHOP_ID, 8),
            (ids::CRAFT_RECIPE_SUCCESS_EVENT_RECIPE_ID, 12),
        ] {
            host = host.constant(field, at);
        }
        let host = wolf_killed_by_player(standing(purse(host, coins), position));
        let loader = host.install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Hunt)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let _ran = loader.raise::<WorldLoaded>(&());
        let mut values = Vec::new();
        for quest in [WOLVES, SMITH, COINS, CLIMB] {
            values.push(info(
                KnowledgeId::from_guid(quest.guid()),
                KnowledgeInfo::BOOL,
                0,
            ));
            let started = KnowledgeId::from_guid(quest.part("started").guid());
            values.push(info(started, KnowledgeInfo::BOOL, 1));
            values.push(info(counter(quest), KnowledgeInfo::COUNTER, 0));
        }
        loader.set_knowledge(&values, &[]);
        assert!(QUESTS.get().is_some());
        loader
    }

    /// What the grants since the last call raised, by counter.
    fn raised(loader: &Installed) -> Vec<(KnowledgeId, u32)> {
        loader
            .grants()
            .into_iter()
            .map(|(_, id, _, amount)| (id, amount))
            .collect()
    }

    /// A 24-byte event with each `u32` at its offset.
    fn event(fields: &[(usize, u32)]) -> [u8; 24] {
        let mut bytes = [0u8; 24];
        for &(at, value) in fields {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn a_kill_the_game_credits_to_the_player_counts_for_the_wolf_s_template() {
        let events = FakeHost::new().game_events();
        let died_event = events.hash(facts::ENTITY_DIED_EVENT.id()).unwrap();
        let mut died = vec![0u8; events.size(facts::ENTITY_DIED_EVENT.id()).unwrap()];
        let dead = WOLF.get().to_le_bytes();
        put(
            &events,
            &mut died,
            facts::ENTITY_DIED_EVENT_DEAD_ENTITY_ID,
            &dead,
        );
        let loader = start(0, [0.0; 3]);
        assert_eq!(loader.raise_game_event(died_event, &died), 1);
        assert_eq!(
            raised(&loader),
            [(counter(WOLVES), 1)],
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_craft_the_player_asked_for_counts_once_per_success() {
        let loader = start(0, [0.0; 3]);
        let ask = event(&[(8, PLAYER.get()), (16, RECIPE), (20, 2)]);
        assert_eq!(loader.raise_game_event(CRAFT, &ask), 1);
        let success = event(&[(12, RECIPE)]);
        assert_eq!(loader.raise_game_event(CRAFTED, &success), 1);
        let _ran = loader.raise::<Ticked>(&TickData::new(1));
        assert_eq!(
            raised(&loader),
            [(counter(SMITH), 1)],
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_pickup_raises_the_counter_to_what_the_inventory_holds() {
        let loader = start(3, [0.0; 3]);
        let picked = enshwick::abi::headed!(ItemAction {
            kind: ItemAction::PICKUP,
            character: PLAYER,
            item: ItemId::new(COIN),
            count: 3,
            inventory: Entity::from_engine(41),
            slot: 0,
        });
        assert!(loader.item_action(&picked));
        assert_eq!(
            raised(&loader),
            [(counter(COINS), 3)],
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_player_within_the_radius_reaches_the_place_on_the_next_tick() {
        let near = [102.0, 20.0, 99.0];
        let loader = start(0, near);
        let _ran = loader.tick(Duration::from_secs(1));
        assert_eq!(
            raised(&loader),
            [(counter(CLIMB), 1)],
            "{:?}",
            loader.logs()
        );
        drop(loader);
        let loader = start(0, [120.0, 20.0, 100.0]);
        let _ran = loader.tick(Duration::from_secs(1));
        assert!(raised(&loader).is_empty(), "20 m away");
    }
}
