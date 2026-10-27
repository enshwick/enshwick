//! Static halves: a component type that holds its template's set-up values
//! reads through the world like any other and is never written.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::ecs::Entity;
    use enshwick::world::{ComponentType, World, WorldError, WritableType};
    use enshwick::{Load, Mod, Off};

    const FUEL_SETUP: u32 = 0x139a_fbec;
    const FUEL: u32 = 0x0418_2abb;
    const WELL: Entity = Entity(4096);

    struct Resolved {
        world: World,
        setup: ComponentType,
        setup_writes: WritableType,
        fuel_writes: WritableType,
    }

    static RESOLVED: OnceLock<Resolved> = OnceLock::new();

    struct Wells;

    impl Mod for Wells {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.wells", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let world = load.world()?;
            let _first = RESOLVED.set(Resolved {
                world,
                setup: world.component_type(load, "keen::ecs::FuelSetup")?,
                setup_writes: world.writable_type(load, "keen::ecs::FuelSetup")?,
                fuel_writes: world.writable_type(load, "keen::ecs::Fuel")?,
            });
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Wells);

    #[test]
    fn a_static_half_reads_its_set_up_values_and_refuses_writes() {
        let mut setup = vec![0u8; 0x28];
        setup[0x8] = 1;
        let loader = enshwick::test::FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .type_hash("keen::ecs::FuelSetup", FUEL_SETUP)
            .type_hash("keen::ecs::Fuel", FUEL)
            .type_size("keen::ecs::FuelSetup", 0x28)
            .type_size("keen::ecs::Fuel", 16)
            .component(WELL.0, FUEL_SETUP, &setup)
            .component(WELL.0, FUEL, &[0; 16])
            .static_half(FUEL_SETUP)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Wells)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let resolved = RESOLVED.get().unwrap();
        let world = resolved.world;

        let data = world.component(WELL, resolved.setup).unwrap();
        // SAFETY: the fake keeps the well's 0x28-byte `FuelSetup` for the
        // process.
        let automatic: u8 = unsafe { enshwick::raw::read_at(data.cast(), 0x8) };
        assert_eq!(automatic, 1, "the set-up value the template gives");

        assert_eq!(
            world.update(WELL, resolved.setup_writes, |_| ()),
            Err(WorldError::StaticHalf)
        );
        assert_eq!(world.update(WELL, resolved.fuel_writes, |_| ()), Ok(()));
        assert!(
            loader.writes().iter().all(|&(_, ty)| ty == FUEL),
            "nothing wrote the static half"
        );
    }
}
