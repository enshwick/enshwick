//! Notices: on screen for clients with Enshwick, chat for the rest.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick_loader_api::abi::{Header, PlayerInfo, Status};
    use enshwick_loader_api::ecs::Entity;
    use enshwick_loader_api::net::Peer;
    use enshwick_loader_api::world::{Notice, World};
    use enshwick_loader_api::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Herald;

    impl Mod for Herald {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "herald", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Herald);

    fn player(entity: u32, key: u64) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity,
            key,
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }
    }

    #[test]
    fn a_notice_shows_on_screen_where_it_can_and_goes_to_chat_elsewhere() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
            .player(player(2, 20))
            .player(player(3, 30))
            .net_peer(Peer::Player(20))
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().unwrap();
        let modded = world.player(Entity(2)).unwrap();
        let vanilla = world.player(Entity(3)).unwrap();
        assert_eq!(world.notify(&modded, "Low fuel"), Notice::OnScreen);
        assert_eq!(world.notify(&vanilla, "Low fuel"), Notice::Chat);
        assert_eq!(loader.notices(), [(20, "Low fuel".to_owned())]);
        assert_eq!(loader.chat(), [(30, "Low fuel".to_owned())]);
    }
}
