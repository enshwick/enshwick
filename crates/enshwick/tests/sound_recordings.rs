//! A mod's recordings on a client whose game holds `FramelessQOA` sounds to
//! build from: a replaced vanilla recording holds or streams the mod's
//! encoded samples, two mods replacing one apply in load order with the log
//! naming both, and a sound whose container fails after its recording keeps
//! its ids taken.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Level, ModHandle, Side, Status};
    use enshwick::content::{Guid, types};
    use enshwick::resources::{Constant, Field, Kind, Registry, Type, Value, fnv1a32};
    use enshwick::sound::{SAMPLE_RATE, SoundId};
    use enshwick::test::{FakeHost, Installed};

    const SOUND: &str = "demo::SoundResource";
    const CONTAINER: &str = "demo::SoundContainerResource";
    const ENTRY: &str = "demo::SoundContainerResourceSoundEntry";
    /// The game's one streamed sound, which mods build from.
    const WATERFALL: Guid = Guid([1; 16]);
    /// A vanilla recording held inline, which mods replace.
    const CREAK: Guid = Guid([2; 16]);

    fn types() -> Vec<Type<'static>> {
        let uint = |name| Type::new(name, Kind::Uint32, 4, 4);
        vec![
            Type::new("demo::uint8", Kind::Uint8, 1, 1),
            Type::new("demo::uint16", Kind::Uint16, 2, 2),
            uint("demo::uint32"),
            Type::new("demo::sint64", Kind::Sint64, 8, 8),
            Type::new("demo::BlobString", Kind::BlobString, 8, 4),
            Type::new("demo::BlobArray<uint8>", Kind::BlobArray, 8, 4).with_inner("demo::uint8"),
            Type::new("demo::SoundDataFormat", Kind::Enum, 1, 1).with_constants(vec![
                Constant::new("Uncompressed", 0),
                Constant::new("FramelessQOA", 1),
            ]),
            Type::new("demo::SoundChannelConfiguration", Kind::Enum, 1, 1)
                .with_constants(vec![Constant::new("Mono", 0), Constant::new("Stereo", 1)]),
            Type::new("demo::Time", Kind::Struct, 8, 8).with_fields(vec![Field::new(
                "value",
                0,
                "demo::sint64",
            )]),
            Type::new("demo::ContentHash", Kind::Struct, 16, 4).with_fields(vec![
                Field::new("size", 0, "demo::uint32"),
                Field::new("hash0", 4, "demo::uint32"),
                Field::new("hash1", 8, "demo::uint32"),
                Field::new("hash2", 12, "demo::uint32"),
            ]),
            Type::new("demo::ResourceContentReference", Kind::Struct, 20, 4).with_fields(vec![
                Field::new("hash", 0, "demo::ContentHash"),
                Field::new("category", 16, "demo::uint32"),
            ]),
            Type::new(ENTRY, Kind::Struct, 4, 4),
            Type::new(CONTAINER, Kind::Struct, 4, 4).with_fields(vec![Field::new(
                "priority",
                0,
                "demo::uint32",
            )]),
            Type::new(SOUND, Kind::Struct, 56, 8).with_fields(vec![
                Field::new("format", 0, "demo::SoundDataFormat"),
                Field::new("channelConfiguration", 1, "demo::SoundChannelConfiguration"),
                Field::new("framesPerSecond", 2, "demo::uint16"),
                Field::new("frameCount", 4, "demo::uint32"),
                Field::new("duration", 8, "demo::Time"),
                Field::new("data", 16, "demo::BlobArray<uint8>"),
                Field::new("streamedData", 24, "demo::ResourceContentReference"),
                Field::new("debugName", 44, "demo::BlobString"),
            ]),
        ]
    }

    fn sound(format: &str, frames: u64, data: Vec<Value>, size: u64, category: u64) -> Value {
        let word = |name: &str, value: u64| (name.to_owned(), Value::Uint(value));
        Value::Struct(vec![
            ("format".into(), Value::Name(format.into())),
            ("channelConfiguration".into(), Value::Name("Mono".into())),
            word("framesPerSecond", u64::from(SAMPLE_RATE)),
            word("frameCount", frames),
            (
                "duration".into(),
                Value::Struct(vec![("value".into(), Value::Int(1))]),
            ),
            ("data".into(), Value::List(data)),
            (
                "streamedData".into(),
                Value::Struct(vec![
                    (
                        "hash".into(),
                        Value::Struct(vec![
                            word("size", size),
                            word("hash0", 0),
                            word("hash1", 0),
                            word("hash2", 0),
                        ]),
                    ),
                    word("category", category),
                ]),
            ),
            ("debugName".into(), Value::Text("vanilla".into())),
        ])
    }

    /// A short mono 16-bit WAV file at the game's rate with `frames` frames.
    fn wav(frames: u32) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut file = std::io::Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut file, spec).unwrap();
        for i in 0..frames {
            writer
                .write_sample(i16::try_from(i % 100).unwrap())
                .unwrap();
        }
        writer.finalize().unwrap();
        file.into_inner()
    }

    fn sound_hash() -> u32 {
        fnv1a32(SOUND.as_bytes())
    }

    /// A client whose archive holds the streamed waterfall and the inline
    /// creak, and whose mods' folder holds `files`.
    fn client(files: &[(&str, Vec<u8>)]) -> Installed {
        let registry = Registry::new(types());
        let encode = |value: &Value| registry.encode(SOUND, value, None).unwrap();
        let waterfall = sound("FramelessQOA", 9, Vec::new(), 24, 0xb3c9_d834);
        let creak = sound("FramelessQOA", 2, vec![Value::Uint(7); 8], 0, 0);
        let mut host = FakeHost::new()
            .side(Side::CLIENT)
            .pack(ModHandle(2), Side::BOTH)
            .pack(ModHandle(3), Side::BOTH)
            .types(types())
            .type_hash(types::SOUND_RESOURCE.id(), sound_hash())
            .type_hash(
                types::SOUND_CONTAINER_RESOURCE.id(),
                fnv1a32(CONTAINER.as_bytes()),
            )
            .type_hash("sound_container_sound_entry", fnv1a32(ENTRY.as_bytes()))
            .resource(WATERFALL.key_by_hash(sound_hash()), encode(&waterfall))
            .resource(CREAK.key_by_hash(sound_hash()), encode(&creak));
        for (name, bytes) in files {
            host = host.mod_file(name, bytes.clone());
        }
        host.install()
    }

    /// The creak as the client builds it from the archive now.
    fn built_creak(loader: &Installed) -> Value {
        let registry = Registry::new(types());
        let key = CREAK.key_by_hash(sound_hash());
        let archived = registry
            .encode(
                SOUND,
                &sound("FramelessQOA", 2, vec![Value::Uint(7); 8], 0, 0),
                None,
            )
            .unwrap();
        registry
            .decode(SOUND, &loader.build_resource(&key, &archived).unwrap())
            .unwrap()
    }

    /// Runs `init` as the mod `me` of side both on `loader`.
    fn run(
        loader: &Installed,
        me: usize,
        init: impl FnOnce(&mut enshwick::Load<'_>) -> Result<(), enshwick::Off>,
    ) -> Status {
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick::__private::init_pack(loader.api(), ModHandle(me), Side::BOTH, init) }
    }

    #[test]
    fn two_mods_replacing_one_recording_apply_in_load_order_and_the_log_names_both() {
        // Past one second, so it streams.
        const LONG: u32 = 48_100;
        let loader = client(&[("creak.wav", wav(30)), ("slam.wav", wav(LONG))]);
        let creak = SoundId::from_guid(CREAK);
        let status = run(&loader, 1, |load| load.replace_sound(creak, "creak.wav"));
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let inline = built_creak(&loader);
        assert_eq!(inline.field("frameCount"), Some(&Value::Uint(30)));
        let Some(Value::List(data)) = inline.field("data") else {
            panic!("no data");
        };
        assert_eq!(data.len(), 16, "two slices of 30 frames, held inline");
        let status = run(&loader, 2, |load| load.replace_sound(creak, "slam.wav"));
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let refused = run(&loader, 3, |load| {
            load.replace_sound(SoundId::from_guid(Guid([9; 16])), "creak.wav")
        });
        assert_ne!(refused, Status::OK, "a recording the game lacks");
        let built = built_creak(&loader);
        assert_eq!(
            built.field("format"),
            Some(&Value::Name("FramelessQOA".into()))
        );
        assert_eq!(
            built.field("frameCount"),
            Some(&Value::Uint(u64::from(LONG))),
            "the later file"
        );
        assert_eq!(built.field("data"), Some(&Value::List(Vec::new())));
        let streamed = built.field("streamedData").unwrap();
        assert_eq!(
            streamed.field("category"),
            Some(&Value::Uint(0xb3c9_d834)),
            "the waterfall's category, which the inline creak lacks"
        );
        let size = streamed.field("hash").and_then(|h| h.field("size"));
        let slices = u64::from(LONG.div_ceil(20));
        assert_eq!(size, Some(&Value::Uint(slices * 8)), "a u64 per 20 frames");
        assert!(
            loader
                .served_content()
                .iter()
                .any(|(hash, bytes)| u64::from(hash[0]) == slices * 8
                    && bytes.len() as u64 == slices * 8),
            "the encoded bytes were served as content"
        );
        assert!(
            loader
                .logs()
                .iter()
                .any(|(level, line)| *level == Level::WARN
                    && line.contains("slam.wav replaces the recording")
                    && line.contains("/creak.wav")),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_sound_whose_container_fails_after_its_recording_keeps_its_ids_and_may_retry() {
        // The archive holds no container to build a new one from, so the
        // container fails after the recording was counted and served.
        let loader = client(&[("horn.wav", wav(30))]);
        let horn = enshwick::content::Id::new("you.first:horn");
        let status = run(&loader, 1, |load| {
            for attempt in 0..2 {
                let failed = load
                    .add_sound(enshwick::sound::NewSound::new(horn, "horn.wav"))
                    .unwrap_err();
                assert!(
                    failed.to_string().contains("to build a new one from"),
                    "attempt {attempt}, not refused as a taken id: {failed}"
                );
            }
            Ok(())
        });
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let declared = loader.content_ids();
        for id in ["you.first:horn", "you.first:horn.sound"] {
            assert!(declared.iter().any(|d| d == id), "{id}: {declared:?}");
        }
        assert_eq!(
            loader.resources().len(),
            1,
            "the recording stays served under its id, each attempt patching it"
        );
    }
}
