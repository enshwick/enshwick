//! A patch that asks to rebuild the key it is building is refused, as the
//! loader refuses it: the rebuild would wait on its own build.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering};

    use enshwick::abi::Status;
    use enshwick::content::{rebuild, types};
    use enshwick::test::{FakeHost, Installed, OTHER_ITEM};
    use enshwick::{Load, Mod, Off};

    /// What the patch's own rebuild answered: 0 not asked, 1 done, 2 refused.
    static ANSWER: AtomicU8 = AtomicU8::new(0);

    struct Rebuilder;

    impl Mod for Rebuilder {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let other = types::ITEM_INFO.key(OTHER_ITEM)?;
            load.serve(other, other, move |item| {
                let answer = if rebuild(other).is_ok() { 1 } else { 2 };
                ANSWER.store(answer, Ordering::Relaxed);
                Some(item.to_vec())
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Rebuilder,
        r#"
        id = "you.rebuilder"
        side = "both"
        "#
    );

    fn start() -> Installed {
        let loader = FakeHost::new().items().removal().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Rebuilder)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    #[test]
    fn a_patch_rebuilding_its_own_key_is_refused_and_asks_no_rebuild() {
        let loader = start();
        let host = enshwick::host().unwrap();
        let other = types::ITEM_INFO.key(OTHER_ITEM).unwrap();
        assert!(host.served_resource(other).is_ok());
        assert_eq!(
            ANSWER.load(Ordering::Relaxed),
            2,
            "refused inside its build"
        );
        assert!(loader.rebuilds().is_empty());
        assert!(rebuild(other).is_ok(), "outside its build it passes");
        assert_eq!(loader.rebuilds(), [other]);
    }
}
