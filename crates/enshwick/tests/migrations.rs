//! A mod's migrations through the fake loader: settings and stores changed
//! as the loader would, and an item renamed served under its old GUID too.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::items::NewItem;
    use enshwick::content::{Guid, Id, types};
    use enshwick::migrations::{Migrations, current_id};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const OLD_SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const SAFE_ID: Id = Id::new("you.safes:safe");
    const SAFE: Guid = SAFE_ID.guid();
    const ITEM: u32 = 1;

    struct Safes;

    impl Mod for Safes {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.safes", side: SERVER)
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
            load.add_item(NewItem::new(SAFE_ID, CHEST))?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Safes);

    fn host() -> FakeHost {
        let mut host = FakeHost::new()
            .type_hash(types::ITEM_INFO.id(), ITEM)
            .type_hash(types::ITEM_REGISTRY.id(), 2)
            .type_size("item_stat", 0x28);
        for (id, at) in [
            ("item_info.id", 0),
            ("item_info.object", 4),
            ("item_info.name", 0x48),
            ("item_info.icon_image", 0x58),
            ("item_info.description", 0x68),
            ("item_info.flags", 0x78),
            ("item_info.pid_entity", 0x80),
            ("item_info.ui_values", 0x90),
        ] {
            host = host.constant(id, at);
        }
        for (id, at) in [
            ("item_stat.label", 0),
            ("item_stat.format", 4),
            ("item_stat.value", 8),
            ("item_stat.signed", 0x24),
        ] {
            host = host.constant(id, at);
        }
        host.constant("item_registry.refs", 0)
            .constant("item_registry.names", 0x38)
    }

    fn start() -> enshwick::test::Installed {
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
        assert!(
            refused.to_string().contains("limit is a number"),
            "{refused}"
        );
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
        assert!(served.contains(&SAFE.key_by_hash(ITEM)));
        assert!(served.contains(&OLD_SAFE.key_by_hash(ITEM)));
    }

    #[test]
    fn the_item_s_id_is_declared_and_old_stacks_read_as_it() {
        let loader = start();
        assert_eq!(loader.content_ids(), ["you.safes:safe"]);
        assert_eq!(current_id(OLD_SAFE.id()), SAFE.id());
        assert_eq!(current_id(CHEST.id()), CHEST.id());
    }
}
