//! Component writes: raw updates, typed fields and the stat setters.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::WorldError;
    use enshwick::abi::Status;
    use enshwick::game_events::Field;
    use enshwick::world::{Meter, World, WritableType};
    use enshwick::{Load, Mod, Off};

    /// What the mod resolved at init.
    struct Resolved {
        world: World,
        flags: WritableType,
        immune: Field<bool>,
        copy: WritableType,
    }

    static RESOLVED: OnceLock<Resolved> = OnceLock::new();

    struct Writes;

    impl Mod for Writes {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let world = load.world()?;
            let flags = WritableType::resolve(load, "keen::ecs::CombatFlags")?;
            let immune = flags.field(load, "immune")?;
            let copy = WritableType::resolve(load, "keen::ecs::NetworkHealth")?;
            let _first = RESOLVED.set(Resolved {
                world,
                flags,
                immune,
                copy,
            });
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Writes,
        r#"
        id = "you.writes"
        side = "server"
        "#
    );

    /// An attribute set: the id it keeps, the values' offset at 4, flags at
    /// 6, count at 7, then the values.
    fn set(root: u32, values: [i32; 8]) -> Vec<u8> {
        let mut bytes = root.to_le_bytes().to_vec();
        bytes.extend(8u16.to_le_bytes());
        bytes.extend([1, 8]);
        bytes.extend(values.iter().flat_map(|v| v.to_le_bytes()));
        bytes
    }

    #[test]
    fn writes_mark_their_components_and_stats_stay_in_bounds() {
        let health = set(10, [150, 0, 0, 0, 0, 0, 0, 200]);
        let dead = set(10, [0, 0, 0, 0, 0, 0, 0, 200]);
        let stamina = set(20, [80, 0, 100, 0, 0, 0, 0, 0]);
        let loader = enshwick::test::FakeHost::new()
            .world()
            .type_hash("attribute_set.health", 1)
            .type_hash("attribute_set.stamina", 2)
            .type_hash("attribute_set.mana", 3)
            .type_hash("attribute_set.level", 4)
            .type_hash("attribute_set.experience", 5)
            .type_hash("keen::ecs::CombatFlags", 6)
            .type_hash("keen::ecs::NetworkHealth", 7)
            .type_size("keen::ecs::CombatFlags", 8)
            .type_size("keen::ecs::NetworkHealth", 8)
            .field_offset("keen::ecs::CombatFlags", "immune", 0)
            .constant("ecs.level_data_offset", 4)
            .constant("ecs.level_flags", 6)
            .constant("ecs.attribute_count", 7)
            .constant("attribute.health", 10)
            .constant("attribute.health_max_slot", 7)
            .constant("attribute.stamina", 20)
            .constant("attribute.stamina_max_slot", 2)
            .constant("attribute.mana", 30)
            .constant("attribute.mana_max_slot", 1)
            .constant("attribute.level", 40)
            .constant("attribute.experience", 50)
            .constant("attribute.experience_needed_slot", 2)
            .component(Entity(7), 1, &health)
            .component(Entity(7), 2, &stamina)
            .component(Entity(7), 6, &[0; 8])
            .component(Entity(8), 1, &dead)
            .template(Entity(7), [1; 16])
            .template(Entity(8), [2; 16])
            .refuse_write(7)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Writes)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let resolved = RESOLVED.get().unwrap();
        let world = resolved.world;
        assert_eq!(
            world.update(Entity(7), resolved.copy, |_| ()),
            Err(WorldError::Refused),
            "the loader refuses the copies the game keeps for clients"
        );

        assert_eq!(
            world.set_health(Entity(7), 500),
            Ok(Meter {
                current: 200,
                max: 200
            })
        );
        assert_eq!(world.heal(Entity(7), -1000).map(|m| m.current), Ok(1));
        assert_eq!(world.heal(Entity(7), 49).map(|m| m.current), Ok(50));
        assert_eq!(world.set_health(Entity(8), 10), Err(WorldError::Refused));
        assert_eq!(world.set_stamina(Entity(7), -5).map(|m| m.current), Ok(0));
        assert_eq!(world.set_mana(Entity(7), 5), Err(WorldError::Lacks));
        assert_eq!(world.set_mana(Entity(9), 5), Err(WorldError::Gone));
        assert_eq!(world.vitals(Entity(7)).unwrap().health.current, 50);

        assert_eq!(world.get(Entity(7), resolved.immune), Some(false));
        assert_eq!(world.set(Entity(7), resolved.immune, true), Ok(()));
        assert_eq!(world.get(Entity(7), resolved.immune), Some(true));
        let size = world.update(Entity(7), resolved.flags, |bytes| {
            bytes[1] = 9;
            bytes.len()
        });
        assert_eq!(size, Ok(8));
        let flag = |at: usize| loader.component_bytes(Entity(7), 6).map(|flags| flags[at]);
        assert_eq!(flag(1), Some(9));
        assert_eq!(
            world.update(Entity(8), resolved.flags, |_| ()),
            Err(WorldError::Lacks)
        );
        let half = std::panic::catch_unwind(|| {
            world.update(Entity(7), resolved.flags, |bytes| {
                bytes[2] = 4;
                panic!("halfway");
            })
        });
        assert!(half.is_err());
        assert_eq!(flag(2), Some(0));
        let nested = world.update(Entity(7), resolved.flags, |outer| {
            outer[3] = 1;
            world.update(Entity(7), resolved.flags, |inner| inner[3])
        });
        assert_eq!(nested, Ok(Ok(0)));
        assert_eq!(flag(3), Some(1));
        assert!(loader.writes().contains(&(7, 6)));
        assert!(loader.writes().contains(&(7, 1)));
    }
}
