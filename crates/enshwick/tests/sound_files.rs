//! Sounds a mod adds from WAV files in its folder: checked the same on a
//! server, which serves nothing, and refused for another format, a path
//! outside the folder or a mod that does not run on both sides.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Side, Status};
    use enshwick::content::id;
    use enshwick::sound::{NewSound, SAMPLE_RATE};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    static RESULTS: Mutex<Vec<Result<(), String>>> = Mutex::new(Vec::new());

    fn write_wav(path: &std::path::Path, rate: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..4800i16 {
            writer.write_sample(i).unwrap();
        }
        writer.finalize().unwrap();
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
                if let Ok(container) = added {
                    assert_eq!(container.guid(), name.guid());
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
            Ok(Self)
        }
    }

    /// Two content ids of this mod whose game ids are one.
    fn clashing() -> (enshwick::content::Id, enshwick::content::Id) {
        let mut seen = std::collections::HashMap::new();
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
        let dir = std::env::temp_dir().join(format!("enshwick-sound-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_wav(&dir.join("horn.wav"), SAMPLE_RATE);
        write_wav(&dir.join("slow.wav"), 44_100);
        let loader = FakeHost::new()
            .side(Side::SERVER)
            .mod_dir(dir.to_str().unwrap())
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Horns)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let results = RESULTS.lock().unwrap().clone();
        assert_eq!(results[0], Ok(()));
        for (result, why) in
            results[1..4]
                .iter()
                .zip(["48000 Hz", "inside the mod's folder", "gone.wav"])
        {
            let refused = result.as_ref().unwrap_err();
            assert!(refused.contains(why), "{refused}");
        }
        assert_eq!(results[4], Ok(()));
        assert!(
            results[5].is_err(),
            "an id whose game id another holds is refused"
        );
        assert!(loader.resources().is_empty(), "a server plays no sound");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
