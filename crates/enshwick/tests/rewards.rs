//! Rewards for actions, given on a fake server and kept in the world's data.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, ItemAction, PlayerData, PlayerInfo, Status, TickData};
    use enshwick::events::{PlayerJoined, ShutdownStarted, Ticked, WorldLoaded, WorldSaved};
    use enshwick::progress::Experience;
    use enshwick::rewards::{Action, Give, Limit, Reward};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Entity, ItemId, Load, Mod, Off, PlayerKey};

    const TONIC: ItemId = ItemId::named("you.rewards:tonic");

    struct Rewarding;

    impl Mod for Rewarding {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_reward(
                Reward::new("welcome", Action::Join)
                    .with(Give::Experience(40))
                    .with_limit(Limit::Once),
            )?;
            load.add_reward(
                Reward::new("tonic", Action::Consume(TONIC)).with(Give::Experience(5)),
            )?;
            // Player 3 gets nothing.
            load.on_reward(|grant| grant.player != Entity(3))?;
            assert!(
                load.add_reward(Reward::new("tonic", Action::Join).with(Give::Experience(1)))
                    .is_err(),
                "a name declared twice is refused"
            );
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Rewarding,
        r#"
        id = "you.rewards"
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

    fn join(loader: &Installed, entity: u32, key: u64) {
        let joined = PlayerData {
            header: Header::of::<PlayerData>(),
            slot: 0,
            entity: Entity(entity),
            key: PlayerKey::new(key),
        };
        let _heard = loader.raise::<PlayerJoined>(&joined);
        let _ran = loader.raise::<Ticked>(&TickData::new(1));
    }

    /// The experience the next pass adds for `entity`, with room for 1000.
    fn gained(loader: &Installed, entity: u32) -> u32 {
        let mut rows = [Experience {
            entity: Entity(entity),
            level: 1,
            progress: 0,
            need: 1000,
            gained: 0,
        }];
        loader.experience(&mut rows, 25, |level| level * 1000);
        rows[0].gained
    }

    #[test]
    fn a_reward_follows_its_action_holds_its_limit_and_keeps_it_in_the_world() {
        let data = std::env::temp_dir().join(format!("enshwick-rewards-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let loader = FakeHost::new()
            .world()
            .player(player(2, 7))
            .player(player(3, 8))
            .data_dir(data.to_str().unwrap())
            .world_key("world0")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Rewarding)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1);

        join(&loader, 2, 7);
        assert_eq!(gained(&loader, 2), 40);
        join(&loader, 2, 7);
        assert_eq!(gained(&loader, 2), 0, "once per player");
        join(&loader, 3, 8);
        assert_eq!(gained(&loader, 3), 0, "the filter holds player 3 back");

        let used = ItemAction {
            header: Header::of::<ItemAction>(),
            kind: ItemAction::CONSUME,
            player: Entity(2),
            item: TONIC,
            count: 1,
            inventory: Entity(40),
            slot: 0,
        };
        for _ in 0..2 {
            assert!(loader.item_action(&used));
            let _ran = loader.raise::<Ticked>(&TickData::new(2));
        }
        assert_eq!(gained(&loader, 2), 10, "every use rewards");

        assert_eq!(loader.raise::<WorldSaved>(&()), 1);
        let _shut = loader.raise::<ShutdownStarted>(&());
        let text = std::fs::read_to_string(data.join("rewards.toml")).unwrap();
        assert!(text.contains("welcome = [7]"), "{text}");
        let _gone = std::fs::remove_dir_all(&data);
    }
}
