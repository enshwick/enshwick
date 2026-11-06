//! Mods add enemies as copies of vanilla ones with their own values and
//! arsenal, add arsenals of vanilla attacks and change vanilla arsenals,
//! the changes of two mods chained.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::enemies::{
        ARSENAL_REGISTRY, ArsenalChange, ArsenalId, AttackChange, NewArsenal, NewEnemy,
    };
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{BITE, ENEMY, ENEMY_ARSENAL, FakeHost, HOWL, LUNGE, OTHER_ARSENAL};
    use enshwick::typed::FactionType;
    use enshwick::{Load, Mod, Off};

    const ELDER: Id = Id::new("you.enemies:elder");
    const PACK: Id = Id::new("you.enemies:pack");

    static ADDED: Mutex<Option<ArsenalId>> = Mutex::new(None);
    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Two content ids of this mod whose game ids are one: the second is a
    /// collision.
    fn clashing() -> (Id, Id) {
        let mut seen = rustc_hash::FxHashMap::default();
        for i in 0u32.. {
            let name: &'static str = Box::leak(format!("you.enemies:n{i}").into_boxed_str());
            if let Some(first) = seen.insert(Guid::named(name).fnv1a(), name) {
                return (Id::new(first), Id::new(name));
            }
        }
        unreachable!("a 32-bit hash repeats")
    }

    fn refused(result: Result<impl Sized, Off>) {
        if let Err(off) = result {
            REFUSED.lock().unwrap().push(off.to_string());
        }
    }

    struct Enemies;

    impl Mod for Enemies {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let pack = load.add_arsenal(NewArsenal {
                id: PACK,
                like: ENEMY_ARSENAL,
                change: ArsenalChange {
                    attacks: Some(vec![HOWL, BITE]),
                    ..ArsenalChange::default()
                },
            })?;
            *ADDED.lock().unwrap() = Some(pack);
            load.add_enemy(NewEnemy {
                level: Some(20),
                health: Some(2.0),
                damage: Some(1.5),
                faction: Some(FactionType::Vukah),
                arsenal: Some(pack),
                boss: Some(true),
                ..NewEnemy::new(ELDER, ENEMY)
            })?;
            // Two changes of one vanilla arsenal apply in the order served.
            load.change_arsenal(
                ENEMY_ARSENAL,
                ArsenalChange {
                    attacks: Some(vec![LUNGE]),
                    ..ArsenalChange::default()
                },
            )?;
            load.change_arsenal(
                ENEMY_ARSENAL,
                ArsenalChange {
                    tune: vec![AttackChange {
                        threat: Some(9.0),
                        ..AttackChange::new(LUNGE)
                    }],
                    ..ArsenalChange::default()
                },
            )?;
            refused(load.add_arsenal(NewArsenal {
                id: Id::new("you.enemies:nothing"),
                like: ENEMY_ARSENAL,
                change: ArsenalChange {
                    attacks: Some(vec![Guid([9; 16])]),
                    ..ArsenalChange::default()
                },
            }));
            refused(load.change_arsenal(Guid([8; 16]), ArsenalChange::default()));
            refused(load.add_enemy(NewEnemy {
                level: Some(0),
                ..NewEnemy::new(Id::new("you.enemies:zero"), ENEMY)
            }));
            // A tune of an attack the arsenal does not pick.
            refused(load.add_arsenal(NewArsenal {
                id: Id::new("you.enemies:untuned"),
                like: ENEMY_ARSENAL,
                change: ArsenalChange {
                    attacks: Some(vec![HOWL]),
                    tune: vec![AttackChange::new(BITE)],
                    ..ArsenalChange::default()
                },
            }));
            // Not a boss already: no change, never a refusal.
            refused(load.add_enemy(NewEnemy {
                boss: Some(false),
                ..NewEnemy::new(Id::new("you.enemies:calm"), ENEMY)
            }));
            let (first, second) = clashing();
            for id in [first, second] {
                refused(load.add_arsenal(NewArsenal {
                    id,
                    like: OTHER_ARSENAL,
                    change: ArsenalChange::default(),
                }));
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Enemies,
        r#"
        id = "you.enemies"
        side = "both"
        "#
    );

    fn start() -> enshwick::test::Installed {
        let loader = FakeHost::new().enemies().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Enemies)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn built(
        loader: &enshwick::test::Installed,
        key: ResourceKey,
        base: ResourceKey,
        ty: &str,
    ) -> Value {
        let base = enshwick::host().unwrap().game_resource(base).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        Registry::game().unwrap().decode(ty, &bytes).unwrap()
    }

    fn component<'a>(template: &'a Value, name: &str) -> Option<&'a Value> {
        let Some(Value::List(components)) = template.field("components") else {
            return None;
        };
        components.iter().find_map(|held| match held {
            Value::Variant(held) if held.type_name() == name => Some(held.value()),
            _ => None,
        })
    }

    fn storage(value: Option<&Value>) -> Vec<Value> {
        match value.and_then(|value| value.field("dataStorage")) {
            Some(Value::List(storage)) => storage.clone(),
            _ => Vec::new(),
        }
    }

    fn guids(list: Option<&Value>) -> Vec<Guid> {
        match list {
            Some(Value::List(items)) => items
                .iter()
                .filter_map(|item| match item.field("guid") {
                    Some(Value::Guid(guid)) => Some(*guid),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn an_enemy_copy_takes_its_level_health_damage_faction_arsenal_and_boss_mark() {
        let loader = start();
        let host = enshwick::host().unwrap();
        let hash = host.hash("ecs::TemplateResource").unwrap();
        let elder = built(
            &loader,
            ELDER.guid().key_by_hash(hash),
            ENEMY.key_by_hash(hash),
            "demo::ecs::TemplateResource",
        );
        assert_eq!(
            storage(component(&elder, "demo::ecs::Level")),
            [Value::Uint(20), Value::Uint(105)]
        );
        let factor = component(&elder, "demo::ecs::EnemyScaling")
            .and_then(|scaling| scaling.field("enemyLevelScaling"))
            .and_then(|scaling| scaling.field("healthScaleFactor"));
        assert_eq!(factor, Some(&Value::Float(1.0)), "twice the vanilla 0.5");
        assert_eq!(
            storage(component(&elder, "demo::ecs::DamageMod"))[2],
            Value::Float(1.5)
        );
        assert_eq!(
            component(&elder, "demo::ecs::Faction").and_then(|f| f.field("faction")),
            Some(&Value::Name("Vukah".into()))
        );
        let pack = ADDED.lock().unwrap().unwrap();
        assert_eq!(pack.guid(), PACK.guid());
        assert_eq!(
            component(&elder, "demo::ecs::EnemyArsenalReference").and_then(|a| a.field("arsenal")),
            Some(&Value::Guid(pack.guid()))
        );
        assert!(component(&elder, "demo::ecs::Boss").is_some());

        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 5, "{refused:?}");
        assert!(refused[0].contains("no vanilla attack"), "{}", refused[0]);
        assert!(refused[1].contains("no vanilla arsenal"), "{}", refused[1]);
        assert!(refused[2].contains("levels start at 1"), "{}", refused[2]);
        assert!(
            refused[3].contains("holds no attack"),
            "a tune past the picks: {}",
            refused[3]
        );
        assert!(
            refused[4].contains("content id"),
            "a collision: {}",
            refused[4]
        );
    }

    #[test]
    fn arsenals_are_added_from_vanilla_attacks_and_two_changes_of_one_chain() {
        let loader = start();
        let hash = enshwick::host()
            .unwrap()
            .hash("enemy::EnemyArsenalRegistryResource")
            .unwrap();
        let key = ARSENAL_REGISTRY.key_by_hash(hash);
        let registry = built(
            &loader,
            key,
            key,
            "demo::enemy::EnemyArsenalRegistryResource",
        );
        let Some(Value::List(arsenals)) = registry.field("arsenals") else {
            panic!("{registry:?}")
        };
        assert_eq!(
            guids(registry.field("arsenals"))[..3],
            [ENEMY_ARSENAL, OTHER_ARSENAL, PACK.guid()]
        );
        assert_eq!(
            guids(arsenals[2].field("attacks")),
            [HOWL, BITE],
            "picked across arsenals"
        );
        assert_eq!(
            guids(arsenals[0].field("attacks")),
            [LUNGE],
            "the first change"
        );
        let threat = match arsenals[0].field("attacks") {
            Some(Value::List(attacks)) => attacks[0]
                .field("description")
                .and_then(|d| d.field("threat"))
                .cloned(),
            _ => None,
        };
        assert_eq!(
            threat,
            Some(Value::Float(9.0)),
            "the second change, on the first's result"
        );
    }
}
