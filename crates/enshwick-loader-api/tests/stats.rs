//! Health, stamina, mana, level and experience from attribute sets.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::ecs::Entity;
    use enshwick_loader_api::world::{Meter, World};
    use enshwick_loader_api::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    /// An attribute set: the id it keeps, the values' offset at 4, flags at
    /// 6, count at 7, then the values.
    #[repr(C)]
    struct Set {
        root: u32,
        data: u16,
        flags: u8,
        count: u8,
        values: [i32; 8],
    }

    struct Stats;

    impl Mod for Stats {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "stats", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Stats);

    const fn set(root: u32, values: [i32; 8]) -> Set {
        Set {
            root,
            data: 8,
            flags: 1,
            count: 8,
            values,
        }
    }

    #[test]
    fn vitals_and_progress_read_their_slots_of_the_expected_sets() {
        let health = set(10, [150, 0, 0, 0, 0, 0, 0, 200]);
        let stamina = set(20, [80, 0, 100, 0, 0, 0, 0, 0]);
        let level = set(40, [12, 50, 0, 0, 0, 0, 0, 0]);
        let experience = set(50, [300, 4, 900, 0, 0, 0, 0, 0]);
        let wrong = set(99, [1; 8]);
        let loader = enshwick_loader_api::test::FakeHost::new()
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
            .component(7, 1, core::ptr::from_ref(&health).cast())
            .component(7, 2, core::ptr::from_ref(&stamina).cast())
            .component(7, 4, core::ptr::from_ref(&level).cast())
            .component(7, 5, core::ptr::from_ref(&experience).cast())
            .component(8, 1, core::ptr::from_ref(&wrong).cast())
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
    }
}
