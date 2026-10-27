//! Effects: typed game events and teleports, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::abi::{Status, TeleportRequest};
    use enshwick::effects::{Effects, NpcCommandType, Spawn, Teleport};
    use enshwick::world::WorldError;
    use enshwick::{Load, Mod, Off};

    static EFFECTS: OnceLock<Effects> = OnceLock::new();

    struct Acting;

    impl Mod for Acting {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.acting", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = EFFECTS.set(load.effects());
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Acting);

    fn word<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
        bytes[at..at + N].try_into().unwrap()
    }

    #[test]
    fn typed_effects_lay_out_their_events_and_teleports_their_requests() {
        let npc = "npc_command_event";
        let threat = "add_threat_target_event";
        let ping = "ui_ping_event";
        let loader = enshwick::test::FakeHost::new()
            .constant("world_transform.position", 0)
            .constant("world_transform.orientation", 24)
            .constant("world_transform.scale", 40)
            .type_hash(npc, 1)
            .type_size(npc, 72)
            .constant("npc_command_event.type", 8)
            .constant("npc_command_event.npc_id", 12)
            .constant("npc_command_event.target_transform", 16)
            .type_hash(threat, 2)
            .type_size(threat, 24)
            .constant("add_threat_target_event.attacker_id", 8)
            .constant("add_threat_target_event.target_id", 12)
            .constant("add_threat_target_event.threat_value", 16)
            .type_hash(ping, 3)
            .type_size(ping, 40)
            .constant("ui_ping_event.sender_entity_id", 8)
            .constant("ui_ping_event.position", 16)
            .template(Entity(5), [5; 16])
            .install();
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
        assert_eq!((*ty, order.len(), order[8]), (1, 72, 1));
        assert_eq!(u32::from_le_bytes(word(order, 12)), 42);
        assert_eq!(i64::from_le_bytes(word(order, 16)), 1 << 32);
        assert_eq!(i64::from_le_bytes(word(order, 32)), -3 << 32);
        assert_eq!(
            word::<4>(order, 16 + 36),
            1.0f32.to_le_bytes(),
            "w of the quaternion"
        );
        assert_eq!(word::<4>(order, 16 + 40), 1.0f32.to_le_bytes(), "scale");
        let (_, aggro) = &emits[1];
        assert_eq!(u32::from_le_bytes(word(aggro, 8)), 5);
        assert_eq!(u32::from_le_bytes(word(aggro, 12)), 6);
        assert_eq!(word::<4>(aggro, 16), 2.5f32.to_le_bytes());
        let (_, pinged) = &emits[2];
        assert_eq!(i64::from_le_bytes(word(pinged, 16)), 1 << 31);

        effects
            .teleport(Entity(5), [10.0, 20.0, 30.0], Teleport::new().best_spawn())
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
