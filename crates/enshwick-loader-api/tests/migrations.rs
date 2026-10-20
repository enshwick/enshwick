//! A mod's migrations through the fake loader: settings and stores changed
//! as the loader would, and an item renamed served under its old GUID too.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::content::items::NewItem;
    use enshwick_loader_api::content::{Guid, types};
    use enshwick_loader_api::migrations::Migrations;
    use enshwick_loader_api::test::FakeHost;
    use enshwick_loader_api::{Load, Mod, Off};

    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const OLD_SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const SAFE: Guid = Guid::parse("9b4f7e32-6d5c-4e8f-a021-3c8d6f7b0e42");

    struct Safes;

    impl Mod for Safes {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "safes", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.migrate(
                Migrations::new()
                    .add("2", |m| {
                        m.rename_setting("cost", "cost_multiplier")
                            .remove_setting("legacy")
                            .rename_content(OLD_SAFE, SAFE)
                    })
                    .add("3", |m| {
                        m.map_setting("limit", |old| {
                            let limit = old.as_integer().ok_or("limit is a number")?;
                            Ok(toml::Value::Integer(limit * 2))
                        })
                        .rename_store("owners", "keepers")
                        .map_store("keepers", |table| {
                            table.insert("version".to_owned(), toml::Value::Integer(3));
                            Ok(())
                        })
                    }),
            )?;
            load.add_item(NewItem::new(SAFE, CHEST, "Safe"))?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Safes);

    fn host() -> FakeHost {
        let mut host = FakeHost::new().type_size("keen::ImpactUiValue", 0x28);
        for (field, at) in [
            ("itemId", 0),
            ("objectId", 4),
            ("name", 0x48),
            ("iconImage", 0x58),
            ("description", 0x68),
            ("flags", 0x78),
            ("pidEntity", 0x80),
            ("uiValues", 0x90),
        ] {
            host = host.field_offset("keen::ItemInfo", field, at);
        }
        for (field, at) in [
            ("locaId", 0),
            ("valueFormat", 4),
            ("value", 8),
            ("isSigned", 0x24),
        ] {
            host = host.field_offset("keen::ImpactUiValue", field, at);
        }
        host.field_offset("keen::ItemRegistryResource", "itemRefs", 0)
            .field_offset("keen::ItemRegistryResource", "dbgNames", 0x38)
    }

    fn start() -> enshwick_loader_api::test::Installed {
        let loader = host().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    #[test]
    fn the_loader_gets_every_migration_in_order() {
        let loader = start();
        assert_eq!(loader.migrations(), ["2", "3"]);
    }

    #[test]
    fn an_old_settings_file_is_migrated() {
        let loader = start();
        let new = loader
            .migrate_settings("cost = 3\nlimit = 4\nlegacy = true\n")
            .unwrap();
        assert_eq!(new, "cost_multiplier = 3\nlimit = 8\n");
        let refused = loader.migrate_settings("limit = \"x\"\n").unwrap_err();
        assert!(refused.contains("limit is a number"), "{refused}");
    }

    #[test]
    fn an_old_store_is_renamed_and_changed() {
        let loader = start();
        let (name, text) = loader.migrate_store("owners", "count = 1\n").unwrap();
        assert_eq!(name, "keepers");
        assert_eq!(text, "count = 1\nversion = 3\n");
    }

    #[test]
    fn a_renamed_item_is_served_under_its_old_guid_too() {
        let loader = start();
        let served: Vec<_> = loader.resources().iter().map(|r| r.key).collect();
        assert!(served.contains(&SAFE.key(types::ITEM_INFO)));
        assert!(served.contains(&OLD_SAFE.key(types::ITEM_INFO)));
    }
}
