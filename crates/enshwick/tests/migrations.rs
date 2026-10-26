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
    use enshwick::{Load, Mod, Off};

    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const OLD_LOCKER: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const LOCKER_ID: Id = Id::new("you.lockers:locker");
    const LOCKER: Guid = LOCKER_ID.guid();

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
        assert!(served.contains(&LOCKER.key(types::ITEM_INFO)));
        assert!(served.contains(&OLD_LOCKER.key(types::ITEM_INFO)));
    }

    #[test]
    fn the_item_s_id_is_declared_and_old_stacks_read_as_it() {
        let loader = start();
        assert_eq!(loader.content_ids(), ["you.lockers:locker"]);
        assert_eq!(current_id(OLD_LOCKER.id()), LOCKER.id());
        assert_eq!(current_id(CHEST.id()), CHEST.id());
    }
}
