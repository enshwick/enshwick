//! Player lookups by account key and by name.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::world::World;
    use enshwick::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Lookups;

    impl Mod for Lookups {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "lookups", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Lookups);

    fn player(entity: u32, key: u64, name: &str) -> PlayerInfo {
        let mut bytes = [0u8; 64];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity,
            key,
            permissions: 0,
            name_len: u8::try_from(name.len()).unwrap(),
            name: bytes,
        }
    }

    #[test]
    fn players_are_found_by_key_and_by_name_ignoring_case() {
        let loader = enshwick::test::FakeHost::new()
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
            .player(player(10, 7, "Ann"))
            .player(player(11, 8, "ann"))
            .player(player(12, 9, "Bob"))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Lookups)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().unwrap();

        assert_eq!(
            world.player_by_key(9).map(|p| p.name).as_deref(),
            Some("Bob")
        );
        assert!(world.player_by_key(1).is_none());
        let keys: Vec<u64> = world.players_named("ANN").iter().map(|p| p.key).collect();
        assert_eq!(keys, [7, 8]);
        assert!(world.players_named("Bo").is_empty());
    }
}
