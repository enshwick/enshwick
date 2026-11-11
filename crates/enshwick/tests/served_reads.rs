//! A served read whose every patch fails reads a changed vanilla item as
//! the archive has it and a new item as missing, and served reads keep no
//! output of a patch built through the API.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::{Guid, types};
    use enshwick::test::{FakeHost, Installed, OTHER_ITEM, SPARE_ITEM};
    use enshwick::{Load, Mod, Off};

    /// An item the mod adds from `SPARE_ITEM`, whose build always fails.
    const NEW_ITEM: Guid = Guid::parse("5e7ead00-0000-4000-8000-000000000001");

    struct Failing;

    impl Mod for Failing {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let spare = types::ITEM_INFO.key(SPARE_ITEM)?;
            load.serve(spare, spare, |_| None)?;
            load.serve(types::ITEM_INFO.key(NEW_ITEM)?, spare, |_| None)?;
            let other = types::ITEM_INFO.key(OTHER_ITEM)?;
            load.serve(other, other, |item| Some(item.to_vec()))?;
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
    fn served_reads_keep_no_output_of_a_patch_the_api_built() {
        let loader = start();
        let host = enshwick::host().unwrap();
        let other = types::ITEM_INFO.key(OTHER_ITEM).unwrap();
        for _ in 0..10 {
            assert_eq!(host.served_resource(other).ok(), host.game_resource(other));
        }
        assert_eq!(loader.kept_outputs(), 0, "each output was a lent buffer");
    }

    #[test]
    fn a_new_item_whose_every_patch_fails_reads_as_missing() {
        let _loader = start();
        let host = enshwick::host().unwrap();
        let new = types::ITEM_INFO.key(NEW_ITEM).unwrap();
        assert!(host.served_resource(new).is_err());
    }
}
