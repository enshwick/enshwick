//! A re-keyed item follows its new item when that one is rebuilt: the
//! rebuild builds again the keys whose builds read it, as the loader's does.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering};

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::types;
    use enshwick::test::{FakeHost, Installed, OTHER_ITEM, SPARE_ITEM};
    use enshwick::{Load, Mod, Off};

    /// Where the fake's items hold their category byte.
    const CATEGORY_AT: usize = 22;

    /// The category the mod's patch gives `OTHER_ITEM`.
    static CATEGORY: AtomicU8 = AtomicU8::new(0x10);

    struct Follow;

    impl Mod for Follow {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let new = types::ITEM_INFO.key(OTHER_ITEM)?;
            load.serve(new, new, |item, out| {
                out.copy_from(item)?;
                *out.get_mut(CATEGORY_AT)? = CATEGORY.load(Ordering::Relaxed);
                Some(())
            })?;
            load.allow_rebuild(new)?;
            load.rekey_item(SPARE_ITEM, OTHER_ITEM)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Follow,
        r#"
        id = "you.follow"
        side = "both"
        "#
    );

    fn start() -> Installed {
        CATEGORY.store(0x10, Ordering::Relaxed);
        let loader = FakeHost::new().items().removal().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Follow)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// The category byte of the bytes the game holds for `key`.
    fn held_category(loader: &Installed, key: &ResourceKey) -> Option<u8> {
        loader.held_resource(key)?.get(CATEGORY_AT).copied()
    }

    #[test]
    fn a_rebuild_of_the_new_item_builds_the_item_rekeyed_to_it_again() {
        let loader = start();
        let host = enshwick::host().unwrap();
        let (old, new) = (
            types::ITEM_INFO.key(SPARE_ITEM).unwrap(),
            types::ITEM_INFO.key(OTHER_ITEM).unwrap(),
        );
        let built = loader
            .build_resource(&old, &host.game_resource(old).unwrap())
            .unwrap();
        assert_eq!(built.get(CATEGORY_AT), Some(&0x10), "the new item's data");
        assert_eq!(held_category(&loader, &old), Some(0x10));

        CATEGORY.store(0x20, Ordering::Relaxed);
        assert_eq!(
            (host.api().rebuild_resource)(host.handle(), &raw const new),
            Status::OK
        );
        assert_eq!(
            held_category(&loader, &old),
            Some(0x20),
            "the old item follows its new item's rebuild"
        );
    }
}
