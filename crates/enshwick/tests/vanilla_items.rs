//! Vanilla items by debug name, from the item registry the fake serves.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::content::vanilla::{self, VanillaError};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const BOX: Guid = Guid([7; 16]);

    static FOUND: Mutex<Vec<Result<Guid, VanillaError>>> = Mutex::new(Vec::new());

    struct Looking;

    impl Mod for Looking {
        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            let mut found = FOUND.lock().unwrap();
            found.clear();
            for name in [
                "Prop_Storage_32_T2_Chest_Wood",
                "Prop_Storage_24_T1_Box",
                "Chest_Wood",
            ] {
                found.push(vanilla::item(name));
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Looking,
        r#"
        id = "you.looking"
        side = "server"
        "#
    );

    #[test]
    fn a_mod_finds_the_vanilla_items_the_fake_lists_by_their_debug_names() {
        let loader = FakeHost::new()
            .items()
            .workshops()
            .vanilla_items(&[
                (CHEST, "Prop_Storage_32_T2_Chest_Wood"),
                (BOX, "Prop_Storage_24_T1_Box"),
            ])
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Looking)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let found = FOUND.lock().unwrap();
        assert_eq!(found[0], Ok(CHEST));
        assert_eq!(found[1], Ok(BOX));
        assert!(
            matches!(&found[2], Err(VanillaError::Unknown { near, .. }) if near == &["Prop_Storage_32_T2_Chest_Wood"]),
            "{:?}",
            found[2]
        );
    }
}
