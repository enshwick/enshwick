//! Notices: on screen for clients with Enshwick, chat for the rest.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::abi::{Header, PlayerInfo, Status};
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
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity: Entity(entity),
            key: enshwick::PlayerKey::new(key),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }
    }

    #[test]
    fn a_notice_shows_on_screen_where_it_can_and_goes_to_chat_elsewhere() {
        let loader = enshwick::test::FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .player(player(2, 20))
            .player(player(3, 30))
            .net_peer(Peer::Player(enshwick::PlayerKey::new(20)))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Herald)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().unwrap();
        let modded = world.player(Entity(2)).unwrap();
        let vanilla = world.player(Entity(3)).unwrap();
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
}
