//! Effects: typed game events and teleports, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::WorldError;
    use enshwick::abi::{Status, TeleportRequest};
    use enshwick::content::EngineFacts;
    use enshwick::effects::{Effects, NpcCommandType, Spawn, Teleport};
    use enshwick::test::FakeHost;
    use enshwick::test::facts::{self, Fact};
    use enshwick::{Load, Mod, Off};

    static EFFECTS: OnceLock<Effects> = OnceLock::new();

    /// What an NPC order answered in the mod's init.
    static IN_INIT: OnceLock<Result<(), WorldError>> = OnceLock::new();

    /// What an NPC order answered in the mod's ready.
    static IN_READY: OnceLock<Result<(), WorldError>> = OnceLock::new();

    struct Acting;

    impl Mod for Acting {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let effects = load.effects();
            let _first = IN_INIT.set(effects.npc_command(42, NpcCommandType::Reset));
            let _first = EFFECTS.set(effects);
            Ok(Self)
        }

        fn ready(&'static self, _host: enshwick::Host) {
            if let Some(effects) = EFFECTS.get() {
                let _first = IN_READY.set(effects.npc_command(42, NpcCommandType::Reset));
            }
        }
    }

    #[test]
    fn an_npc_order_in_init_finds_no_world_and_goes_once_init_is_done() {
        let loader = FakeHost::new().world().effect_events().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Acting)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(
            IN_INIT.get(),
            Some(&Err(WorldError::Unavailable)),
            "the loader has no world while mods initialise"
        );
        assert_eq!(
            IN_READY.get(),
            Some(&Err(WorldError::Unavailable)),
            "nor when it calls ready, at its start"
        );
        assert!(loader.emits().is_empty());
        EFFECTS
            .get()
            .unwrap()
            .npc_command(42, NpcCommandType::Reset)
            .unwrap();
        assert_eq!(loader.emits().len(), 1);
    }

    enshwick::test::declare_mod!(
        Acting,
        r#"
        id = "you.acting"
        side = "server"
        "#
    );

    fn word<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
        bytes[at..at + N].try_into().unwrap()
    }

    #[test]
    fn typed_effects_lay_out_their_events_and_teleports_their_requests() {
        let e = Entity::from_engine;
        let host = FakeHost::new().world().effect_events();
        let at = |fact: Fact| host.offset(fact.id()).unwrap();
        let (position, orientation, scale) = (
            at(facts::WORLD_TRANSFORM_POSITION),
            at(facts::WORLD_TRANSFORM_ORIENTATION),
            at(facts::WORLD_TRANSFORM_SCALE),
        );
        let (kind, npc, target) = (
            at(facts::NPC_COMMAND_EVENT_TYPE),
            at(facts::NPC_COMMAND_EVENT_NPC_ID),
            at(facts::NPC_COMMAND_EVENT_TARGET_TRANSFORM),
        );
        let (attacker, victim, threat) = (
            at(facts::ADD_THREAT_TARGET_EVENT_ATTACKER_ID),
            at(facts::ADD_THREAT_TARGET_EVENT_TARGET_ID),
            at(facts::ADD_THREAT_TARGET_EVENT_THREAT_VALUE),
        );
        let pinged_at = at(facts::UI_PING_EVENT_POSITION);
        let npc_event = host.hash(facts::NPC_COMMAND_EVENT.id()).unwrap();
        let npc_size = host.size(facts::NPC_COMMAND_EVENT.id()).unwrap();
        let loader = host
            .template(e(5), enshwick::content::Guid([5; 16]))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Acting)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let effects = EFFECTS.get().unwrap();

        effects
            .npc_command(42, NpcCommandType::GoToPosition([1.0, 2.0, -3.0]))
            .unwrap();
        effects.add_threat(e(5), e(6), 2.5).unwrap();
        effects.ping(e(6), [0.5, 0.0, 0.0]).unwrap();
        assert_eq!(
            effects.set_toggle(e(5), 9, true),
            Err(WorldError::Unavailable),
            "the toggle event is not on this fake build"
        );
        let emits = loader.emits();
        assert_eq!(emits.len(), 3);
        let (ty, order) = &emits[0];
        assert_eq!((*ty, order.len(), order[kind]), (npc_event, npc_size, 1));
        assert_eq!(u32::from_le_bytes(word(order, npc)), 42);
        assert_eq!(i64::from_le_bytes(word(order, target + position)), 1 << 32);
        assert_eq!(
            i64::from_le_bytes(word(order, target + position + 16)),
            -3 << 32
        );
        assert_eq!(
            word::<4>(order, target + orientation + 12),
            1.0f32.to_le_bytes(),
            "w of the quaternion"
        );
        assert_eq!(
            word::<4>(order, target + scale),
            1.0f32.to_le_bytes(),
            "scale"
        );
        let (_, aggro) = &emits[1];
        assert_eq!(u32::from_le_bytes(word(aggro, attacker)), 5);
        assert_eq!(u32::from_le_bytes(word(aggro, victim)), 6);
        assert_eq!(word::<4>(aggro, threat), 2.5f32.to_le_bytes());
        let (_, pinged) = &emits[2];
        assert_eq!(i64::from_le_bytes(word(pinged, pinged_at)), 1 << 31);

        effects
            .teleport(
                e(5),
                [10.0, 20.0, 30.0],
                Teleport::new().search_best_spawn_position(),
            )
            .unwrap();
        assert_eq!(
            effects.teleport(e(7), [0.0; 3], Teleport::new()),
            Err(WorldError::Gone)
        );
        let spawned = effects
            .spawn(
                enshwick::content::Guid([7; 16]),
                [1.0, 2.0, 3.0],
                Spawn::new().level(4),
            )
            .unwrap();
        assert_eq!(
            effects.spawn(enshwick::content::Guid([0; 16]), [0.0; 3], Spawn::new()),
            Err(WorldError::BadArgs)
        );
        let spawns = loader.spawns();
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].0, spawned.get());
        assert_eq!(
            (spawns[0].1.template, spawns[0].1.level),
            (enshwick::content::Guid([7; 16]), 4)
        );
        assert_eq!(spawns[0].1.position, [1.0, 2.0, 3.0]);
        let teleports = loader.teleports();
        assert_eq!(teleports.len(), 1);
        assert_eq!(teleports[0].entity, e(5));
        assert_eq!(teleports[0].flags, TeleportRequest::BEST_SPAWN);
        assert_eq!(teleports[0].position, [10.0, 20.0, 30.0]);
    }

    /// Positions past the engine's 32.32 fixed point: NaN, huge and 2^31
    /// metres, the first value it cannot hold.
    const PAST_THE_FIXED_POINT: [[f64; 3]; 3] = [
        [f64::NAN, 0.0, 0.0],
        [1e300, 0.0, 0.0],
        [0.0, -2_147_483_648.0, 0.0],
    ];

    /// The last metres on each axis the fixed point holds.
    const AT_THE_BOUND: [f64; 3] = [2_147_483_647.0, -2_147_483_647.0, 0.0];

    #[test]
    fn a_spawn_teleport_order_or_ping_past_the_fixed_point_is_refused_and_reaches_nothing() {
        let loader = FakeHost::new()
            .world()
            .effect_events()
            .template(Entity::from_engine(5), enshwick::content::Guid([5; 16]))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Acting)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let effects = EFFECTS.get().unwrap();
        let template = enshwick::content::Guid([7; 16]);
        for far in PAST_THE_FIXED_POINT {
            assert_eq!(
                effects.spawn(template, far, Spawn::new()),
                Err(WorldError::BadArgs),
                "spawn {far:?}"
            );
            assert_eq!(
                effects.teleport(Entity::from_engine(5), far, Teleport::new()),
                Err(WorldError::BadArgs),
                "teleport {far:?}"
            );
            assert_eq!(
                effects.npc_command(42, NpcCommandType::GoToPosition(far)),
                Err(WorldError::BadArgs),
                "order {far:?}"
            );
            assert_eq!(
                effects.ping(Entity::from_engine(6), far),
                Err(WorldError::BadArgs),
                "ping {far:?}"
            );
        }
        assert!(loader.spawns().is_empty() && loader.teleports().is_empty());
        assert!(loader.emits().is_empty());

        effects.spawn(template, AT_THE_BOUND, Spawn::new()).unwrap();
        assert_eq!(
            effects.teleport(Entity::from_engine(5), AT_THE_BOUND, Teleport::new()),
            Err(WorldError::OutsideWorld),
            "the fixed point holds it, the game's world box does not"
        );
        effects
            .npc_command(42, NpcCommandType::GoToPosition(AT_THE_BOUND))
            .unwrap();
        effects.ping(Entity::from_engine(6), AT_THE_BOUND).unwrap();
        assert_eq!(loader.spawns()[0].1.position, AT_THE_BOUND);
        assert!(loader.teleports().is_empty());
        assert_eq!(loader.emits().len(), 2);
    }

    #[test]
    fn a_teleport_past_the_worlds_box_is_outside_the_world_and_one_inside_it_lands() {
        let loader = FakeHost::new()
            .world()
            .world_box([0.0, -64.0, 0.0], [10_240.0, 2_560.0, 10_240.0])
            .template(Entity::from_engine(5), enshwick::content::Guid([5; 16]))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Acting)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let effects = EFFECTS.get().unwrap();
        let edge = 2_147_483_647.999_999_8;
        for far in [
            [edge, 900.0, edge],
            [10_300.0, 100.0, 0.0],
            [-5.0, 100.0, 0.0],
            [0.0, 2_561.0, 0.0],
        ] {
            assert_eq!(
                effects.teleport(Entity::from_engine(5), far, Teleport::new()),
                Err(WorldError::OutsideWorld),
                "teleport {far:?}"
            );
        }
        assert!(loader.teleports().is_empty());
        assert_eq!(
            effects.teleport(
                Entity::from_engine(5),
                [f64::NAN, 0.0, 0.0],
                Teleport::new()
            ),
            Err(WorldError::BadArgs),
            "a position the fixed point cannot hold stays a bad argument"
        );
        let corner = [10_240.0, 2_560.0, 10_240.0];
        effects
            .teleport(Entity::from_engine(5), corner, Teleport::new())
            .unwrap();
        assert_eq!(loader.teleports()[0].position, corner);
    }

    #[test]
    fn the_fake_keeps_the_games_box_and_names_a_missing_entity_before_its_target() {
        let loader = FakeHost::new()
            .world()
            .template(Entity::from_engine(5), enshwick::content::Guid([5; 16]))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Acting)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let effects = EFFECTS.get().unwrap();
        let far = [15_000.0, 100.0, 0.0];
        assert_eq!(
            effects.teleport(Entity::from_engine(5), far, Teleport::new()),
            Err(WorldError::OutsideWorld)
        );
        assert_eq!(
            effects.teleport(Entity::from_engine(9), far, Teleport::new()),
            Err(WorldError::Gone),
            "the entity is looked up before the box"
        );
        assert!(loader.teleports().is_empty());
    }

    #[test]
    fn without_entity_calls_a_far_spawn_or_teleport_is_unavailable_as_the_loader_answers() {
        let loader = FakeHost::new()
            .world()
            .without_entity_commands()
            .template(Entity::from_engine(5), enshwick::content::Guid([5; 16]))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Acting)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let effects = EFFECTS.get().unwrap();
        for far in PAST_THE_FIXED_POINT {
            assert_eq!(
                effects.spawn(enshwick::content::Guid([7; 16]), far, Spawn::new()),
                Err(WorldError::Unavailable),
                "spawn {far:?}"
            );
            assert_eq!(
                effects.teleport(Entity::from_engine(5), far, Teleport::new()),
                Err(WorldError::Unavailable),
                "teleport {far:?}"
            );
        }
    }
}
