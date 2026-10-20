//! Actions: typed game events and teleports, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick_loader_api::abi::{Status, TeleportRequest};
    use enshwick_loader_api::actions::{Actions, NpcOrder, Teleport};
    use enshwick_loader_api::ecs::Entity;
    use enshwick_loader_api::world::WorldError;
    use enshwick_loader_api::{Load, Mod, Off};

    static ACTIONS: OnceLock<Actions> = OnceLock::new();

    struct Acting;

    impl Mod for Acting {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "acting", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = ACTIONS.set(load.actions()?);
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Acting);

    fn word<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
        bytes[at..at + N].try_into().unwrap()
    }

    #[test]
    fn typed_actions_lay_out_their_events_and_teleports_their_requests() {
        let npc = "keen::ecs::NpcCommandEvent";
        let threat = "keen::ecs::AddThreatTargetEvent";
        let ping = "keen::ecs::UiPingEvent";
        let loader = enshwick_loader_api::test::FakeHost::new()
            .field_offset("keen::WorldTransform", "position", 0)
            .field_offset("keen::WorldTransform", "orientation", 24)
            .field_offset("keen::WorldTransform", "scale", 40)
            .type_hash(npc, 1)
            .type_size(npc, 72)
            .field_offset(npc, "type", 8)
            .field_offset(npc, "npcId", 12)
            .field_offset(npc, "targetTransform", 16)
            .type_hash(threat, 2)
            .type_size(threat, 24)
            .field_offset(threat, "attackerId", 8)
            .field_offset(threat, "targetId", 12)
            .field_offset(threat, "threatValue", 16)
            .type_hash(ping, 3)
            .type_size(ping, 40)
            .field_offset(ping, "senderEntityId", 8)
            .field_offset(ping, "position", 16)
            .template(5, [5; 16])
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let actions = ACTIONS.get().unwrap();

        actions
            .npc_command(42, NpcOrder::GoTo([1.0, 2.0, -3.0]))
            .unwrap();
        actions.add_threat(Entity(5), Entity(6), 2.5).unwrap();
        actions.ping(Entity(6), [0.5, 0.0, 0.0]).unwrap();
        assert_eq!(
            actions.set_toggle(Entity(5), 9, true),
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

        actions
            .teleport(Entity(5), [10.0, 20.0, 30.0], Teleport::new().best_spawn())
            .unwrap();
        assert_eq!(
            actions.teleport(Entity(7), [0.0; 3], Teleport::new()),
            Err(WorldError::Gone)
        );
        let teleports = loader.teleports();
        assert_eq!(teleports.len(), 1);
        assert_eq!(teleports[0].entity, 5);
        assert_eq!(teleports[0].flags, TeleportRequest::BEST_SPAWN);
        assert_eq!(teleports[0].position, [10 << 32, 20 << 32, 30 << 32]);
    }
}
