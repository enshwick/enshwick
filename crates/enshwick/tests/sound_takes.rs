//! The takes of a vanilla sound container: two mods replace and add them in
//! load order, a take names the game's recording or one an earlier mod
//! declared, and a take of no known recording is refused at init.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ModHandle, Side, Status};
    use enshwick::content::{Guid, Id, types};
    use enshwick::resources::{Field, Kind, Registry, Type, Value, fnv1a32};
    use enshwick::sound::{SoundChange, SoundContainerId, SoundId, Take, Takes};
    use enshwick::test::FakeHost;

    const CONTAINER: &str = "demo::SoundContainerResource";
    const ENTRY: &str = "demo::SoundContainerResourceSoundEntry";
    const SOUND: &str = "demo::SoundResource";
    const CHEST: Guid = Guid([1; 16]);
    const CREAK: Guid = Guid([2; 16]);
    const SLAM: Guid = Guid([3; 16]);
    const UNKNOWN: Guid = Guid([4; 16]);
    const BELL: Id = Id::new("you.first:bell.sound");

    fn types() -> Vec<Type<'static>> {
        vec![
            Type::new("demo::float32", Kind::Float32, 4, 4),
            Type::new("demo::Guid", Kind::Guid, 16, 4),
            Type::new("demo::SoundContainerResourceEntry", Kind::Struct, 4, 4),
            Type::new(ENTRY, Kind::Struct, 20, 4)
                .with_inner("demo::SoundContainerResourceEntry")
                .with_fields(vec![
                    Field::new("chance", 0, "demo::float32"),
                    Field::new("sound", 4, "demo::Guid"),
                ]),
            Type::new("demo::Variant", Kind::BlobVariant, 12, 4)
                .with_inner("demo::SoundContainerResourceEntry"),
            Type::new("demo::Entries", Kind::BlobArray, 8, 4).with_inner("demo::Variant"),
            Type::new(CONTAINER, Kind::Struct, 12, 4).with_fields(vec![
                Field::new("volume", 0, "demo::float32"),
                Field::new("entries", 4, "demo::Entries"),
            ]),
            Type::new(SOUND, Kind::Struct, 4, 4).with_fields(vec![Field::new(
                "volume",
                0,
                "demo::float32",
            )]),
        ]
    }

    fn entry(sound: Guid, chance: f64) -> Value {
        Value::Variant(Box::new(enshwick::resources::Variant::new(
            ENTRY,
            Value::Struct(vec![
                ("chance".into(), Value::Float(chance)),
                ("sound".into(), Value::Guid(sound)),
            ]),
        )))
    }

    fn chest(entries: Vec<Value>) -> Value {
        Value::Struct(vec![
            ("volume".into(), Value::Float(1.0)),
            ("entries".into(), Value::List(entries)),
        ])
    }

    fn take(sound: Guid, chance: f32) -> Take {
        {
            let mut value = Take::new(SoundId::from_guid(sound));
            value.chance = chance;
            value
        }
    }

    fn host() -> FakeHost {
        let sound_hash = fnv1a32(SOUND.as_bytes());
        FakeHost::new()
            .side(Side::CLIENT)
            .pack(ModHandle(2), Side::BOTH)
            .types(types())
            .type_hash(
                types::SOUND_CONTAINER_RESOURCE.id(),
                fnv1a32(CONTAINER.as_bytes()),
            )
            .type_hash(types::SOUND_RESOURCE.id(), sound_hash)
            .type_hash("sound_container_sound_entry", fnv1a32(ENTRY.as_bytes()))
            .resource(CREAK.key_by_hash(sound_hash), vec![0; 4])
            .resource(SLAM.key_by_hash(sound_hash), vec![0; 4])
    }

    /// Runs `init` as the mod `me` of side both on `loader`.
    fn run(
        loader: &enshwick::test::Installed,
        me: usize,
        init: impl FnOnce(&mut enshwick::Load<'_>) -> Result<(), enshwick::Off>,
    ) -> Status {
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick::__private::init_pack(loader.api(), ModHandle(me), Side::BOTH, init) }
    }

    #[test]
    fn two_mods_replace_and_add_takes_in_load_order() {
        let loader = host().install();
        let chest_id = SoundContainerId::from_guid(CHEST);
        let first = run(&loader, 1, |load| {
            load.declare(BELL)?;
            let replace = {
                let mut value = SoundChange::default();
                value.takes = Some(Takes::Replace(vec![take(CREAK, 0.5)]));
                value
            };
            load.change_sound(chest_id, replace)
        });
        assert_eq!(first, Status::OK, "{:?}", loader.logs());
        let second = run(&loader, 2, |load| {
            let unknown = {
                let mut value = SoundChange::default();
                value.takes = Some(Takes::Add(vec![take(UNKNOWN, 1.0)]));
                value
            };
            let refused = load.change_sound(chest_id, unknown).unwrap_err();
            assert!(
                refused.to_string().contains(&UNKNOWN.to_string()),
                "{refused}"
            );
            let add = {
                let mut value = SoundChange::default();
                value.volume = Some(0.25);
                value.takes = Some(Takes::Add(vec![take(SLAM, 1.0), take(BELL.guid(), 0.75)]));
                value
            };
            load.change_sound(chest_id, add)
        });
        assert_eq!(second, Status::OK, "{:?}", loader.logs());
        let registry = Registry::new(types());
        let vanilla = registry
            .encode(CONTAINER, &chest(vec![entry(Guid([9; 16]), 1.0)]), None)
            .unwrap();
        let key = CHEST.key_by_hash(fnv1a32(CONTAINER.as_bytes()));
        let built = loader.build_resource(&key, &vanilla).unwrap();
        let read = registry.decode(CONTAINER, &built).unwrap();
        assert_eq!(
            read,
            Value::Struct(vec![
                ("volume".into(), Value::Float(0.25)),
                (
                    "entries".into(),
                    Value::List(vec![
                        entry(CREAK, 0.5),
                        entry(SLAM, 1.0),
                        entry(BELL.guid(), 0.75),
                    ])
                ),
            ]),
            "the first mod's take replaced the game's; the second's follow it"
        );
    }

    #[test]
    fn a_pack_s_change_whose_take_is_unknown_turns_off_alone() {
        use enshwick::packs::{Pack, PackFiles};

        let other = Guid([5; 16]);
        let files = PackFiles::new().with(
            "sounds.toml",
            format!(
                "[[change]]\nsound = \"{CHEST}\"\ntakes = {{ add = [{{ sound = \"{UNKNOWN}\" }}] }}\n\n\
                 [[change]]\nsound = \"{other}\"\nvolume = 0.5\n"
            ),
        );
        let loader = host().install();
        let status = run(&loader, 1, |load| {
            load.add_pack(&Pack::parse("you.first", &files)?)
        });
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let reason = |container: Guid| {
            let name = format!("sound change {container}");
            loader
                .features()
                .into_iter()
                .find(|(feature, _)| *feature == name)
                .map(|(_, reason)| reason)
        };
        assert!(
            reason(CHEST)
                .flatten()
                .is_some_and(|why| why.contains(&UNKNOWN.to_string())),
            "{:?}",
            loader.features()
        );
        assert_eq!(reason(other), Some(None), "{:?}", loader.features());
    }

    #[test]
    fn a_take_naming_a_mod_s_content_that_is_no_recording_is_refused() {
        const HORN: Id = Id::new("you.first:horn");
        let loader = host().install();
        let first = run(&loader, 1, |load| load.declare(HORN).map(|_| ()));
        assert_eq!(first, Status::OK, "{:?}", loader.logs());
        let second = run(&loader, 2, |load| {
            let horn = {
                let mut value = SoundChange::default();
                value.takes = Some(Takes::Add(vec![take(HORN.guid(), 1.0)]));
                value
            };
            let refused = load
                .change_sound(SoundContainerId::from_guid(CHEST), horn)
                .unwrap_err();
            assert!(
                refused
                    .to_string()
                    .contains("you.first:horn is no recording"),
                "{refused}"
            );
            Ok(())
        });
        assert_eq!(second, Status::OK, "{:?}", loader.logs());
    }
}
