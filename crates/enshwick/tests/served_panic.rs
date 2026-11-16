//! A served resource's builder that panics turns off only that resource;
//! one that returns `None` turns its mod off in the game's build, and in a
//! served read is refused and runs again.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::{Guid, ResourceKey, Status};
    use enshwick::{Load, Mod, Off};

    const BROKEN: ResourceKey = ResourceKey {
        guid: Guid([0xb0; 16]),
        type_hash: 0x0b0b_0b0b,
        part: 0,
    };
    const PICKY: ResourceKey = ResourceKey {
        guid: Guid([0x9c; 16]),
        ..BROKEN
    };
    const SOUND: ResourceKey = ResourceKey {
        guid: Guid([0x50; 16]),
        ..BROKEN
    };

    static BROKEN_RUNS: AtomicU32 = AtomicU32::new(0);
    static PICKY_RUNS: AtomicU32 = AtomicU32::new(0);

    struct Builders;

    impl Mod for Builders {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.serve(BROKEN, BROKEN, |_, _| {
                BROKEN_RUNS.fetch_add(1, Ordering::Relaxed);
                panic!("a broken builder");
            })?;
            load.serve(PICKY, PICKY, |_, _| {
                PICKY_RUNS.fetch_add(1, Ordering::Relaxed);
                None
            })?;
            load.serve(SOUND, SOUND, |base, out| out.copy_from(base))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Builders,
        r#"
        id = "you.builders"
        side = "server"
        "#
    );

    fn start(host: enshwick::test::FakeHost) -> enshwick::test::Installed {
        let loader = host.install();
        // SAFETY: the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn a_builder_that_panics_is_never_called_again_and_its_resource_is_named_off() {
        // The keys' type is an item's here, so the feature names it as the
        // loader does.
        let item = enshwick::content::types::ITEM_INFO.id();
        let loader = start(enshwick::test::FakeHost::new().type_hash(item, BROKEN.type_hash));
        for _ in 0..2 {
            assert_eq!(loader.build_resource(&BROKEN, &[1]), Err(Status::MISSING));
        }
        assert_eq!(BROKEN_RUNS.load(Ordering::Relaxed), 1, "called once only");
        let features = loader.features();
        let name = format!("served item {}", BROKEN.guid);
        let off = features.iter().find(|(have, _)| *have == name);
        assert!(
            off.is_some_and(|(_, why)| why.as_deref().is_some_and(|why| why.contains("panicked"))),
            "{features:?}"
        );
        assert!(!loader.is_off(), "the mod stays on");
        assert_eq!(loader.build_resource(&SOUND, &[3]), Ok(vec![3]));
    }

    #[test]
    fn a_builder_that_returns_none_in_the_game_s_build_turns_its_mod_off() {
        // Another test of this file may run its builder in this process.
        let before = PICKY_RUNS.load(Ordering::Relaxed);
        let loader = start(enshwick::test::FakeHost::new());
        assert_eq!(loader.build_resource(&PICKY, &[1]), Err(Status::MISSING));
        assert!(loader.is_off());
        let reason = format!("served resource {} failed to build", PICKY.guid);
        assert!(
            loader.logs().iter().any(|(_, line)| line.contains(&reason)),
            "{:?}",
            loader.logs()
        );
        assert_eq!(loader.build_resource(&PICKY, &[1]), Err(Status::MISSING));
        assert_eq!(
            PICKY_RUNS.load(Ordering::Relaxed) - before,
            1,
            "a mod that is off builds nothing"
        );
    }

    #[test]
    fn a_builder_that_returns_none_in_a_served_read_is_refused_and_runs_at_the_next_read() {
        let before = PICKY_RUNS.load(Ordering::Relaxed);
        let loader = start(enshwick::test::FakeHost::new().resource(PICKY, vec![1]));
        let host = enshwick::host().unwrap();
        for _ in 0..2 {
            assert_eq!(
                host.served_resource(PICKY).ok(),
                Some(vec![1]),
                "a changed key whose every patch failed reads as archived"
            );
        }
        assert_eq!(PICKY_RUNS.load(Ordering::Relaxed) - before, 2);
        assert!(!loader.is_off(), "the game never read that build");
        assert!(loader.features().is_empty());
    }
}
