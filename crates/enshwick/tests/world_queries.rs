//! Entity queries, destroy requests, saves and per-world stores.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::WorldError;
    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::content::EngineFacts;
    use enshwick::ecs::ComponentType;
    use enshwick::events::{ShutdownStarted, WorldLoaded, WorldSaved};
    use enshwick::state::WorldStore;
    use enshwick::test::{FakeHost, facts};
    use enshwick::world::World;
    use enshwick::{Load, Mod, Off};

    const BED: u32 = 3;

    /// The hash the world preset gives the current transform.
    fn transform() -> u32 {
        FakeHost::new()
            .world()
            .hash(facts::CURRENT_TRANSFORM.id())
            .unwrap()
    }

    static WORLD: OnceLock<World> = OnceLock::new();
    static STORE: OnceLock<WorldStore<Visits>> = OnceLock::new();
    static BEDS: OnceLock<ComponentType> = OnceLock::new();

    #[derive(Default, serde::Deserialize, serde::Serialize)]
    struct Visits {
        count: u32,
    }

    struct Queries;

    impl Mod for Queries {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let world = load.world()?;
            let _first = BEDS.set(ComponentType::resolve(load, "keen::ecs::NpcBed")?);
            let _first = WORLD.set(world);
            let _first = STORE.set(load.world_store("visits")?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Queries,
        r#"
        id = "you.queries"
        side = "server"
        "#
    );

    #[test]
    fn queries_destroys_saves_and_stores_follow_the_world() {
        let data = std::env::temp_dir().join(format!("enshwick-queries-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let player = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(2),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        let loader = FakeHost::new()
            .world()
            .type_hash("keen::ecs::NpcBed", BED)
            .placed(BED, Entity(10), [0.0, 0.0, 30.0])
            .placed(BED, Entity(11), [3.0, 0.0, 4.0])
            .placed(transform(), Entity(2), [0.0, 0.0, 0.0])
            .player(player)
            .data_dir(data.to_str().unwrap())
            .world_key("world0-1a2b3c4d")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Queries)`.
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
        assert_eq!(loader.raise::<ShutdownStarted>(&()), 1);
        assert_eq!(store.key(), None);
        let file = data.join("worlds/world0-1a2b3c4d/visits.toml");
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("count = 3"), "{text}");
        let _gone = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_store_that_does_not_read_stays_shut_and_says_why_in_the_status() {
        let data = std::env::temp_dir().join(format!("enshwick-shut-store-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let file = data.join("worlds/world0-1a2b3c4d/visits.toml");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "count = ").unwrap();
        let loader = FakeHost::new()
            .world()
            .type_hash("keen::ecs::NpcBed", BED)
            .data_dir(data.to_str().unwrap())
            .world_key("world0-1a2b3c4d")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Queries)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let store = STORE.get().unwrap();
        assert!(
            loader.status().concat().is_empty(),
            "nothing to say before the world"
        );
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1);
        assert_eq!(store.key(), None);
        assert!(store.shut_reason().is_some());
        let lines = loader.status().concat();
        assert!(
            lines.contains("visits") && lines.contains("does not parse"),
            "{lines}"
        );
        let shown = |loader: &enshwick::test::Installed| {
            loader
                .features()
                .into_iter()
                .find(|(name, _)| name == "world store visits")
                .and_then(|(_, off)| off)
        };
        let off = shown(&loader).unwrap_or_default();
        assert!(
            off.contains("does not parse"),
            "the Mods screen shows the store off: {off:?}"
        );
        assert_eq!(loader.raise::<WorldSaved>(&()), 1);
        assert_eq!(loader.raise::<ShutdownStarted>(&()), 1);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "count = ",
            "kept for a fix"
        );
        let _gone = std::fs::remove_dir_all(&data);
    }
}
