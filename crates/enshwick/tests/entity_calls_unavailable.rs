//! The entity calls tell a call this process cannot make apart from an
//! entity that is gone.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Side, Status};
    use enshwick::effects::{Effects, Teleport};
    use enshwick::test::FakeHost;
    use enshwick::world::{World, WritableType};
    use enshwick::{Entity, Load, Mod, Off, WorldError};

    static CALLS: OnceLock<(World, Effects, WritableType)> = OnceLock::new();

    struct Caller;

    impl Mod for Caller {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let flags = WritableType::resolve(load, "keen::ecs::CombatFlags")?;
            let _first = CALLS.set((load.world()?, load.effects(), flags));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Caller,
        r#"
        id = "you.caller"
        side = "both"
        "#
    );

    /// A world where entity 5 is in the world and entity 9 is not.
    fn world() -> FakeHost {
        FakeHost::new()
            .world()
            .type_hash("keen::ecs::CombatFlags", 6)
            .type_size("keen::ecs::CombatFlags", 8)
            .component(Entity(5), 6, &[0; 8])
            .template(Entity(5), [5; 16])
    }

    /// What destroy, `set_saved`, teleport and a component write answer for
    /// `entity` on `host`.
    fn answers(host: FakeHost, entity: Entity) -> [Result<(), WorldError>; 4] {
        let loader = host.install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Caller)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let (world, effects, flags) = CALLS.get().unwrap();
        [
            world.set_saved(entity, false),
            effects.teleport(entity, [0.0; 3], Teleport::new()),
            world.update(entity, *flags, |_| ()),
            world.destroy(entity),
        ]
    }

    #[test]
    fn on_a_client_that_hosts_no_game_every_entity_call_is_missing_not_gone() {
        let answered = answers(world().side(Side::CLIENT), Entity(5));
        assert_eq!(answered, [Err(WorldError::Missing); 4]);
        let unknown = answers(world().side(Side::CLIENT), Entity(9));
        assert_eq!(unknown, [Err(WorldError::Missing); 4]);
    }

    #[test]
    fn off_the_game_thread_a_client_that_hosts_no_game_keeps_saves_missing() {
        let loader = world().side(Side::CLIENT).install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Caller)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let (world, _, _) = CALLS.get().unwrap();
        let world = *world;
        let answered = std::thread::spawn(move || world.set_saved(Entity(5), false))
            .join()
            .unwrap();
        assert_eq!(answered, Err(WorldError::Missing));
    }

    #[test]
    fn a_client_that_hosts_its_game_answers_gone_for_an_entity_not_in_the_world() {
        let answered = answers(world().side(Side::CLIENT).hosting(true), Entity(9));
        assert_eq!(answered, [Err(WorldError::Gone); 4]);
    }

    #[test]
    fn a_build_without_the_entity_commands_is_missing_them_for_every_entity() {
        let answered = answers(world().without_entity_commands(), Entity(5));
        assert_eq!(answered, [Err(WorldError::Missing); 4]);
        let gone = answers(world(), Entity(9));
        assert_eq!(gone, [Err(WorldError::Gone); 4]);
    }
}
