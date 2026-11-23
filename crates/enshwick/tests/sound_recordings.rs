//! A mod's recordings on a client whose game holds an uncompressed sound to
//! build from: a replaced vanilla recording streams the mod's samples, and
//! two mods replacing one apply in load order with the log naming both.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Level, ModHandle, Side, Status};
    use enshwick::content::{Guid, types};
    use enshwick::resources::{Constant, Field, Kind, Registry, Type, Value, fnv1a32};
    use enshwick::sound::{SAMPLE_RATE, SoundId};
    use enshwick::test::{FakeHost, Installed};

    const SOUND: &str = "demo::SoundResource";
    /// The game's one streamed uncompressed sound, which mods build from.
    const WATERFALL: Guid = Guid([1; 16]);
    /// A vanilla recording held inline, which mods replace.
    const CREAK: Guid = Guid([2; 16]);

    fn types() -> Vec<Type<'static>> {
        let uint = |name| Type::new(name, Kind::Uint32, 4, 4);
        vec![
            Type::new("demo::uint8", Kind::Uint8, 1, 1),
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
            Type::new(SOUND, Kind::Struct, 56, 8).with_fields(vec![
                Field::new("format", 0, "demo::SoundDataFormat"),
                Field::new("channelConfiguration", 1, "demo::SoundChannelConfiguration"),
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
        let waterfall = sound("Uncompressed", 9, Vec::new(), 18, 0xb3c9_d834);
        let creak = sound("FramelessQOA", 2, vec![Value::Uint(7); 8], 0, 0);
        let mut host = FakeHost::new()
            .side(Side::CLIENT)
            .types(types())
            .type_hash(types::SOUND_RESOURCE.id(), sound_hash())
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
        let loader = client(&[("creak.wav", wav(30)), ("slam.wav", wav(70))]);
        let creak = SoundId::from_guid(CREAK);
        for (me, file) in [(1, "creak.wav"), (2, "slam.wav")] {
            let status = run(&loader, me, |load| load.replace_sound(creak, file));
            assert_eq!(status, Status::OK, "{:?}", loader.logs());
        }
        let refused = run(&loader, 3, |load| {
            load.replace_sound(SoundId::from_guid(Guid([9; 16])), "creak.wav")
        });
        assert_ne!(refused, Status::OK, "a recording the game lacks");
        let built = built_creak(&loader);
        assert_eq!(
            built.field("format"),
            Some(&Value::Name("Uncompressed".into()))
        );
        assert_eq!(
            built.field("frameCount"),
            Some(&Value::Uint(70)),
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
        assert_eq!(size, Some(&Value::Uint(140)), "70 frames of 2 bytes");
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
}
