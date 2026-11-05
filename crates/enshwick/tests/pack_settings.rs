//! A pack's numbers that follow each world's settings.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, PlayerData, PlayerInfo, Status, TickData};
    use enshwick::events::{PlayerJoined, SettingsReplaced, Ticked, WorldLoaded};
    use enshwick::packs::{Pack, PackFiles};
    use enshwick::progress::Experience;
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    const MANIFEST: &str = "id = \"you.bonus\"\nversion = \"1.0.0\"\n\n[[setting]]\nkey = \"bonus\"\ndefault = 5\nmin = 0\n";
    const REWARDS: &str = "[[reward]]\nid = \"welcome\"\non = { join = true }\ngive = [{ experience = { setting = \"bonus\" } }]\n";

    struct Bonus;

    impl Mod for Bonus {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let files = PackFiles::new()
                .with("pack.toml", MANIFEST)
                .with("rewards.toml", REWARDS);
            load.add_pack(&Pack::parse_with("you.bonus", &files, None)?)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Bonus,
        r#"
        id = "you.bonus"
        side = "server"
        "#
    );

    fn player(entity: u32) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(entity),
            key: PlayerKey::new(u64::from(entity)),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }
    }

    /// The experience a join of `entity` gives in the running world.
    fn joined(loader: &Installed, entity: u32) -> u32 {
        let data = PlayerData {
            header: Header::of::<PlayerData>(),
            slot: 0,
            entity: Entity(entity),
            key: PlayerKey::new(u64::from(entity)),
        };
        let _heard = loader.raise::<PlayerJoined>(&data);
        let _ran = loader.raise::<Ticked>(&TickData::new(1));
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
    fn each_world_gives_its_own_setting_s_value_and_the_default_without_one() {
        let data =
            std::env::temp_dir().join(format!("enshwick-pack-settings-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let loader = FakeHost::new()
            .world()
            .player(player(2))
            .player(player(3))
            .player(player(4))
            .data_dir(data.to_str().unwrap())
            .world_key("world0")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Bonus)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(
            loader.config_schema("settings").is_some(),
            "the pack's settings are the mod's"
        );
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1);
        assert_eq!(
            joined(&loader, 2),
            5,
            "the default while no world's settings came"
        );

        loader.set_settings("settings", "bonus = 30\n");
        let _replaced = loader.raise::<SettingsReplaced>(&());
        assert_eq!(joined(&loader, 3), 30);

        loader.set_settings("settings", "bonus = 7\n");
        let _replaced = loader.raise::<SettingsReplaced>(&());
        assert_eq!(joined(&loader, 4), 7, "another world, another value");
        let _gone = std::fs::remove_dir_all(&data);
    }
}
