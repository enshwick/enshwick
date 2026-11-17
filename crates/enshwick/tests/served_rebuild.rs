//! A rebuild runs the patches of every mod that built the bytes the game
//! holds, also of a mod turned off since, and then the keys whose builds
//! read it, as the loader's rebuild does; it refuses as the loader refuses a
//! mod that is off, a key the mod does not serve, a key whose base was not
//! kept and a build of another length.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

    use enshwick::abi::{Guid, ModHandle, ResourceKey, Side, Status};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};

    /// The data pack's handle; the fake gives the mod 1.
    const PACK: ModHandle = ModHandle(2);

    const fn key(byte: u8) -> ResourceKey {
        ResourceKey {
            guid: Guid([byte; 16]),
            type_hash: 0x0b0b_0b0b,
            part: 0,
        }
    }

    /// The pack copies it, then the mod writes `VALUE` over it.
    const BUILT: ResourceKey = key(0xbb);
    /// The pack's, whose every build fails, which turns the pack off.
    const FAILING: ResourceKey = key(0xfa);
    /// The mod's, whose patch reads `BUILT` as served.
    const READER: ResourceKey = key(0xee);
    /// The mod's, served without asking for a rebuild.
    const UNKEPT: ResourceKey = key(0x0c);
    /// The mod's, one byte longer at each build.
    const GROWING: ResourceKey = key(0x6e);
    /// The mod's, whose builder panics once `PANIC` is set.
    const PANICKY: ResourceKey = key(0x9a);
    /// The mod's, whose patch rebuilds `BUILT` once `NEST` is set.
    const NESTING: ResourceKey = key(0x4e);

    /// The byte the mod's patch of `BUILT` writes.
    static VALUE: AtomicU8 = AtomicU8::new(0);
    /// How many times the pack's patch of `BUILT` ran.
    static PACK_RUNS: AtomicU32 = AtomicU32::new(0);
    /// How many times the patch of `READER` ran.
    static READER_RUNS: AtomicU32 = AtomicU32::new(0);
    /// How many times the patch of `GROWING` ran.
    static GROWTH: AtomicU8 = AtomicU8::new(0);
    static PANIC: AtomicBool = AtomicBool::new(false);
    static NEST: AtomicBool = AtomicBool::new(false);

    struct Builder;

    impl Mod for Builder {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.serve(BUILT, BUILT, |_, out| {
                out.copy_from(&[VALUE.load(Ordering::Relaxed)])
            })?;
            load.allow_rebuild(BUILT)?;
            load.serve(READER, READER, |base, out| {
                READER_RUNS.fetch_add(1, Ordering::Relaxed);
                enshwick::host()?.served_resource(BUILT).ok()?;
                out.copy_from(base)
            })?;
            load.serve(UNKEPT, UNKEPT, |base, out| out.copy_from(base))?;
            load.serve(GROWING, GROWING, |_, out| {
                let len = GROWTH.fetch_add(1, Ordering::Relaxed) + 1;
                out.copy_from(&vec![0; usize::from(len)])
            })?;
            load.allow_rebuild(GROWING)?;
            load.serve(PANICKY, PANICKY, |base, out| {
                assert!(!PANIC.load(Ordering::Relaxed), "the builder panics");
                out.copy_from(base)
            })?;
            load.allow_rebuild(PANICKY)?;
            load.serve(NESTING, NESTING, |base, out| {
                if NEST.load(Ordering::Relaxed) {
                    assert_eq!(rebuild(BUILT), Status::OK);
                }
                out.copy_from(base)
            })?;
            load.allow_rebuild(NESTING)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Builder,
        r#"
        id = "you.builder"
        side = "server"
        "#
    );

    fn start() -> Installed {
        VALUE.store(0, Ordering::Relaxed);
        GROWTH.store(0, Ordering::Relaxed);
        PANIC.store(false, Ordering::Relaxed);
        let loader = FakeHost::new()
            .resource(BUILT, vec![5])
            .resource(PANICKY, vec![1])
            .pack(PACK, Side::SERVER)
            .install();
        // SAFETY: the fake host's table lives for the process.
        let status = unsafe {
            enshwick::__private::init_pack(loader.api(), PACK, Side::SERVER, |load| {
                load.serve(BUILT, BUILT, |base, out| {
                    PACK_RUNS.fetch_add(1, Ordering::Relaxed);
                    out.copy_from(base)
                })?;
                load.serve(FAILING, FAILING, |_, _| None)
            })
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        // SAFETY: the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// What the loader answers the mod's rebuild of `key`.
    fn rebuild(key: ResourceKey) -> Status {
        let host = enshwick::host().unwrap();
        (host.api().rebuild_resource)(host.handle(), &raw const key)
    }

    #[test]
    fn a_rebuild_runs_the_patch_of_a_pack_turned_off_since_the_game_s_first_build() {
        let loader = start();
        assert_eq!(loader.build_resource(&BUILT, &[5]), Ok(vec![0]));
        assert_eq!(loader.build_resource(&FAILING, &[1]), Err(Status::MISSING));
        assert!(!loader.is_off(), "only the pack turned off");
        let before = PACK_RUNS.load(Ordering::Relaxed);
        let host = enshwick::host().unwrap();
        assert_eq!(host.served_resource(BUILT).ok(), Some(vec![0]));
        assert_eq!(loader.build_resource(&BUILT, &[5]), Ok(vec![0]));
        assert_eq!(
            PACK_RUNS.load(Ordering::Relaxed),
            before,
            "a served read and a later game build skip the off pack"
        );
        assert_eq!(rebuild(BUILT), Status::OK);
        assert_eq!(
            PACK_RUNS.load(Ordering::Relaxed),
            before + 1,
            "the rebuild ran the patches of the game's first build"
        );
    }

    #[test]
    fn a_rebuild_builds_again_the_keys_whose_build_read_it_once_it_changed() {
        let loader = start();
        assert!(loader.build_resource(&BUILT, &[5]).is_ok());
        assert_eq!(loader.build_resource(&READER, &[1]), Ok(vec![1]));
        let before = READER_RUNS.load(Ordering::Relaxed);
        assert_eq!(rebuild(BUILT), Status::OK);
        assert_eq!(
            READER_RUNS.load(Ordering::Relaxed),
            before,
            "unchanged, so its reader stays"
        );
        VALUE.store(9, Ordering::Relaxed);
        assert_eq!(rebuild(BUILT), Status::OK);
        assert_eq!(
            READER_RUNS.load(Ordering::Relaxed),
            before + 1,
            "changed, so its reader built again"
        );
    }

    #[test]
    fn a_rebuild_is_refused_where_the_loader_cannot_build_it() {
        let loader = start();
        assert!(loader.build_resource(&UNKEPT, &[1]).is_ok());
        assert_eq!(rebuild(UNKEPT), Status::MISSING, "no base kept");
        assert_eq!(loader.build_resource(&GROWING, &[0]), Ok(vec![0]));
        assert_eq!(rebuild(GROWING), Status::REFUSED, "another length");
        assert_eq!(rebuild(FAILING), Status::BAD_ARGS, "the pack's key");
        enshwick::host().unwrap().fault("turned off by the test");
        assert_eq!(rebuild(BUILT), Status::FAULTED, "the mod is off");
    }

    #[test]
    fn a_builder_that_panics_again_in_a_rebuild_is_named_off_once() {
        let loader = start();
        assert_eq!(loader.build_resource(&PANICKY, &[1]), Ok(vec![1]));
        PANIC.store(true, Ordering::Relaxed);
        let host = enshwick::host().unwrap();
        assert_eq!(
            host.served_resource(PANICKY).ok(),
            Some(vec![1]),
            "with its only patch failed, read as archived"
        );
        let named = || {
            loader
                .logs()
                .iter()
                .filter(|(_, line)| line.contains("feature served"))
                .count()
        };
        assert_eq!(named(), 1, "{:?} {:?}", loader.logs(), loader.features());
        assert_eq!(rebuild(PANICKY), Status::MISSING, "no patch built");
        assert_eq!(named(), 1, "named off once");
    }

    #[test]
    fn a_rebuild_a_patch_asks_for_is_noted_after_the_rebuild_running_it() {
        let loader = start();
        assert!(loader.build_resource(&BUILT, &[5]).is_ok());
        assert!(loader.build_resource(&NESTING, &[1]).is_ok());
        NEST.store(true, Ordering::Relaxed);
        assert_eq!(rebuild(NESTING), Status::OK);
        assert!(
            loader.rebuilds().ends_with(&[NESTING, BUILT]),
            "{:?}",
            loader.rebuilds()
        );
    }

    #[test]
    fn a_rebuild_of_a_key_the_game_never_built_runs_no_patch() {
        let loader = start();
        let before = PACK_RUNS.load(Ordering::Relaxed);
        assert_eq!(rebuild(BUILT), Status::OK);
        assert_eq!(PACK_RUNS.load(Ordering::Relaxed), before);
        assert_eq!(
            loader.rebuilds(),
            [BUILT, GROWING, PANICKY, NESTING, BUILT],
            "allowed at init, asked now"
        );
    }
}
