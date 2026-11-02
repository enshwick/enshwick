//! Kick, ban and unban by account through the game's account actions.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::WorldError;
    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::world::{AccountAction, World};
    use enshwick::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Admin;

    impl Mod for Admin {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Admin,
        r#"
        id = "you.admin"
        side = "server"
        "#
    );

    fn player(entity: u32, key: u64, permissions: u8) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity: enshwick::Entity(entity),
            key: enshwick::PlayerKey::new(key),
            permissions,
            name_len: 0,
            name: [0; 64],
        }
    }

    #[test]
    fn accounts_are_banned_online_or_not_and_administrators_never() {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .player(player(10, 7, 0))
            .player(player(11, 8, PlayerInfo::CAN_KICK_BAN))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();

        assert_eq!(
            world.account_action(AccountAction::Ban, enshwick::PlayerKey::new(7)),
            Ok(())
        );
        assert_eq!(
            world.account_action(AccountAction::Ban, enshwick::PlayerKey::new(99)),
            Ok(())
        );
        assert_eq!(
            world.account_action(AccountAction::Ban, enshwick::PlayerKey::new(8)),
            Err(WorldError::Refused)
        );
        assert_eq!(
            world.account_action(AccountAction::Kick, enshwick::PlayerKey::new(99)),
            Err(WorldError::Refused)
        );
        assert_eq!(
            world.account_action(AccountAction::Ban, enshwick::PlayerKey::new(0)),
            Err(WorldError::Refused)
        );
        assert_eq!(
            loader.banned(),
            [enshwick::PlayerKey::new(7), enshwick::PlayerKey::new(99)]
        );
        assert_eq!(loader.removed(), [7]);

        assert_eq!(
            world.account_action(AccountAction::UnBan, enshwick::PlayerKey::new(99)),
            Ok(())
        );
        assert_eq!(loader.banned(), [enshwick::PlayerKey::new(7)]);
    }

    #[test]
    fn the_allowlist_lists_each_account_once_from_any_thread() {
        let loader = enshwick::test::FakeHost::new().world().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();

        assert_eq!(world.allowlisted(enshwick::PlayerKey::new(7)), Ok(false));
        assert_eq!(
            world.set_allowlisted(enshwick::PlayerKey::new(7), true),
            Ok(())
        );
        assert_eq!(
            world.set_allowlisted(enshwick::PlayerKey::new(7), true),
            Ok(())
        );
        let other =
            std::thread::spawn(move || world.set_allowlisted(enshwick::PlayerKey::new(9), true))
                .join()
                .unwrap();
        assert_eq!(other, Ok(()));
        assert_eq!(
            loader.allowlisted(),
            [enshwick::PlayerKey::new(7), enshwick::PlayerKey::new(9)]
        );
        assert_eq!(world.allowlisted(enshwick::PlayerKey::new(7)), Ok(true));

        assert_eq!(
            world.set_allowlisted(enshwick::PlayerKey::new(7), false),
            Ok(())
        );
        assert_eq!(
            world.set_allowlisted(enshwick::PlayerKey::new(7), false),
            Ok(())
        );
        assert_eq!(loader.allowlisted(), [enshwick::PlayerKey::new(9)]);
        assert_eq!(
            world.set_allowlisted(enshwick::PlayerKey::new(0), true),
            Err(WorldError::Refused)
        );
        assert_eq!(
            world.allowlisted(enshwick::PlayerKey::new(0)),
            Err(WorldError::Refused)
        );
    }

    #[test]
    fn the_allowlist_is_missing_outside_a_dedicated_server() {
        let loader = enshwick::test::FakeHost::new()
            .side(enshwick::abi::Side::CLIENT)
            .world()
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();

        assert_eq!(
            world.set_allowlisted(enshwick::PlayerKey::new(7), true),
            Err(WorldError::Missing)
        );
        assert_eq!(
            world.allowlisted(enshwick::PlayerKey::new(7)),
            Err(WorldError::Missing)
        );
        assert!(loader.allowlisted().is_empty());
    }

    #[test]
    fn players_are_muted_and_heard_again_but_never_administrators_or_the_host() {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .host_key(enshwick::PlayerKey::new(9))
            .player(player(10, 7, 0))
            .player(player(11, 8, PlayerInfo::CAN_KICK_BAN))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();
        let key = enshwick::PlayerKey::new;

        assert_eq!(world.muted(key(7)), Ok(false));
        assert_eq!(world.set_muted(key(7), true), Ok(()));
        assert_eq!(world.set_muted(key(7), true), Ok(()));
        let away = std::thread::spawn(move || world.set_muted(key(99), true))
            .join()
            .unwrap();
        assert_eq!(away, Ok(()), "an account not online is muted too");
        assert_eq!(loader.muted(), [key(7), key(99)]);
        assert_eq!(world.muted(key(7)), Ok(true));

        assert_eq!(world.set_muted(key(8), true), Err(WorldError::Refused));
        assert_eq!(world.set_muted(key(9), true), Err(WorldError::Refused));
        assert_eq!(world.set_muted(key(0), true), Err(WorldError::Refused));
        assert_eq!(world.muted(key(0)), Err(WorldError::Refused));

        assert_eq!(world.set_muted(key(7), false), Ok(()));
        assert_eq!(world.set_muted(key(7), false), Ok(()));
        assert_eq!(world.set_muted(key(8), false), Ok(()));
        assert_eq!(loader.muted(), [key(99)]);
    }

    #[test]
    fn chat_mutes_are_missing_on_a_client_that_hosts_no_game() {
        let loader = enshwick::test::FakeHost::new()
            .side(enshwick::abi::Side::CLIENT)
            .world()
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();
        let key = enshwick::PlayerKey::new;

        assert_eq!(world.set_muted(key(7), true), Err(WorldError::Missing));
        assert_eq!(world.muted(key(7)), Err(WorldError::Missing));
        assert!(loader.muted().is_empty());
    }

    #[test]
    fn chat_mutes_hold_on_a_game_a_client_hosts() {
        let loader = enshwick::test::FakeHost::new()
            .side(enshwick::abi::Side::CLIENT)
            .hosting(true)
            .host_key(enshwick::PlayerKey::new(9))
            .world()
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();
        let key = enshwick::PlayerKey::new;

        assert_eq!(world.set_muted(key(7), true), Ok(()));
        assert_eq!(world.muted(key(7)), Ok(true));
        assert_eq!(world.set_muted(key(9), true), Err(WorldError::Refused));
        assert_eq!(loader.muted(), [key(7)]);
    }

    #[test]
    fn the_host_is_never_kicked_or_banned_and_a_second_ban_is_no_error() {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .host_key(enshwick::PlayerKey::new(7))
            .player(player(10, 7, 0))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();

        assert_eq!(
            world.account_action(AccountAction::Kick, enshwick::PlayerKey::new(7)),
            Err(WorldError::Refused)
        );
        assert_eq!(
            world.account_action(AccountAction::Ban, enshwick::PlayerKey::new(7)),
            Err(WorldError::Refused)
        );
        assert_eq!(
            world.account_action(AccountAction::UnBan, enshwick::PlayerKey::new(7)),
            Ok(())
        );
        assert_eq!(
            world.account_action(AccountAction::Ban, enshwick::PlayerKey::new(99)),
            Ok(())
        );
        assert_eq!(
            world.account_action(AccountAction::Ban, enshwick::PlayerKey::new(99)),
            Ok(())
        );
        assert_eq!(loader.banned(), [enshwick::PlayerKey::new(99)]);
        assert!(loader.removed().is_empty());
    }

    #[test]
    fn account_actions_wait_for_the_game_thread() {
        let loader = enshwick::test::FakeHost::new().world().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();

        let answer = std::thread::spawn(move || {
            world.account_action(AccountAction::Ban, enshwick::PlayerKey::new(5))
        })
        .join()
        .unwrap();
        assert_eq!(answer, Err(WorldError::WrongThread));
        assert!(loader.banned().is_empty());
    }
}
