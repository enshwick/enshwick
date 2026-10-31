//! Loot from destroyed things links only where the build names the single
//! roll's output slot.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::loot::{Drop, Kinds, Source};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const ROLL: u64 = 0x1_4010_0000;
    const ROLL_ONE: u64 = 0x1_4020_0000;

    struct Drops {
        destroyed: Result<(), String>,
    }

    impl Mod for Drops {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let destroyed = load
                .add_loot(Drop::new(
                    enshwick::ItemId::new(7),
                    0.5,
                    Source::kinds(Kinds::DESTROYED),
                ))
                .map_err(|off| off.to_string());
            Ok(Self { destroyed })
        }
    }

    enshwick::test::declare_mod!(
        Drops,
        r#"
        id = "you.drops"
        side = "server"
        "#
    );

    fn build() -> FakeHost {
        FakeHost::new()
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.give", 0x1_4030_0000)
            .symbol("loot.roll", ROLL)
            .symbol("loot.roll_one", ROLL_ONE)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .constant("loot.txn_context", 0x10)
            .constant("loot.txn_entity", 0x18)
            .constant("loot.request_node", 0x08)
            .constant("loot.request_labels", 0x0c)
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .type_hash("destroy_on_loot", 3)
            .type_hash("boss", 4)
            .type_hash("interaction_lock", 5)
            .constant("interaction_lock.item", 0)
    }

    fn linked(host: FakeHost) -> (Result<(), String>, Vec<u64>) {
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Drops)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let destroyed = Drops::instance().unwrap().destroyed.clone();
        let targets = loader.hooks().iter().map(|hook| hook.target).collect();
        (destroyed, targets)
    }

    #[test]
    fn the_single_roll_links_where_the_build_names_its_slot() {
        let (destroyed, targets) = linked(
            build()
                .type_size("loot_slot", 16)
                .constant("loot_slot.item", 0)
                .constant("loot_slot.count", 4),
        );
        assert_eq!(destroyed, Ok(()));
        assert_eq!(targets, [ROLL, ROLL_ONE]);
    }

    #[test]
    fn a_slot_too_small_for_its_fields_keeps_the_single_roll_vanilla() {
        let (destroyed, targets) = linked(
            build()
                .type_size("loot_slot", 5)
                .constant("loot_slot.item", 0)
                .constant("loot_slot.count", 4),
        );
        assert!(destroyed.unwrap_err().contains("loot_slot"));
        assert_eq!(targets, [ROLL]);
    }

    #[test]
    fn a_build_without_the_slot_keeps_the_single_roll_vanilla() {
        let (destroyed, targets) = linked(build());
        assert!(destroyed.unwrap_err().contains("loot_slot"));
        assert_eq!(targets, [ROLL]);
    }
}
