//! Notices: on screen for clients with Enshwick, chat for the rest.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::abi::{PlayerInfo, Status};
    use enshwick::net::Peer;
    use enshwick::world::{Notice, World};
    use enshwick::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Herald;

    impl Mod for Herald {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Herald,
        r#"
        id = "you.herald"
        side = "server"
        "#
    );

    fn player(entity: u32, key: u64) -> PlayerInfo {
        enshwick::abi::headed!(PlayerInfo {
            slot: entity,
            entity: Entity::from_engine(entity),
            key: enshwick::PlayerKey::new(key),
            permissions: 0,
            name_len: 0,
            name: [0; PlayerInfo::NAME_MAX],
        })
    }

    #[test]
    fn a_notice_shows_on_screen_where_it_can_and_goes_to_chat_elsewhere() {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .player(player(2, 20))
            .player(player(3, 30))
            .net_peer(Peer::Player(enshwick::PlayerKey::new(20)))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Herald)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().unwrap();
        let modded = world.player(Entity::from_engine(2)).unwrap();
        let vanilla = world.player(Entity::from_engine(3)).unwrap();
        assert_eq!(world.notify(&modded, "Low fuel"), Notice::OnScreen);
        assert_eq!(world.notify(&vanilla, "Low fuel"), Notice::Chat);
        assert_eq!(
            loader.notices(),
            [(enshwick::PlayerKey::new(20), "Low fuel".to_owned())]
        );
        assert_eq!(
            loader.chat(),
            [(enshwick::PlayerKey::new(30), "Low fuel".to_owned())]
        );
    }

    fn started(players: &[PlayerInfo]) -> (enshwick::test::Installed, &'static World) {
        let loader = players
            .iter()
            .fold(enshwick::test::FakeHost::new().world(), |host, info| {
                host.player(*info)
            })
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Herald)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        (loader, WORLD.get().unwrap())
    }

    #[test]
    fn a_connected_player_without_a_key_or_a_character_is_not_found_by_the_other_half() {
        let (loader, world) = started(&[player(4, 0), player(0, 50)]);
        assert_eq!(
            world.notify(Entity::from_engine(4), "Low fuel"),
            Notice::Nobody
        );
        assert_eq!(
            world.can_build(enshwick::PlayerKey::new(50), [0.0; 3], [1.0; 3]),
            Err(enshwick::world::BuildRefusal::Unchecked(
                enshwick::WorldError::Gone
            ))
        );
        assert!(loader.chat().is_empty() && loader.notices().is_empty());
    }

    #[test]
    fn a_line_for_a_character_off_the_game_thread_says_why_it_goes_to_no_player() {
        let (loader, world) = started(&[player(2, 20)]);
        std::thread::spawn(|| {
            world.tell(Entity::from_engine(2), "Lost");
            world.tell(enshwick::PlayerKey::new(20), "Found");
        })
        .join()
        .unwrap();
        assert_eq!(
            loader.chat(),
            [(enshwick::PlayerKey::new(20), "Found".to_owned())]
        );
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("only on the game thread")),
            "{:?}",
            loader.logs()
        );
    }
}
