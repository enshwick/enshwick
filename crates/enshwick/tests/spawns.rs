//! Mods add spawn entries to the game's random spawners, by spawner or
//! where an enemy spawns already, and change their counts and timeouts.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::content::spawns::{SpawnEntry, SpawnerChange, SpawnerPick};
    use enshwick::content::{EngineFacts, Guid};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{ENEMY, FakeHost, NEST, NEST_GROUP};
    use enshwick::{Load, Mod, Off};

    const ELDER: Guid = Guid::parse("0d75b9a9-7e48-4f3a-bf2a-19bf6021f531");

    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Spawns;

    impl Mod for Spawns {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_spawn_entry(SpawnEntry {
                weight: 2.0,
                ..SpawnEntry::new(SpawnerPick::Listing(ENEMY), ELDER)
            })?;
            load.change_spawner(
                SpawnerPick::Spawner(NEST),
                SpawnerChange {
                    max_entities_alive: Some(9),
                    min_spawn_timeout: Some(1.5),
                    ..SpawnerChange::default()
                },
            )?;
            // A GUID no spawner holds, and one the nest holds but not as a template.
            for listed in [Guid([7; 16]), NEST_GROUP] {
                if let Err(off) =
                    load.add_spawn_entry(SpawnEntry::new(SpawnerPick::Listing(listed), ELDER))
                {
                    REFUSED.lock().unwrap().push(off.to_string());
                }
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Spawns,
        r#"
        id = "you.spawns"
        side = "server"
        "#
    );

    #[test]
    fn a_spawner_gets_the_entry_where_the_enemy_spawns_and_its_changed_counts() {
        let loader = FakeHost::new().enemies().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Spawns)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());

        let host = enshwick::host().unwrap();
        let key = NEST.key_by_hash(host.hash("ecs::TemplateResource").unwrap());
        let base = host.game_resource(key).unwrap();
        let built = loader.build_resource(&key, &base).unwrap();
        let nest = Registry::game()
            .unwrap()
            .decode("demo::ecs::TemplateResource", &built)
            .unwrap();
        let Some(Value::List(components)) = nest.field("components") else {
            panic!("{nest:?}")
        };
        let Some(Value::Variant(spawner)) = components.first() else {
            panic!("{components:?}")
        };
        let spawner = spawner.value();
        let Some(Value::List(entries)) = spawner.field("spawnEntries") else {
            panic!("{spawner:?}")
        };
        let templates: Vec<_> = entries
            .iter()
            .map(|entry| entry.field("entityTemplates").cloned())
            .collect();
        assert_eq!(
            templates,
            [
                Some(Value::List(vec![Value::Guid(ENEMY)])),
                Some(Value::List(vec![Value::Guid(ELDER)])),
            ]
        );
        assert_eq!(entries[1].field("weight"), Some(&Value::Float(2.0)));
        assert_eq!(spawner.field("maxEntitiesAlive"), Some(&Value::Uint(9)));
        assert_eq!(
            spawner.field("maxSpawnAmount"),
            Some(&Value::Uint(3)),
            "kept"
        );
        assert_eq!(
            spawner
                .field("minSpawnTimeout")
                .and_then(|t| t.field("value")),
            Some(&Value::Int(1_500_000_000))
        );

        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 2, "{refused:?}");
        assert!(
            refused[0].contains("no vanilla random spawner lists"),
            "{}",
            refused[0]
        );
    }
}
