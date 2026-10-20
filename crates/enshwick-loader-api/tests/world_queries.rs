//! Entity queries, destroy requests, saves and per-world stores.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick_loader_api::abi::{Header, PlayerInfo, Status};
    use enshwick_loader_api::ecs::Entity;
    use enshwick_loader_api::events::{Shutdown, WorldLoaded, WorldSaved};
    use enshwick_loader_api::state::WorldStore;
    use enshwick_loader_api::world::{ComponentType, World, WorldError};
    use enshwick_loader_api::{Load, Mod, Off};

    const TRANSFORM: u32 = 1;
    const BED: u32 = 3;

    static WORLD: OnceLock<World> = OnceLock::new();
    static STORE: OnceLock<WorldStore<Visits>> = OnceLock::new();
    static BEDS: OnceLock<ComponentType> = OnceLock::new();

    #[derive(Default, serde::Deserialize, serde::Serialize)]
    struct Visits {
        count: u32,
    }

    struct Queries;

    impl Mod for Queries {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "queries", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let world = load.world()?;
            let _first = BEDS.set(world.component_type(load, "keen::ecs::NpcBed")?);
            let _first = WORLD.set(world);
            let _first = STORE.set(load.world_store("visits")?);
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Queries);

    #[test]
    fn queries_destroys_saves_and_stores_follow_the_world() {
        let data = std::env::temp_dir().join(format!("enshwick-queries-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let player = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: 2,
            key: 7,
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        let loader = enshwick_loader_api::test::FakeHost::new()
            .type_hash("keen::ecs::CurrentTransform", TRANSFORM)
            .type_hash("keen::ecs::UsedItem", 2)
            .type_hash("keen::ecs::NpcBed", BED)
            .placed(BED, 10, [0.0, 0.0, 30.0])
            .placed(BED, 11, [3.0, 0.0, 4.0])
            .placed(TRANSFORM, 2, [0.0, 0.0, 0.0])
            .player(player)
            .data_dir(data.to_str().unwrap())
            .world_key("world0-1a2b3c4d")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().unwrap();
        let store = STORE.get().unwrap();

        let beds = *BEDS.get().unwrap();
        assert_eq!(world.entities(beds).len(), 2);
        let near: Vec<Entity> = world
            .near([0.0; 3], 10.0, beds)
            .iter()
            .map(|p| p.entity)
            .collect();
        assert_eq!(near, [Entity(11)]);

        assert_eq!(world.destroy(Entity(11)), Ok(()));
        assert_eq!(world.destroy(Entity(2)), Err(WorldError::Refused));
        assert_eq!(world.destroy(Entity(99)), Err(WorldError::Gone));
        assert_eq!(loader.destroyed(), [11]);
        assert_eq!(world.save(), Ok(()));
        assert_eq!(loader.saves(), 1);
        assert_eq!(world.key().as_deref(), Some("world0-1a2b3c4d"));

        assert_eq!(store.update(|v| v.count += 1), None);
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1);
        assert_eq!(store.update(|v| v.count = 3), Some(()));
        assert_eq!(loader.raise::<WorldSaved>(&()), 1);
        assert_eq!(loader.raise::<Shutdown>(&()), 1);
        assert_eq!(store.key(), None);
        let file = data.join("worlds/world0-1a2b3c4d/visits.toml");
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("count = 3"), "{text}");
        let _gone = std::fs::remove_dir_all(&data);
    }
}
