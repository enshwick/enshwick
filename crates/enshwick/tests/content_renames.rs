//! A mod's content renames through the fake loader: the loader gets them,
//! and an item renamed is served under its old GUID too.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ContentRename, Status};
    use enshwick::content::items::NewItem;
    use enshwick::content::renames::{Renames, current_id};
    use enshwick::content::{EngineFacts, Guid, Id, types};
    use enshwick::test::FakeHost;
    use enshwick::{ItemId, Load, Mod, Off};

    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const OLDEST_LOCKER: Guid = Guid::parse("1c0d9e5a-7b2f-4e3a-8d61-5f4a2b9c7e10");
    const OLD_LOCKER: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const LOCKER_ID: Id = Id::new("you.lockers:locker");
    const LOCKER: Guid = LOCKER_ID.guid();

    /// What the mod's renames after its first answered: one onto itself and
    /// a second declaration.
    static AGAIN: Mutex<Vec<Status>> = Mutex::new(Vec::new());

    struct Lockers;

    impl Mod for Lockers {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.declare(LOCKER_ID)?;
            load.rename_content(
                Renames::new()
                    .rename(OLDEST_LOCKER, OLD_LOCKER)
                    .rename(OLD_LOCKER, LOCKER),
            )?;
            let onto_itself = ContentRename {
                from: CHEST,
                to: CHEST,
            };
            *AGAIN.lock().unwrap() = vec![rename_raw(&[onto_itself]), rename_raw(&[])];
            load.add_item(NewItem::new(LOCKER_ID, CHEST))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lockers,
        r#"
        id = "you.lockers"
        side = "server"
        "#
    );

    fn host() -> FakeHost {
        FakeHost::new().items()
    }

    /// The hash items are served under.
    fn item() -> u32 {
        host().hash(types::ITEM_INFO.id()).unwrap()
    }

    fn start() -> enshwick::test::Installed {
        let loader = host().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Lockers)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// Hands the host table `renames` as a mod would.
    fn rename_raw(renames: &[ContentRename]) -> Status {
        let host = enshwick::host().expect("the mod's host");
        let count = u32::try_from(renames.len()).unwrap();
        (host.api().rename_content)(host.handle(), renames.as_ptr(), count)
    }

    #[test]
    fn the_loader_gets_every_rename_in_order() {
        let loader = start();
        assert_eq!(
            loader.content_renames(),
            [(OLDEST_LOCKER, OLD_LOCKER), (OLD_LOCKER, LOCKER)]
        );
    }

    #[test]
    fn a_second_declaration_or_a_rename_to_itself_is_refused_and_one_after_init_is_too_late() {
        let _loader = start();
        assert_eq!(*AGAIN.lock().unwrap(), [Status::REFUSED, Status::REFUSED]);
        assert_eq!(rename_raw(&[]), Status::TOO_LATE, "after init");
    }

    #[test]
    fn a_renamed_item_is_served_under_every_old_guid_too() {
        let loader = start();
        let served: Vec<_> = loader.resources().iter().map(|r| r.key).collect();
        for guid in [LOCKER, OLD_LOCKER, OLDEST_LOCKER] {
            assert!(served.contains(&guid.key_by_hash(item())), "{guid}");
        }
    }

    /// A mod whose renames the loader refuses, which adds its item anyway.
    mod refused {
        use std::sync::Mutex;

        use enshwick::abi::Status;
        use enshwick::content::items::NewItem;
        use enshwick::content::renames::{Renames, current_id};
        use enshwick::content::{Guid, Id};
        use enshwick::test::FakeHost;
        use enshwick::{ItemId, Load, Mod, Off};

        use super::{CHEST, OLD_LOCKER};

        const DRAWER_ID: Id = Id::new("you.drawers:drawer");
        /// A GUID no mod declared, which no rename may end on.
        const LOOSE: Guid = Guid([5; 16]);

        /// Whether the refused renames answered an error.
        static REFUSED: Mutex<Option<bool>> = Mutex::new(None);

        struct Drawers;

        impl Mod for Drawers {
            fn init(load: &mut Load<'_>) -> Result<Self, Off> {
                load.declare(DRAWER_ID)?;
                let renames = Renames::new()
                    .rename(OLD_LOCKER, DRAWER_ID.guid())
                    .rename(CHEST, LOOSE);
                *REFUSED.lock().unwrap() = Some(load.rename_content(renames).is_err());
                load.add_item(NewItem::new(DRAWER_ID, CHEST))?;
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Drawers,
            r#"
            id = "you.drawers"
            side = "server"
            "#
        );

        #[test]
        fn refused_renames_move_no_id_and_the_item_is_still_added() {
            let loader = FakeHost::new().items().install();
            // SAFETY: the exports of this module's `declare_mod!(Drawers)`.
            let status =
                unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
            assert_eq!(status, Status::OK, "{:?}", loader.logs());
            assert_eq!(*REFUSED.lock().unwrap(), Some(true));
            let old = ItemId::from_guid(OLD_LOCKER);
            assert_eq!(current_id(old), old);
        }
    }

    #[test]
    fn old_stacks_read_as_the_item_s_current_id() {
        let loader = start();
        assert_eq!(loader.content_ids(), ["you.lockers:locker"]);
        let locker = ItemId::from_guid(LOCKER);
        assert_eq!(current_id(ItemId::from_guid(OLDEST_LOCKER)), locker);
        assert_eq!(current_id(ItemId::from_guid(OLD_LOCKER)), locker);
        assert_eq!(
            current_id(ItemId::from_guid(CHEST)),
            ItemId::from_guid(CHEST)
        );
    }
}
