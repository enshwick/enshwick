//! Sounds a mod adds from WAV files in its folder: checked the same on a
//! server, which serves nothing, and refused for another format, a path
//! outside the folder or a mod that does not run on both sides.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Side, Status};
    use enshwick::content::id;
    use enshwick::sound::{NewSound, SAMPLE_RATE, SoundChange, SoundId, Take, Takes};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    static RESULTS: Mutex<Vec<Result<(), String>>> = Mutex::new(Vec::new());

    /// A short mono 16-bit WAV file at `rate`.
    fn wav(rate: u32) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut file = std::io::Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut file, spec).unwrap();
        for i in 0..4800i16 {
            writer.write_sample(i).unwrap();
        }
        writer.finalize().unwrap();
        file.into_inner()
    }

    struct Horns;

    impl Mod for Horns {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            for (name, file) in [
                (id!("horn"), "horn.wav"),
                (id!("slow"), "slow.wav"),
                (id!("away"), "../horn.wav"),
                (id!("gone"), "gone.wav"),
            ] {
                let added = load.add_sound(NewSound::new(name, file));
                if let Ok(added) = &added {
                    assert_eq!(added.container.guid(), name.guid());
                    let sound = name.try_part("sound").map(enshwick::content::Id::guid);
                    assert_eq!(added.takes.first().map(|t| t.guid()), sound);
                }
                RESULTS
                    .lock()
                    .unwrap()
                    .push(added.map(|_| ()).map_err(|off| off.to_string()));
            }
            let (first, second) = clashing();
            for name in [first, second] {
                let added = load.add_sound(NewSound::new(name, "horn.wav"));
                RESULTS
                    .lock()
                    .unwrap()
                    .push(added.map(|_| ()).map_err(|off| off.to_string()));
            }
            let horn = SoundId::from_guid(id!("horn").guid());
            for container in [
                {
                    let mut value = SoundChange::default();
                    value.takes = Some(Takes::Replace(vec![Take::new(horn)]));
                    value
                },
                {
                    let mut value = SoundChange::default();
                    value.min_distance = Some(50.0);
                    value
                },
            ] {
                let added = load.add_sound({
                    let mut value = NewSound::new(id!("odd"), "horn.wav");
                    value.container = container;
                    value
                });
                RESULTS
                    .lock()
                    .unwrap()
                    .push(added.map(|_| ()).map_err(|off| off.to_string()));
            }
            Ok(Self)
        }
    }

    /// Two content ids of this mod whose game ids are one.
    fn clashing() -> (enshwick::content::Id, enshwick::content::Id) {
        let mut seen = rustc_hash::FxHashMap::default();
        for i in 0u32.. {
            let name: &'static str = Box::leak(format!("you.horns:n{i}").into_boxed_str());
            let game = enshwick::content::Guid::named(name).fnv1a();
            if let Some(first) = seen.insert(game, name) {
                return (
                    enshwick::content::Id::new(first),
                    enshwick::content::Id::new(name),
                );
            }
        }
        unreachable!("a 32-bit hash repeats")
    }

    enshwick::test::declare_mod!(
        Horns,
        r#"
        id = "you.horns"
        side = "both"
        "#
    );

    #[test]
    fn a_server_checks_each_file_and_serves_nothing() {
        let loader = FakeHost::new()
            .side(Side::SERVER)
            .mod_file("horn.wav", wav(SAMPLE_RATE))
            .mod_file("slow.wav", wav(44_100))
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Horns)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let results = RESULTS.lock().unwrap().clone();
        assert_eq!(results[0], Ok(()));
        assert_eq!(results[1], Ok(()), "a 44.1 kHz file is resampled");
        for (result, why) in results[2..4]
            .iter()
            .zip(["inside the mod's folder", "gone.wav"])
        {
            let refused = result.as_ref().unwrap_err();
            assert!(refused.contains(why), "{refused}");
        }
        assert_eq!(results[4], Ok(()));
        assert!(
            results[5].is_err(),
            "an id whose game id another holds is refused"
        );
        for (result, why) in results[6..8].iter().zip([
            "takes are its files",
            "least distance 50 is past the greatest 35",
        ]) {
            let refused = result.as_ref().unwrap_err();
            assert!(refused.contains(why), "refused as on a client: {refused}");
        }
        assert!(loader.resources().is_empty(), "a server plays no sound");
    }

    #[test]
    fn a_client_without_a_sound_to_build_from_declares_nothing_for_a_refused_sound() {
        let loader = FakeHost::new()
            .side(Side::CLIENT)
            .mod_file("horn.wav", wav(SAMPLE_RATE))
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Horns)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(
            RESULTS.lock().unwrap()[0].is_err(),
            "no sound to build from"
        );
        let declared = loader.content_ids();
        assert!(
            !declared.iter().any(|id| id.starts_with("you.horns:horn")),
            "{declared:?}"
        );
        assert!(loader.resources().is_empty());
    }
}
