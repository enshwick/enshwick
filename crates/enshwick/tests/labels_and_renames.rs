//! Names on entities: a label a mod sets, and a player's rename a mod's
//! filter lets through or refuses.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::world::World;
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    static WORLD: OnceLock<World> = OnceLock::new();

    const CHEST: Entity = Entity(4096);

    struct Signs;

    impl Mod for Signs {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            load.on_rename(|rename| {
                rename.player.as_ref().is_some_and(|p| p.name == "Ada") && rename.text != "Bad"
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Signs,
        r#"
        id = "you.signs"
        side = "server"
        "#
    );

    fn started() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().world().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Signs)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        loader
    }

    fn player(name: &str) -> PlayerInfo {
        let mut bytes = [0; 64];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(2),
            key: PlayerKey::new(0x0110_0001_0000_0042),
            permissions: 0,
            name_len: name.len().try_into().unwrap(),
            name: bytes,
        }
    }

    #[test]
    fn a_label_reaches_the_loader_with_its_entity_and_text() {
        let loader = started();
        let world = WORLD.get().unwrap();
        assert!(world.set_label(CHEST, "Built by Ada"));
        assert!(world.set_label(CHEST, ""), "an empty text removes the name");
        assert_eq!(
            loader.labels(),
            [
                (CHEST.0, "Built by Ada".to_owned()),
                (CHEST.0, String::new())
            ]
        );
    }

    #[test]
    fn a_rename_filter_sees_the_player_and_the_text() {
        let loader = started();
        assert!(loader.rename(CHEST, Some(&player("Ada")), "Tools"));
        assert!(!loader.rename(CHEST, Some(&player("Ada")), "Bad"));
        assert!(!loader.rename(CHEST, Some(&player("Bob")), "Tools"));
        assert!(
            !loader.rename(CHEST, None, "Tools"),
            "a player the server could not tell"
        );
    }
}
