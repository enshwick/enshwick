//! A served resource's builder that panics turns off only that resource;
//! one that returns `None` is refused and runs again.

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

    #[test]
    fn a_builder_that_panics_is_never_called_again_and_its_resource_is_named_off() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        for _ in 0..2 {
            assert_eq!(loader.build_resource(&BROKEN, &[1]), Err(Status::MISSING));
        }
        assert_eq!(BROKEN_RUNS.load(Ordering::Relaxed), 1, "called once only");
        let features = loader.features();
        let off = features
            .iter()
            .find(|(name, _)| name.contains(&BROKEN.guid.to_string()));
        assert!(
            off.is_some_and(|(_, why)| why.as_deref().is_some_and(|why| why.contains("panicked"))),
            "{features:?}"
        );
        assert!(!loader.is_off(), "the mod stays on");
        assert_eq!(loader.build_resource(&SOUND, &[3]), Ok(vec![3]));
    }

    #[test]
    fn a_builder_that_returns_none_is_refused_and_runs_at_the_next_build() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        for _ in 0..2 {
            assert_eq!(loader.build_resource(&PICKY, &[1]), Err(Status::MISSING));
        }
        assert_eq!(PICKY_RUNS.load(Ordering::Relaxed), 2);
        assert!(loader.features().is_empty());
    }
}
