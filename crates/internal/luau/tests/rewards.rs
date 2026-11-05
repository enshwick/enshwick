//! `enshwick.rewards`: rewards a script declares at start and grants later.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, ItemAction, PlayerInfo, TickData};
    use enshwick::events::{Ticked, WorldLoaded};
    use enshwick::progress::Experience;
    use enshwick::test::FakeHost;
    use enshwick::{Entity, PlayerKey};

    use crate::harness::{logged, run};

    #[test]
    fn a_script_declares_a_reward_its_item_use_grants_and_grants_another_by_name() {
        let player = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(2),
            key: PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        let data =
            std::env::temp_dir().join(format!("enshwick-luau-rewards-{}", std::process::id()));
        let loader = run(
            FakeHost::new()
                .world()
                .player(player)
                .data_dir(data.to_str().unwrap())
                .world_key("world0"),
            r#"
            local tonic = enshwick.content.item_id("you.luau:tonic")
            enshwick.rewards.add_reward({ name = "sip", on = { Consume = tonic }, give = { { Experience = 3 } } })
            enshwick.rewards.add_reward({ name = "bonus", on = "Join", give = { { Experience = 20 } }, limit = "Once" })
            enshwick.on("Ticked", function()
                enshwick.log.info(`bonus {enshwick.rewards.grant(2, "bonus")} unknown {enshwick.rewards.grant(2, "none")}`)
            end)
            "#,
        );
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1);
        let used = ItemAction {
            header: Header::of::<ItemAction>(),
            kind: ItemAction::CONSUME,
            player: Entity(2),
            item: enshwick::ItemId::named("you.luau:tonic"),
            count: 1,
            inventory: Entity(40),
            slot: 0,
        };
        assert!(loader.item_action(&used));
        let _ran = loader.raise::<Ticked>(&TickData::new(1));
        assert!(
            logged(&loader, "bonus true unknown false"),
            "{:?}",
            loader.logs()
        );
        let mut rows = [Experience {
            entity: Entity(2),
            level: 1,
            progress: 0,
            need: 1000,
            gained: 0,
        }];
        loader.experience(&mut rows, 25, |level| level * 1000);
        assert_eq!(rows[0].gained, 23, "{:?}", loader.logs());
        let _gone = std::fs::remove_dir_all(&data);
    }
}
