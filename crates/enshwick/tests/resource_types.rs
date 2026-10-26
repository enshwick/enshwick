//! Resource types by enshwick id: the loader answers each type's hash on
//! the running build, and a key carries it.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::{Guid, types};
    use enshwick::{Load, Mod, Off};

    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");

    static KEYS: OnceLock<(ResourceKey, Result<ResourceKey, String>)> = OnceLock::new();

    struct Keys;

    impl Mod for Keys {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.keys", side: BOTH)
        }

        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            let item = CHEST.key(types::ITEM_INFO)?;
            let image = CHEST
                .key(types::UI_TEXTURE_RESOURCE)
                .map_err(|off| off.to_string());
            let _first = KEYS.set((item, image));
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Keys);

    #[test]
    fn a_key_carries_the_hash_the_loader_gives_its_type_id() {
        let loader = enshwick::test::FakeHost::new()
            .type_hash(types::ITEM_INFO.id(), 0x1234)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let (item, image) = KEYS.get().unwrap();
        assert_eq!(*item, CHEST.key_by_hash(0x1234));
        let missing = image.as_ref().unwrap_err();
        assert!(
            missing.contains(types::UI_TEXTURE_RESOURCE.id()),
            "{missing}"
        );
    }
}
