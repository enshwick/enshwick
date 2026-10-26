//! Component writes: raw updates, typed fields and the stat setters.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::ecs::Entity;
    use enshwick::game_events::Field;
    use enshwick::world::{Meter, World, WorldError, WritableType};
    use enshwick::{Load, Mod, Off};

    /// What the mod resolved at init.
    struct Resolved {
        world: World,
        flags: WritableType,
        immune: Field<bool>,
        refused: bool,
    }

    static RESOLVED: OnceLock<Resolved> = OnceLock::new();

    struct Writes;

    impl Mod for Writes {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "writes", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let world = load.world()?;
            let flags = world.writable_type(load, "keen::ecs::CombatFlags")?;
            let immune = flags.field(load, "immune")?;
            let refused = world
                .writable_type(load, "keen::ecs::NetworkHealth")
                .is_err();
            let _first = RESOLVED.set(Resolved {
                world,
                flags,
                immune,
                refused,
            });
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Writes);

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
            .type_hash("keen::ecs::CurrentTransform", 100)
            .type_hash("keen::ecs::UsedItem", 101)
            .type_hash("keen::ecs::Health", 1)
            .type_hash("keen::ecs::Stamina", 2)
            .type_hash("keen::ecs::Mana", 3)
            .type_hash("keen::ecs::Level", 4)
            .type_hash("keen::ecs::Experience", 5)
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
            .component(7, 1, &health)
            .component(7, 2, &stamina)
            .component(7, 6, &[0; 8])
            .component(8, 1, &dead)
            .template(7, [1; 16])
            .template(8, [2; 16])
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let resolved = RESOLVED.get().unwrap();
        let world = resolved.world;
        assert!(resolved.refused, "a copy for clients is not writable");

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
        let flag = |at: usize| loader.component_bytes(7, 6).map(|flags| flags[at]);
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
