//! Voiced lines and music on a client: a mod's line in two languages and a
//! fallback fills every slot, a game's line changed in one language keeps
//! the others, and music rows of two mods add up in load order.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ModHandle, Side, Status};
    use enshwick::content::{Guid, Id, types};
    use enshwick::resources::{Constant, Field, Kind, Registry, Type, Value, fnv1a32};
    use enshwick::sound::{NewMusic, NewVoiceover, SAMPLE_RATE, VoiceLine, VoiceoverId};
    use enshwick::test::{FakeHost, Installed};

    const SOUND: &str = "demo::SoundResource";
    const CONTAINER: &str = "demo::SoundContainerResource";
    const ENTRY: &str = "demo::SoundContainerResourceSoundEntry";
    const VOICEOVER: &str = "demo::VoiceoverResource";
    const LANGUAGES: &str = "demo::Voiceover";
    const COLLECTION: &str = "demo::AmbientSfxCollectionResource";
    const ENVIRONMENT: &str = "demo::AmbientEnvironment";
    /// The game's streamed and inline sounds, which mods build from.
    const WATERFALL: Guid = Guid([1; 16]);
    const CREAK: Guid = Guid([2; 16]);
    /// A game container, which a mod's new one builds from.
    const CHEST: Guid = Guid([3; 16]);
    /// A game line voiced in English and German, not in Chinese.
    const READING: Guid = Guid([4; 16]);
    const READING_EN: Guid = Guid([5; 16]);
    const READING_DE: Guid = Guid([6; 16]);
    /// The game's ambience and its one music row's two recordings.
    const AMBIENCE: Guid = Guid([7; 16]);
    const TUNE: [Guid; 2] = [Guid([8; 16]), Guid([9; 16])];

    /// The types of sounds and their containers, which both sides reflect.
    fn sound_types() -> Vec<Type<'static>> {
        let scalar = |name, kind, size: u16| Type::new(name, kind, size.into(), size);
        let enumeration = |name, names: &[&'static str]| {
            Type::new(name, Kind::Enum, 1, 1).with_constants(
                (0u64..)
                    .zip(names)
                    .map(|(value, name)| Constant::new(name, value))
                    .collect(),
            )
        };
        let f = Field::new;
        vec![
            scalar("demo::bool", Kind::Bool, 1),
            scalar("demo::uint8", Kind::Uint8, 1),
            scalar("demo::uint16", Kind::Uint16, 2),
            scalar("demo::uint32", Kind::Uint32, 4),
            scalar("demo::sint64", Kind::Sint64, 8),
            scalar("demo::float32", Kind::Float32, 4),
            Type::new("demo::Guid", Kind::Guid, 16, 4),
            Type::new("demo::BlobString", Kind::BlobString, 8, 4),
            Type::new("demo::BlobArray<uint8>", Kind::BlobArray, 8, 4).with_inner("demo::uint8"),
            Type::new("demo::BlobArray<Guid>", Kind::BlobArray, 8, 4).with_inner("demo::Guid"),
            enumeration("demo::SoundDataFormat", &["Uncompressed", "FramelessQOA"]),
            enumeration("demo::SoundChannelConfiguration", &["Mono", "Stereo"]),
            enumeration("demo::Mode", &["Random"]),
            enumeration("demo::Scene", &["Sfx", "Music", "Ui", "Voice"]),
            enumeration("demo::EitherOrBoth", &["No", "Yes", "Any"]),
            enumeration(
                "demo::PlaybackType",
                &[
                    "PlayAlways",
                    "PlayOnlyWhenInBattle",
                    "PlayOnlyWhenNotInBattle",
                ],
            ),
            enumeration("demo::SfxFlag", &["InAllEnvironments", "OnlyUnderwater"]),
            enumeration(ENVIRONMENT, &["None", "Grassland", "Desert"]),
            Type::new("demo::SfxFlags", Kind::Bitmask32, 4, 4).with_inner("demo::SfxFlag"),
            Type::new("demo::Time", Kind::Struct, 8, 8).with_fields(vec![f(
                "value",
                0,
                "demo::sint64",
            )]),
            Type::new("demo::ContentHash", Kind::Struct, 16, 4).with_fields(vec![
                f("size", 0, "demo::uint32"),
                f("hash0", 4, "demo::uint32"),
                f("hash1", 8, "demo::uint32"),
                f("hash2", 12, "demo::uint32"),
            ]),
            Type::new("demo::ResourceContentReference", Kind::Struct, 20, 4).with_fields(vec![
                f("hash", 0, "demo::ContentHash"),
                f("category", 16, "demo::uint32"),
            ]),
            Type::new(SOUND, Kind::Struct, 56, 8).with_fields(vec![
                f("format", 0, "demo::SoundDataFormat"),
                f("channelConfiguration", 1, "demo::SoundChannelConfiguration"),
                f("framesPerSecond", 2, "demo::uint16"),
                f("frameCount", 4, "demo::uint32"),
                f("duration", 8, "demo::Time"),
                f("data", 16, "demo::BlobArray<uint8>"),
                f("streamedData", 24, "demo::ResourceContentReference"),
                f("debugName", 44, "demo::BlobString"),
            ]),
            Type::new("demo::SoundContainerResourceEntry", Kind::Struct, 4, 4),
            Type::new(ENTRY, Kind::Struct, 20, 4)
                .with_inner("demo::SoundContainerResourceEntry")
                .with_fields(vec![
                    f("chance", 0, "demo::float32"),
                    f("sound", 4, "demo::Guid"),
                ]),
            Type::new("demo::Variant", Kind::BlobVariant, 12, 4)
                .with_inner("demo::SoundContainerResourceEntry"),
            Type::new("demo::Entries", Kind::BlobArray, 8, 4).with_inner("demo::Variant"),
            Type::new(CONTAINER, Kind::Struct, 40, 4).with_fields(vec![
                f("mode", 0, "demo::Mode"),
                f("scene", 1, "demo::Scene"),
                f("hasPosition", 2, "demo::bool"),
                f("volume", 4, "demo::float32"),
                f("pitch", 8, "demo::float32"),
                f("minDistance", 12, "demo::float32"),
                f("maxDistance", 16, "demo::float32"),
                f("gamepadSpeakerVolume", 20, "demo::float32"),
                f("gamepadVibrationIntensity", 24, "demo::float32"),
                f("priority", 28, "demo::uint32"),
                f("entries", 32, "demo::Entries"),
            ]),
        ]
    }

    /// The client's types: sounds, voiced lines and the ambience's music.
    fn types() -> Vec<Type<'static>> {
        let mut types = sound_types();
        types.extend(voice_and_ambience_types());
        types
    }

    /// The types of voiced lines and of the ambience's music.
    fn voice_and_ambience_types() -> Vec<Type<'static>> {
        let f = Field::new;
        vec![
            Type::new(LANGUAGES, Kind::Struct, 48, 4).with_fields(vec![
                f("english", 0, "demo::Guid"),
                f("german", 16, "demo::Guid"),
                f("simplifiedChinese", 32, "demo::Guid"),
            ]),
            Type::new("demo::StaticArray<Guid,3>", Kind::StaticArray, 48, 4)
                .with_inner("demo::Guid")
                .with_count(3),
            Type::new("demo::StaticArray<uint16,3>", Kind::StaticArray, 6, 2)
                .with_inner("demo::uint16")
                .with_count(3),
            Type::new(VOICEOVER, Kind::Struct, 56, 4).with_fields(vec![
                f("languages", 0, "demo::StaticArray<Guid,3>"),
                f("durations", 48, "demo::StaticArray<uint16,3>"),
            ]),
            Type::new("demo::AmbientSfxResource", Kind::Struct, 72, 8).with_fields(vec![
                f("flags", 0, "demo::SfxFlags"),
                f("inFog", 4, "demo::EitherOrBoth"),
                f("indoors", 5, "demo::EitherOrBoth"),
                f("environment", 6, ENVIRONMENT),
                f("playbackType", 7, "demo::PlaybackType"),
                f("startTime", 8, "demo::Time"),
                f("endTime", 16, "demo::Time"),
                f("volume", 24, "demo::float32"),
                f("avoidRepeatingLastX", 28, "demo::uint32"),
                f("originalVolume", 32, "demo::float32"),
                f("sleep", 40, "demo::Time"),
                f("sleepRandomness", 48, "demo::Time"),
                f("soundStart", 56, "demo::uint32"),
                f("soundCount", 60, "demo::uint32"),
                f("pitch", 64, "demo::float32"),
            ]),
            Type::new("demo::Rows", Kind::BlobArray, 8, 4).with_inner("demo::AmbientSfxResource"),
            Type::new(COLLECTION, Kind::Struct, 16, 4).with_fields(vec![
                f("music", 0, "demo::Rows"),
                f("sounds", 8, "demo::BlobArray<Guid>"),
            ]),
        ]
    }

    fn time(nanos: i64) -> Value {
        Value::Struct(vec![("value".into(), Value::Int(nanos))])
    }

    fn sound(frames: u64, data: Vec<Value>, size: u64) -> Value {
        let word = |name: &str, value: u64| (name.to_owned(), Value::Uint(value));
        Value::Struct(vec![
            ("format".into(), Value::Name("FramelessQOA".into())),
            ("channelConfiguration".into(), Value::Name("Mono".into())),
            word("framesPerSecond", u64::from(SAMPLE_RATE)),
            word("frameCount", frames),
            ("duration".into(), time(1)),
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
                    word("category", 1),
                ]),
            ),
            ("debugName".into(), Value::Text("vanilla".into())),
        ])
    }

    fn slots(containers: [Guid; 3], lengths: [u64; 3]) -> Value {
        Value::Struct(vec![
            (
                "languages".into(),
                Value::List(containers.map(Value::Guid).to_vec()),
            ),
            (
                "durations".into(),
                Value::List(lengths.map(Value::Uint).to_vec()),
            ),
        ])
    }

    fn ambience() -> Value {
        let row = Value::Struct(vec![
            ("flags".into(), Value::Flags(Vec::new())),
            ("inFog".into(), Value::Name("No".into())),
            ("indoors".into(), Value::Name("No".into())),
            ("environment".into(), Value::Name("Desert".into())),
            (
                "playbackType".into(),
                Value::Name("PlayOnlyWhenNotInBattle".into()),
            ),
            ("startTime".into(), time(0)),
            ("endTime".into(), time(86_400_000_000_000)),
            ("volume".into(), Value::Float(1.0)),
            ("avoidRepeatingLastX".into(), Value::Uint(1)),
            ("originalVolume".into(), Value::Float(0.2)),
            ("sleep".into(), time(0)),
            ("sleepRandomness".into(), time(0)),
            ("soundStart".into(), Value::Uint(0)),
            ("soundCount".into(), Value::Uint(2)),
            ("pitch".into(), Value::Float(1.0)),
        ]);
        Value::Struct(vec![
            ("music".into(), Value::List(vec![row])),
            ("sounds".into(), Value::List(TUNE.map(Value::Guid).to_vec())),
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

    fn hash(name: &str) -> u32 {
        fnv1a32(name.as_bytes())
    }

    fn encode(name: &str, value: &Value) -> Vec<u8> {
        Registry::new(types()).encode(name, value, None).unwrap()
    }

    /// A client whose archive holds the sounds, a container, the reading,
    /// the ambience, and whose mods' folder holds 50 ms and 100 ms files.
    fn client() -> Installed {
        FakeHost::new()
            .side(Side::CLIENT)
            .pack(ModHandle(2), Side::BOTH)
            .types(types())
            .type_hash(types::SOUND_RESOURCE.id(), hash(SOUND))
            .type_hash(types::SOUND_CONTAINER_RESOURCE.id(), hash(CONTAINER))
            .type_hash("sound_container_sound_entry", hash(ENTRY))
            .type_hash("voiceover", hash(VOICEOVER))
            .type_hash("voiceover_languages", hash(LANGUAGES))
            .type_hash("ambient_sfx_collection", hash(COLLECTION))
            .type_hash("ambient_environment", hash(ENVIRONMENT))
            .resource(
                WATERFALL.key_by_hash(hash(SOUND)),
                encode(SOUND, &sound(9, Vec::new(), 24)),
            )
            .resource(
                CREAK.key_by_hash(hash(SOUND)),
                encode(SOUND, &sound(2, vec![Value::Uint(7); 8], 0)),
            )
            .resource(
                CHEST.key_by_hash(hash(CONTAINER)),
                encode(CONTAINER, &Value::Struct(Vec::new())),
            )
            .resource(READING.key_by_hash(hash(VOICEOVER)), reading())
            .resource(
                AMBIENCE.key_by_hash(hash(COLLECTION)),
                encode(COLLECTION, &ambience()),
            )
            .mod_file("short.wav", wav(2_400))
            .mod_file("long.wav", wav(4_800))
            .install()
    }

    fn reading() -> Vec<u8> {
        encode(
            VOICEOVER,
            &slots([READING_EN, READING_DE, Guid::NIL], [100, 120, 0]),
        )
    }

    /// The resource `key` of type `name` as the client builds it from `base`.
    fn built(loader: &Installed, name: &str, key: Guid, base: &[u8]) -> Value {
        let bytes = loader
            .build_resource(&key.key_by_hash(hash(name)), base)
            .unwrap();
        Registry::new(types()).decode(name, &bytes).unwrap()
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
    fn a_line_in_two_languages_with_a_fallback_fills_every_slot() {
        let loader = client();
        let welcome = Id::new("you.first:welcome");
        let status = run(&loader, 1, |load| {
            let line = load.add_voiceover({
                let mut value = NewVoiceover::new(
                    welcome,
                    vec![
                        VoiceLine::new("english", "short.wav"),
                        VoiceLine::new("simplified_chinese", "long.wav"),
                    ],
                );
                value.fallback = Some("english".into());
                value
            })?;
            assert_eq!(line, VoiceoverId::from_guid(welcome.guid()));
            let unknown = NewVoiceover::new(
                Id::new("you.first:odd"),
                vec![VoiceLine::new("klingon", "short.wav")],
            );
            let refused = load.add_voiceover(unknown).unwrap_err().to_string();
            assert!(
                refused.contains("english, german, simplified_chinese"),
                "{refused}"
            );
            Ok(())
        });
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let english = Guid::named("you.first:welcome.english");
        let chinese = Guid::named("you.first:welcome.simplified_chinese");
        assert_eq!(
            built(&loader, VOICEOVER, welcome.guid(), &reading()),
            slots([english, english, chinese], [5, 5, 10]),
            "German speaks the English fallback; lengths in 10 ms"
        );
        assert!(
            loader
                .content_ids()
                .iter()
                .any(|id| id == "you.first:welcome.english.sound"),
            "{:?}",
            loader.content_ids()
        );
    }

    #[test]
    fn a_game_line_changed_in_one_language_keeps_the_others() {
        let loader = client();
        let status = run(&loader, 1, |load| {
            let german = NewVoiceover::new(
                Id::new("you.first:reading"),
                vec![VoiceLine::new("german", "long.wav")],
            );
            load.change_voiceover(VoiceoverId::from_guid(READING), german)?;
            let missing = NewVoiceover::new(
                Id::new("you.first:none"),
                vec![VoiceLine::new("german", "long.wav")],
            );
            let refused = load.change_voiceover(VoiceoverId::from_guid(Guid([99; 16])), missing);
            assert!(refused.unwrap_err().to_string().contains("no such line"));
            Ok(())
        });
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let german = Guid::named("you.first:reading.german");
        assert_eq!(
            built(&loader, VOICEOVER, READING, &reading()),
            slots([READING_EN, german, Guid::NIL], [100, 10, 0]),
            "Chinese stays silent without a fallback"
        );
    }

    #[test]
    fn a_server_without_the_voice_and_ambience_types_still_checks_lines_and_music() {
        let loader = FakeHost::new()
            .side(Side::SERVER)
            .types(sound_types())
            .type_hash(types::SOUND_RESOURCE.id(), hash(SOUND))
            .type_hash(types::SOUND_CONTAINER_RESOURCE.id(), hash(CONTAINER))
            .mod_file("short.wav", wav(2_400))
            .install();
        let status = run(&loader, 1, |load| {
            load.add_voiceover(NewVoiceover::new(
                Id::new("you.first:welcome"),
                vec![VoiceLine::new("english", "short.wav")],
            ))?;
            let night = {
                let mut value = NewMusic::new(Id::new("you.first:night"), vec!["short.wav".into()]);
                value.environment = Some("Grassland".into());
                value
            };
            load.add_music(night).map(|_| ())
        });
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
    }

    #[test]
    fn music_rows_of_two_mods_add_up_in_load_order() {
        let loader = client();
        let first = run(&loader, 1, |load| {
            let night = {
                let mut value = NewMusic::new(
                    Id::new("you.first:night"),
                    vec!["short.wav".into(), "long.wav".into()],
                );
                value.environment = Some("Grassland".into());
                value.in_battle = Some(false);
                value
            };
            load.add_music(night)?;
            let moon = {
                let mut value = NewMusic::new(Id::new("you.first:moon"), vec!["short.wav".into()]);
                value.environment = Some("Moon".into());
                value
            };
            let refused = load.add_music(moon).unwrap_err().to_string();
            assert!(refused.contains("None, Grassland, Desert"), "{refused}");
            Ok(())
        });
        assert_eq!(first, Status::OK, "{:?}", loader.logs());
        let second = run(&loader, 2, |load| {
            let day = NewMusic::new(Id::new("you.second:day"), vec!["short.wav".into()]);
            load.add_music(day).map(|_| ())
        });
        assert_eq!(second, Status::OK, "{:?}", loader.logs());
        let built = built(
            &loader,
            COLLECTION,
            AMBIENCE,
            &encode(COLLECTION, &ambience()),
        );
        let Some(Value::List(music)) = built.field("music") else {
            panic!("no music: {built:?}");
        };
        assert_eq!(music.len(), 3);
        let at = |row: &Value, name: &str| row.field(name).cloned();
        assert_eq!(
            at(&music[1], "environment"),
            Some(Value::Name("Grassland".into()))
        );
        assert_eq!(
            at(&music[1], "playbackType"),
            Some(Value::Name("PlayOnlyWhenNotInBattle".into()))
        );
        assert_eq!(at(&music[1], "soundStart"), Some(Value::Uint(2)));
        assert_eq!(at(&music[1], "soundCount"), Some(Value::Uint(2)));
        assert_eq!(
            at(&music[2], "flags"),
            Some(Value::Flags(vec!["InAllEnvironments".into()]))
        );
        assert_eq!(at(&music[2], "soundStart"), Some(Value::Uint(4)));
        let named = |id: &str| Value::Guid(Guid::named(id));
        assert_eq!(
            built.field("sounds"),
            Some(&Value::List(vec![
                Value::Guid(TUNE[0]),
                Value::Guid(TUNE[1]),
                named("you.first:night.sound"),
                named("you.first:night.sound2"),
                named("you.second:day.sound"),
            ]))
        );
    }
}
