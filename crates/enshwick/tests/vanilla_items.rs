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
    static LISTED: Mutex<Vec<vanilla::VanillaItem>> = Mutex::new(Vec::new());

    const CHEST_NAME: Guid = Guid([1; 16]);
    const BOX_NAME: Guid = Guid([2; 16]);
    const CRATE: Guid = Guid([8; 16]);

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
            for name in ["Wooden Chest", "скриня", "Box", "Wooden Chst", "Truhe"] {
                found.push(vanilla::item_by_display_name(name));
            }
            *LISTED.lock().unwrap() = vanilla::items().unwrap_or_default();
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
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
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

    #[test]
    fn a_mod_finds_a_vanilla_item_by_the_name_the_game_shows_in_any_language() {
        use enshwick::i18n::Language;

        let loader = FakeHost::new()
            .items()
            .workshops()
            .vanilla_items(&[
                (CHEST, "Prop_Storage_32_T2_Chest_Wood"),
                (BOX, "Prop_Storage_24_T1_Box"),
                (CRATE, "Prop_Storage_24_T1_Crate"),
            ])
            .vanilla_item_names(&[(CHEST, CHEST_NAME), (BOX, BOX_NAME), (CRATE, BOX_NAME)])
            .game_text(CHEST_NAME.fnv1a(), Language::EN_US, "Wooden Chest")
            .game_text(CHEST_NAME.fnv1a(), Language::UK_UA, "Скриня")
            .game_text(BOX_NAME.fnv1a(), Language::EN_US, "Box")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Looking)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let found = FOUND.lock().unwrap();
        assert_eq!(found[3], Ok(CHEST), "the English name");
        assert_eq!(found[4], Ok(CHEST), "another language, whatever the case");
        assert!(
            matches!(&found[5], Err(VanillaError::Ambiguous { items, .. }) if items == &[BOX, CRATE]),
            "two items the game shows by one name: {:?}",
            found[5]
        );
        assert!(
            matches!(&found[6], Err(VanillaError::Unknown { near, .. }) if near == &["Wooden Chest"]),
            "{:?}",
            found[6]
        );
        assert!(
            matches!(&found[7], Err(VanillaError::Unknown { near, .. }) if near.is_empty()),
            "{:?}",
            found[7]
        );

        let listed = LISTED.lock().unwrap();
        let names: Vec<(&str, Option<Guid>)> = listed
            .iter()
            .map(|item| (item.name.as_str(), item.name_tag))
            .collect();
        assert_eq!(
            names,
            [
                ("Prop_Storage_24_T1_Box", Some(BOX_NAME)),
                ("Prop_Storage_24_T1_Crate", Some(BOX_NAME)),
                ("Prop_Storage_32_T2_Chest_Wood", Some(CHEST_NAME)),
            ],
            "every item by debug name, with the tag of its shown name"
        );
        assert_eq!(listed[2].guid, CHEST);
    }

    #[test]
    fn display_names_do_not_read_where_the_game_has_no_texts() {
        let loader = FakeHost::new()
            .items()
            .workshops()
            .vanilla_items(&[(CHEST, "Prop_Storage_32_T2_Chest_Wood")])
            .vanilla_item_names(&[(CHEST, CHEST_NAME)])
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Looking)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            FOUND.lock().unwrap()[3],
            Err(VanillaError::Unreadable("texts")),
            "as on a dedicated server, whose archive holds none"
        );
    }
}
