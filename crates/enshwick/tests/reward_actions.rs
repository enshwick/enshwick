//! Rewards that follow a craft, a level and a loot, and gifts given from code.

#[cfg(test)]
mod tests {
    use enshwick::__private::ids;
    use enshwick::abi::{Header, PlayerInfo, Status, TickData};
    use enshwick::events::{Ticked, WorldLoaded};
    use enshwick::progress::Experience;
    use enshwick::rewards::{Action, Give, Reward, Rewards};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Entity, Load, Mod, Off, PlayerKey, RecipeId};

    const LEVEL_UP: u32 = 0x51;
    const CRAFT: u32 = 0x52;
    const CRAFTED: u32 = 0x53;
    const LOOT: u32 = 0x54;
    const RECIPE: u32 = 77;

    static REWARDS: std::sync::OnceLock<Rewards> = std::sync::OnceLock::new();

    struct Acting;

    impl Mod for Acting {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            for (name, on, amount) in [
                ("third", Action::LevelUp(3), 1),
                ("any-level", Action::LevelUp(0), 10),
                ("smith", Action::Craft(RecipeId::new(RECIPE)), 100),
                ("looter", Action::Loot, 1000),
            ] {
                load.add_reward(Reward::new(name, on).with(Give::Experience(amount)))?;
            }
            let rewards = load.rewards()?;
            let _set = REWARDS.set(rewards);
            // A filter may grant another reward while it decides.
            load.on_reward(move |grant| {
                if grant.reward == "looter" {
                    let _also = rewards.grant(grant.player, "third");
                }
                true
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Acting,
        r#"
        id = "you.acting"
        side = "server"
        "#
    );

    fn player(entity: u32, key: u64) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(entity),
            key: PlayerKey::new(key),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }
    }

    /// A 24-byte event with each `u32` at its offset.
    fn event(fields: &[(usize, u32)]) -> [u8; 24] {
        let mut bytes = [0u8; 24];
        for &(at, value) in fields {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// The experience the next pass adds for `entity`, with room for 10000.
    fn gained(loader: &Installed, entity: u32) -> u32 {
        let _ran = loader.raise::<Ticked>(&TickData::new(1));
        let mut rows = [Experience {
            entity: Entity(entity),
            level: 1,
            progress: 0,
            need: 10_000,
            gained: 0,
        }];
        loader.experience(&mut rows, 25, |level| level * 10_000);
        rows[0].gained
    }

    #[test]
    fn a_level_a_craft_and_a_loot_each_reward_only_their_own_action() {
        let data = std::env::temp_dir().join(format!("enshwick-acting-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let mut host = FakeHost::new()
            .world()
            .player(player(2, 7))
            .player(player(4, 9))
            .data_dir(data.to_str().unwrap())
            .world_key("world0");
        for (ty, hash) in [
            (ids::LEVEL_UP_EVENT, LEVEL_UP),
            (ids::CRAFT_RECIPE_EVENT, CRAFT),
            (ids::CRAFT_RECIPE_SUCCESS_EVENT, CRAFTED),
            (ids::LOOT_INTERACTION_EVENT, LOOT),
        ] {
            host = host.type_hash(ty, hash).type_size(ty, 24);
        }
        for (field, at) in [
            (ids::LEVEL_UP_EVENT_PLAYER_ENTITY_ID, 8),
            (ids::LEVEL_UP_EVENT_LEVEL, 12),
            (ids::CRAFT_RECIPE_EVENT_CRAFTING_OPERATOR_ID, 8),
            (ids::CRAFT_RECIPE_EVENT_CRAFTING_STATION_ID, 12),
            (ids::CRAFT_RECIPE_EVENT_RECIPE_ID, 16),
            (ids::CRAFT_RECIPE_EVENT_RECIPE_AMOUNT, 20),
            (ids::CRAFT_RECIPE_SUCCESS_EVENT_WORKSHOP_ID, 8),
            (ids::CRAFT_RECIPE_SUCCESS_EVENT_RECIPE_ID, 12),
            (ids::LOOT_INTERACTION_EVENT_GUEST_ID, 8),
            (ids::LOOT_INTERACTION_EVENT_HOST_ID, 12),
            (ids::LOOT_INTERACTION_EVENT_LOOT_ALL, 16),
        ] {
            host = host.constant(field, at);
        }
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Acting)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            loader.raise::<WorldLoaded>(&()),
            2,
            "the mod's and its rewards'"
        );

        assert_eq!(
            loader.raise_game_event(LEVEL_UP, &event(&[(8, 2), (12, 2)])),
            2
        );
        assert_eq!(
            gained(&loader, 2),
            10,
            "level 2 gives only the any-level reward"
        );
        assert_eq!(
            loader.raise_game_event(LEVEL_UP, &event(&[(8, 2), (12, 3)])),
            2
        );
        assert_eq!(gained(&loader, 2), 11, "level 3 gives both");

        let other = event(&[(8, 4), (16, RECIPE + 1)]);
        assert_eq!(loader.raise_game_event(CRAFT, &other), 1);
        assert_eq!(
            loader.raise_game_event(CRAFTED, &event(&[(12, RECIPE + 1)])),
            1
        );
        assert_eq!(gained(&loader, 4), 0, "another recipe gives nothing");
        assert_eq!(loader.raise_game_event(CRAFTED, &event(&[(12, RECIPE)])), 1);
        assert_eq!(
            gained(&loader, 4),
            0,
            "a success nobody asked for gives nothing"
        );
        assert_eq!(
            loader.raise_game_event(CRAFT, &event(&[(8, 4), (16, RECIPE)])),
            1
        );
        assert_eq!(loader.raise_game_event(CRAFTED, &event(&[(12, RECIPE)])), 1);
        assert_eq!(gained(&loader, 4), 100, "the player who asked gets it");
        assert_eq!(loader.raise_game_event(CRAFTED, &event(&[(12, RECIPE)])), 1);
        assert_eq!(gained(&loader, 4), 0, "one request pairs with one success");

        assert_eq!(
            loader.raise_game_event(LOOT, &event(&[(8, 4), (12, 50)])),
            1
        );
        assert_eq!(
            gained(&loader, 4),
            1001,
            "the loot and the reward its filter grants"
        );
        assert_eq!(gained(&loader, 2), 0, "only the looter");

        let rewards = *REWARDS.get().unwrap();
        rewards.give(Entity(2), &[Give::Experience(7), Give::Experience(5)]);
        assert_eq!(
            gained(&loader, 2),
            12,
            "every gift of a give, with no reward"
        );
        assert!(rewards.grant(Entity(4), "looter"));
        assert!(!rewards.grant(Entity(4), "unknown"));
        assert!(!rewards.grant(Entity(99), "looter"), "no such player");
        assert_eq!(gained(&loader, 4), 1001);
        let _gone = std::fs::remove_dir_all(&data);
    }
}
