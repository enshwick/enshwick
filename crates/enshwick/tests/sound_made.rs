//! A sound container a mod makes: changes made before or after its maker
//! apply over it, and a client plays it.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Side, Status};
    use enshwick::content::{Guid, types};
    use enshwick::resources::{Field, Kind, Type, fnv1a32};
    use enshwick::sound::{SoundChange, SoundContainerId};
    use enshwick::test::{Drawn, FakeHost};
    use enshwick::{Load, Mod, Off};

    const CHEST: Guid = Guid::named("you.bells:chest");
    const BELL: Guid = Guid::named("you.bells:bell");
    const CONTAINER: &str = "demo::SoundContainerResource";

    struct Bells;

    impl Mod for Bells {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let bell = SoundContainerId::from_guid(BELL);
            load.change_sound(
                bell,
                SoundChange {
                    volume: Some(0.5),
                    ..SoundChange::default()
                },
            )?;
            load.serve(
                BELL.key(types::SOUND_CONTAINER_RESOURCE)?,
                CHEST.key(types::SOUND_CONTAINER_RESOURCE)?,
                |chest| Some(chest.to_vec()),
            )?;
            load.change_sound(
                bell,
                SoundChange {
                    pitch: Some(2.0),
                    ..SoundChange::default()
                },
            )?;
            load.on_frame(move |frame| {
                assert!(frame.play(bell).is_ok(), "a made container plays");
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Bells,
        r#"
        id = "you.bells"
        side = "both"
        "#
    );

    #[test]
    fn changes_before_and_after_the_maker_apply_over_it_and_the_container_plays() {
        let hash = fnv1a32(CONTAINER.as_bytes());
        let loader = FakeHost::new()
            .side(Side::CLIENT)
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
            .type_hash(types::SOUND_CONTAINER_RESOURCE.id(), hash)
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Bells)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.resources()[0].base, CHEST.key_by_hash(hash));
        let chest = [
            1.0f32.to_le_bytes().as_slice(),
            &1.0f32.to_le_bytes(),
            &[0, 0],
            &7u16.to_le_bytes(),
        ]
        .concat();
        let expected = [
            0.5f32.to_le_bytes().as_slice(),
            &2.0f32.to_le_bytes(),
            &[0, 0],
            &7u16.to_le_bytes(),
        ]
        .concat();
        let built = loader.build_resource(&BELL.key_by_hash(hash), &chest);
        assert_eq!(built.unwrap(), expected);
        assert_eq!(loader.draw_frame(), [Drawn::Play(BELL)]);
    }
}
