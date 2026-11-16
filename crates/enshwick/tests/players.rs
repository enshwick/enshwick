//! Player lookups by account key and by name.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::world::PlayerPermissions::CanEditBase;
    use enshwick::world::World;
    use enshwick::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Lookups;

    impl Mod for Lookups {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lookups,
        r#"
        id = "you.lookups"
        side = "server"
        "#
    );

    fn player(entity: u32, key: u64, name: &str) -> PlayerInfo {
        let mut bytes = [0u8; PlayerInfo::NAME_MAX];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity: enshwick::Entity(entity),
            key: enshwick::PlayerKey::new(key),
            permissions: 0,
            name_len: u32::try_from(name.len()).unwrap(),
            name: bytes,
        }
    }

    #[test]
    fn players_are_found_by_key_and_by_name_ignoring_case() {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .player(player(10, 7, "Ann"))
            .player(player(11, 8, "ann"))
            .player(player(12, 9, "Bob"))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Lookups)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().unwrap();

        assert_eq!(
            world
                .player_by_key(enshwick::PlayerKey::new(9))
                .map(|p| p.name)
                .as_deref(),
            Some("Bob")
        );
        assert!(world.player_by_key(enshwick::PlayerKey::new(1)).is_none());
        let keys: Vec<enshwick::PlayerKey> =
            world.players_named("ANN").iter().map(|p| p.key).collect();
        assert_eq!(
            keys,
            [enshwick::PlayerKey::new(7), enshwick::PlayerKey::new(8)]
        );
        assert!(world.players_named("Bo").is_empty());
    }

    #[test]
    fn a_long_name_of_multibyte_characters_reads_whole_and_a_longer_one_cuts_between_characters() {
        let whole = "Ж".repeat(40);
        let long = "🦀".repeat(33);
        let loader = enshwick::test::FakeHost::new()
            .world()
            .player(PlayerInfo {
                slot: 20,
                ..PlayerInfo::with_name(
                    enshwick::Entity(20),
                    enshwick::PlayerKey::new(1),
                    0,
                    &whole,
                )
            })
            .player(PlayerInfo {
                slot: 21,
                ..PlayerInfo::with_name(enshwick::Entity(21), enshwick::PlayerKey::new(2), 0, &long)
            })
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Lookups)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().unwrap();
        let name = |key| {
            world
                .player_by_key(enshwick::PlayerKey::new(key))
                .map(|p| p.name)
        };

        assert_eq!(whole.len(), 80, "over the old 64-byte cut");
        assert_eq!(name(1).as_deref(), Some(whole.as_str()));
        assert_eq!(name(2).as_deref(), Some("🦀".repeat(32).as_str()));
    }

    fn started(host: enshwick::test::FakeHost) -> (enshwick::test::Installed, &'static World) {
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Lookups)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        (loader, WORLD.get().unwrap())
    }

    #[test]
    fn a_mod_takes_and_gives_a_player_s_permissions_until_an_empty_change_ends_it() {
        use enshwick::world::PlayerPermissions::{CanAccessInventories, CanEditWorld};

        let group =
            PlayerInfo::CAN_EDIT_WORLD | PlayerInfo::CAN_EDIT_BASE | PlayerInfo::CAN_RECEIVE_EXP;
        let ann = PlayerInfo {
            permissions: group,
            ..player(10, 7, "Ann")
        };
        let (_loader, world) = started(
            enshwick::test::FakeHost::new()
                .world()
                .world_key("world0")
                .player(ann),
        );
        let key = enshwick::PlayerKey::new(7);
        let held = || world.player_by_key(key).unwrap().permissions;

        world
            .change_permissions(key, &[CanEditWorld], &[CanAccessInventories])
            .unwrap();
        assert_eq!(
            held(),
            PlayerInfo::CAN_EDIT_BASE
                | PlayerInfo::CAN_RECEIVE_EXP
                | PlayerInfo::CAN_ACCESS_INVENTORIES
        );
        world
            .change_permissions(key, &[CanEditWorld], &[CanEditWorld])
            .unwrap();
        assert_eq!(
            held() & PlayerInfo::CAN_EDIT_WORLD,
            0,
            "a taken bit stays gone"
        );
        world.change_permissions(key, &[], &[]).unwrap();
        assert_eq!(held(), group);
    }

    #[test]
    fn a_change_to_an_account_not_in_game_is_refused() {
        let (_loader, world) = started(
            enshwick::test::FakeHost::new()
                .world()
                .world_key("world0")
                .player(player(10, 7, "Ann")),
        );
        let taken = [CanEditBase];
        assert_eq!(
            world.change_permissions(enshwick::PlayerKey::new(8), &taken, &[]),
            Err(enshwick::WorldError::Refused)
        );
        assert_eq!(
            world.change_permissions(enshwick::PlayerKey::new(0), &taken, &[]),
            Err(enshwick::WorldError::Refused)
        );
    }

    #[test]
    fn a_change_off_the_game_thread_is_refused() {
        let (_loader, world) = started(
            enshwick::test::FakeHost::new()
                .world()
                .world_key("world0")
                .player(player(10, 7, "Ann")),
        );
        let off_thread = std::thread::spawn(move || {
            world.change_permissions(enshwick::PlayerKey::new(7), &[CanEditBase], &[])
        })
        .join()
        .unwrap();
        assert_eq!(off_thread, Err(enshwick::WorldError::WrongThread));
    }

    #[test]
    fn a_change_without_a_world_is_unavailable() {
        let (_loader, world) = started(
            enshwick::test::FakeHost::new()
                .world()
                .player(player(10, 7, "Ann")),
        );
        assert_eq!(
            world.change_permissions(enshwick::PlayerKey::new(7), &[CanEditBase], &[]),
            Err(enshwick::WorldError::Unavailable)
        );
    }

    #[test]
    fn ending_a_change_needs_no_player_in_game_nor_the_game_thread() {
        let (_loader, world) = started(
            enshwick::test::FakeHost::new()
                .world()
                .world_key("world0")
                .player(player(10, 7, "Ann")),
        );
        let gone = enshwick::PlayerKey::new(8);
        assert_eq!(world.change_permissions(gone, &[], &[]), Ok(()));
        let off_thread = std::thread::spawn(move || world.change_permissions(gone, &[], &[]))
            .join()
            .unwrap();
        assert_eq!(off_thread, Ok(()));
    }

    #[test]
    fn a_player_who_leaves_takes_every_change_to_their_permissions_with_them() {
        let (loader, world) = started(
            enshwick::test::FakeHost::new()
                .world()
                .world_key("world0")
                .player(player(10, 7, "Ann")),
        );
        let ann = enshwick::PlayerKey::new(7);
        world.change_permissions(ann, &[CanEditBase], &[]).unwrap();
        assert_eq!(
            loader.permission_change(ann),
            (PlayerInfo::CAN_EDIT_BASE, 0)
        );
        loader.player_left(ann);
        assert_eq!(loader.permission_change(ann), (0, 0));
    }
}
