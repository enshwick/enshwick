//! A served read whose every patch fails reads a changed vanilla item as
//! the archive has it and a new item as missing, and served reads and
//! rebuilds free every buffer their patches took.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use enshwick::abi::Status;
    use enshwick::content::{Guid, Id, types};
    use enshwick::test::{FakeHost, Installed, OTHER_ITEM, SPARE_ITEM};
    use enshwick::{Load, Mod, Off};

    /// An item the mod adds from `SPARE_ITEM`, whose build always fails.
    const NEW_ID: Id = Id::new("you.failing:new");
    const NEW_ITEM: Guid = NEW_ID.guid();

    /// How many times the patch of `OTHER_ITEM` ran.
    static BUILDS: AtomicUsize = AtomicUsize::new(0);

    struct Failing;

    impl Mod for Failing {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let spare = types::ITEM_INFO.key(SPARE_ITEM)?;
            load.serve(spare, spare, |_, _| None)?;
            load.declare(NEW_ID)?;
            load.serve(types::ITEM_INFO.key(NEW_ITEM)?, spare, |_, _| None)?;
            let other = types::ITEM_INFO.key(OTHER_ITEM)?;
            load.serve(other, other, |item, out| {
                BUILDS.fetch_add(1, Ordering::Relaxed);
                out.copy_from(item)
            })?;
            load.allow_rebuild(other)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Failing,
        r#"
        id = "you.failing"
        side = "both"
        "#
    );

    fn start() -> Installed {
        let loader = FakeHost::new().items().removal().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Failing)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    #[test]
    fn a_changed_item_whose_every_patch_fails_reads_as_archived() {
        let _loader = start();
        let host = enshwick::host().unwrap();
        let spare = types::ITEM_INFO.key(SPARE_ITEM).unwrap();
        assert_eq!(host.served_resource(spare).ok(), host.game_resource(spare));
    }

    #[test]
    fn served_reads_free_every_buffer_their_patches_took() {
        let loader = start();
        let host = enshwick::host().unwrap();
        let other = types::ITEM_INFO.key(OTHER_ITEM).unwrap();
        for _ in 0..10 {
            assert_eq!(host.served_resource(other).ok(), host.game_resource(other));
        }
        assert_eq!(loader.lent_buffers(), 0, "each read freed its output");
    }

    #[test]
    fn a_rebuild_runs_the_patch_again_and_frees_the_output_it_built() {
        let loader = start();
        let other = types::ITEM_INFO.key(OTHER_ITEM).unwrap();
        let archived = enshwick::host().unwrap().game_resource(other).unwrap();
        assert!(loader.build_resource(&other, &archived).is_ok());
        let before = BUILDS.load(Ordering::Relaxed);
        for _ in 0..10 {
            enshwick::content::rebuild(other).unwrap();
        }
        assert!(
            BUILDS.load(Ordering::Relaxed) >= before + 10,
            "each rebuild ran the patch"
        );
        assert_eq!(loader.lent_buffers(), 0, "each rebuild freed its output");
    }

    #[test]
    fn a_new_item_whose_every_patch_fails_reads_as_missing() {
        let _loader = start();
        let host = enshwick::host().unwrap();
        let new = types::ITEM_INFO.key(NEW_ITEM).unwrap();
        assert!(host.served_resource(new).is_err());
    }
}
