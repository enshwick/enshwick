//! Changes to a vanilla sound container: served over the game's bytes on a
//! client in the order made, checked and left alone on a server.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Side, Status};
    use enshwick::content::{Guid, types};
    use enshwick::resources::{Field, Kind, Type, fnv1a32};
    use enshwick::sound::{SoundChange, SoundContainerId};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const CHEST: Guid = Guid::named("you.sounds:chest");
    const CONTAINER: &str = "demo::SoundContainerResource";

    struct Quieter;

    impl Mod for Quieter {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let chest = SoundContainerId::from_guid(CHEST);
            let louder = SoundChange {
                volume: Some(9.0),
                ..SoundChange::default()
            };
            assert!(load.change_sound(chest, louder).is_err(), "past the bound");
            load.change_sound(
                chest,
                SoundChange {
                    volume: Some(0.5),
                    looped: Some(true),
                    ..SoundChange::default()
                },
            )?;
            load.change_sound(
                chest,
                SoundChange {
                    volume: Some(0.25),
                    pitch: Some(2.0),
                    ..SoundChange::default()
                },
            )?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Quieter,
        r#"
        id = "you.sounds"
        side = "both"
        "#
    );

    fn host(side: Side) -> FakeHost {
        FakeHost::new()
            .side(side)
            .types([
                Type::new("demo::float32", Kind::Float32, 4, 4),
                Type::new("demo::bool", Kind::Bool, 1, 1),
                Type::new("demo::uint16", Kind::Uint16, 2, 2),
                Type::new(CONTAINER, Kind::Struct, 12, 4).with_fields(vec![
                    Field::new("volume", 0, "demo::float32"),
                    Field::new("pitch", 4, "demo::float32"),
                    Field::new("loop", 8, "demo::bool"),
                    Field::new("priority", 10, "demo::uint16"),
                ]),
            ])
            .type_hash(
                types::SOUND_CONTAINER_RESOURCE.id(),
                fnv1a32(CONTAINER.as_bytes()),
            )
    }

    fn base() -> Vec<u8> {
        [
            1.0f32.to_le_bytes().as_slice(),
            &1.0f32.to_le_bytes(),
            &[0, 0],
            &7u16.to_le_bytes(),
        ]
        .concat()
    }

    #[test]
    fn a_client_serves_the_changes_in_the_order_made_over_the_game_s_bytes() {
        let loader = host(Side::CLIENT).install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Quieter)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let key = CHEST.key_by_hash(fnv1a32(CONTAINER.as_bytes()));
        let built = loader.build_resource(&key, &base()).unwrap();
        let expected = [
            0.25f32.to_le_bytes().as_slice(),
            &2.0f32.to_le_bytes(),
            &[1, 0],
            &7u16.to_le_bytes(),
        ]
        .concat();
        assert_eq!(built, expected, "the later change wins, the rest stay");
    }

    #[test]
    fn a_server_checks_the_changes_and_serves_nothing() {
        let loader = host(Side::SERVER).install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Quieter)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(loader.resources().is_empty());
    }
}
