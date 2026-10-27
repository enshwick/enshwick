//! Kick, ban and unban by account through the game's account actions.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::world::{AccountAction, World, WorldError};
    use enshwick::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Admin;

    impl Mod for Admin {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.admin", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Admin);

    fn player(entity: u32, key: u64, permissions: u8) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity,
            key,
            permissions,
            name_len: 0,
            name: [0; 64],
        }
    }

    #[test]
    fn accounts_are_banned_online_or_not_and_administrators_never() {
        let loader = enshwick::test::FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .player(player(10, 7, 0))
            .player(player(11, 8, PlayerInfo::CAN_KICK_BAN))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();

        assert_eq!(world.account_action(AccountAction::Ban, 7), Ok(()));
        assert_eq!(world.account_action(AccountAction::Ban, 99), Ok(()));
        assert_eq!(
            world.account_action(AccountAction::Ban, 8),
            Err(WorldError::Refused)
        );
        assert_eq!(
            world.account_action(AccountAction::Kick, 99),
            Err(WorldError::Refused)
        );
        assert_eq!(
            world.account_action(AccountAction::Ban, 0),
            Err(WorldError::Refused)
        );
        assert_eq!(loader.banned(), [7, 99]);
        assert_eq!(loader.removed(), [7]);

        assert_eq!(world.account_action(AccountAction::UnBan, 99), Ok(()));
        assert_eq!(loader.banned(), [7]);
    }

    #[test]
    fn account_actions_wait_for_the_game_thread() {
        let loader = enshwick::test::FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Admin)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();

        let answer = std::thread::spawn(move || world.account_action(AccountAction::Ban, 5))
            .join()
            .unwrap();
        assert_eq!(answer, Err(WorldError::WrongThread));
        assert!(loader.banned().is_empty());
    }
}
