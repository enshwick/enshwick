//! Health, stamina, mana, level and experience from attribute sets, and an
//! item entity's meter read and written.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::ecs::Entity;
    use enshwick::world::{Meter, World};
    use enshwick::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Stats;

    impl Mod for Stats {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "stats", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Stats);

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
    fn vitals_and_progress_read_their_slots_of_the_expected_sets() {
        let health = set(10, [150, 0, 0, 0, 0, 0, 0, 200]);
        let stamina = set(20, [80, 0, 100, 0, 0, 0, 0, 0]);
        let level = set(40, [12, 50, 0, 0, 0, 0, 0, 0]);
        let experience = set(50, [300, 4, 900, 0, 0, 0, 0, 0]);
        let wrong = set(99, [1; 8]);
        let fuel = set(60, [2, 3, 0, 0, 0, 0, 0, 0]);
        let loader = enshwick::test::FakeHost::new()
            .type_hash("keen::ecs::CurrentTransform", 100)
            .type_hash("keen::ecs::UsedItem", 101)
            .type_hash("keen::ecs::Health", 1)
            .type_hash("keen::ecs::Stamina", 2)
            .type_hash("keen::ecs::Mana", 3)
            .type_hash("keen::ecs::Level", 4)
            .type_hash("keen::ecs::Experience", 5)
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
            .type_hash("keen::ecs::Fuel", 6)
            .constant("attribute.fuel", 60)
            .constant("attribute.fuel_max_slot", 1)
            .component(900, 6, &fuel)
            .component(7, 1, &health)
            .component(7, 2, &stamina)
            .component(7, 4, &level)
            .component(7, 5, &experience)
            .component(8, 1, &wrong)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().unwrap();

        let vitals = world.vitals(Entity(7)).unwrap();
        assert_eq!(
            vitals.health,
            Meter {
                current: 150,
                max: 200
            }
        );
        assert_eq!(
            vitals.stamina,
            Some(Meter {
                current: 80,
                max: 100
            })
        );
        assert_eq!(vitals.mana, None);
        let progress = world.progress(Entity(7)).unwrap();
        assert_eq!(
            (
                progress.level,
                progress.experience,
                progress.experience_needed
            ),
            (12, 300, 900)
        );
        assert!(world.vitals(Entity(8)).is_none());
        assert!(world.vitals(Entity(9)).is_none());

        let charges = |current| Meter { current, max: 3 };
        assert_eq!(world.item_meter(Entity(900)), Some(charges(2)));
        assert_eq!(world.item_meter(Entity(7)), None);
        assert_eq!(
            world.change_item_meter(Entity(900), |c| c - 1),
            Ok(charges(1))
        );
        assert_eq!(
            world.change_item_meter(Entity(900), |c| c - 5),
            Ok(charges(0))
        );
        assert_eq!(world.change_item_meter(Entity(900), |_| 9), Ok(charges(3)));
        assert_eq!(world.item_meter(Entity(900)), Some(charges(3)));
        assert!(world.change_item_meter(Entity(7), |c| c).is_err());
        assert_eq!(loader.writes(), [(900, 6), (900, 6), (900, 6)]);
    }
}
