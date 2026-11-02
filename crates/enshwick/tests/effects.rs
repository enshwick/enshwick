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

    struct Acting;

    impl Mod for Acting {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = EFFECTS.set(load.effects());
            Ok(Self)
        }
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
        let loader = host.template(Entity(5), [5; 16]).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Acting)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let effects = EFFECTS.get().unwrap();

        effects
            .npc_command(42, NpcCommandType::GoToPosition([1.0, 2.0, -3.0]))
            .unwrap();
        effects.add_threat(Entity(5), Entity(6), 2.5).unwrap();
        effects.ping(Entity(6), [0.5, 0.0, 0.0]).unwrap();
        assert_eq!(
            effects.set_toggle(Entity(5), 9, true),
            Err(WorldError::Missing),
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
                Entity(5),
                [10.0, 20.0, 30.0],
                Teleport::new().search_best_spawn_position(),
            )
            .unwrap();
        assert_eq!(
            effects.teleport(Entity(7), [0.0; 3], Teleport::new()),
            Err(WorldError::Gone)
        );
        let spawned = effects
            .spawn([7; 16], [1.0, 2.0, 3.0], Spawn::new().level(4))
            .unwrap();
        assert_eq!(
            effects.spawn([0; 16], [0.0; 3], Spawn::new()),
            Err(WorldError::Refused)
        );
        let spawns = loader.spawns();
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].0, spawned.0);
        assert_eq!((spawns[0].1.template, spawns[0].1.level), ([7; 16], 4));
        assert_eq!(spawns[0].1.position, [1 << 32, 2 << 32, 3 << 32]);
        let teleports = loader.teleports();
        assert_eq!(teleports.len(), 1);
        assert_eq!(teleports[0].entity, Entity(5));
        assert_eq!(teleports[0].flags, TeleportRequest::BEST_SPAWN);
        assert_eq!(teleports[0].position, [10 << 32, 20 << 32, 30 << 32]);
    }
}
