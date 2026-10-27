//! A mod's migrations through the fake loader: settings and stores changed
//! as the loader would, and an item renamed served under its old GUID too.

#[cfg(test)]
mod tests {
    use core::ffi::c_void;

    use enshwick::abi::{Header, MigrateFn, MigrationSet, Status, Str};
    use enshwick::content::items::NewItem;
    use enshwick::content::{Guid, Id, types};
    use enshwick::migrations::{Migrations, current_id};
    use enshwick::test::FakeHost;
    use enshwick::{ItemId, Load, Mod, Off};

    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const OLD_LOCKER: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const LOCKER_ID: Id = Id::new("you.lockers:locker");
    const LOCKER: Guid = LOCKER_ID.guid();
    const ITEM: u32 = 1;

    struct Lockers;

    impl Mod for Lockers {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.lockers", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.migrate(
                Migrations::new()
                    .add("2", |m| {
                        m.rename_setting("cost", "cost_multiplier")
                            .remove_setting("legacy")
                            .rename_content(OLD_LOCKER, LOCKER)
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
            load.add_item(NewItem::new(LOCKER_ID, CHEST))?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Lockers);

    fn host() -> FakeHost {
        let mut host = FakeHost::new()
            .type_hash(types::ITEM_INFO.id(), ITEM)
            .type_hash(types::ITEM_REGISTRY_RESOURCE.id(), 2)
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
        // SAFETY: both functions are the exports of this file's `declare_mod!(Safes)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    unsafe extern "system" fn unchanged(
        _user: *mut c_void,
        _migration: u32,
        _step: u32,
        _input: Str,
        _out: *mut Str,
    ) -> Status {
        Status::OK
    }

    /// Hands the host table an empty set with `map`, as a mod would.
    fn migrate_raw(map: Option<MigrateFn>) -> Status {
        let host = enshwick::host().expect("the mod's host");
        let set = MigrationSet {
            header: Header::of::<MigrationSet>(),
            migrations: core::ptr::null(),
            count: 0,
            map,
            user: core::ptr::null_mut(),
        };
        (host.api().migrate)(host.handle(), &raw const set)
    }

    #[test]
    fn a_migration_set_without_a_map_is_refused_as_bad_arguments() {
        let _loader = start();
        assert_eq!(migrate_raw(None), Status::BAD_ARGS);
        assert_eq!(
            migrate_raw(Some(unchanged)),
            Status::REFUSED,
            "with a map the set gets past the check, to the one set a mod may give"
        );
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
        assert!(served.contains(&LOCKER.key_by_hash(ITEM)));
        assert!(served.contains(&OLD_LOCKER.key_by_hash(ITEM)));
    }

    #[test]
    fn the_item_s_id_is_declared_and_old_stacks_read_as_it() {
        let loader = start();
        assert_eq!(loader.content_ids(), ["you.lockers:locker"]);
        assert_eq!(
            current_id(ItemId::from_guid(OLD_LOCKER)),
            ItemId::from_guid(LOCKER)
        );
        assert_eq!(
            current_id(ItemId::from_guid(CHEST)),
            ItemId::from_guid(CHEST)
        );
    }
}
