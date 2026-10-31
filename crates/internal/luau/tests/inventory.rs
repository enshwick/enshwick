//! A Luau mod reads and changes inventories through `enshwick.inventory`,
//! the same functions a Rust mod calls.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicU32;

    use enshwick::abi::{InventoryOp, InventoryOpResult, InventoryResult, Status};
    use enshwick::events::WorldSaved;
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Scripts};

    const SETUP: u32 = 0xc24d_f82a;
    const INVENTORY: u32 = 0xb195_28a9;
    const CHEST: u32 = 40;
    const COIN: u32 = 0x0c01;

    static SKIPPED: AtomicU32 = AtomicU32::new(0);
    static CASE: Mutex<Option<Scripts>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let scripts = CASE.lock().unwrap().take().unwrap();
            let _runtime =
                enshwick_luau::start(load, "you.luau", scripts, None, Limits::default())?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Luau,
        r#"
        id = "you.luau"
        side = "server"
        "#
    );

    /// A chest with one sub-inventory of two stacks.
    fn world() -> FakeHost {
        let mut setup = vec![0u8; 0x54];
        setup[..4].copy_from_slice(&41u32.to_le_bytes());
        setup[0x40] = 4;
        setup[0x53] = 1;
        let mut stacks = vec![0u8; 0x60];
        for (i, (item, count)) in [(COIN, 7u32), (0x5e01, 1)].into_iter().enumerate() {
            stacks[i * 12..i * 12 + 4].copy_from_slice(&item.to_le_bytes());
            stacks[i * 12 + 4..i * 12 + 8].copy_from_slice(&count.to_le_bytes());
        }
        FakeHost::new()
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.skip_entity", SKIPPED.as_ptr().addr() as u64)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .type_hash("inventory_setup", SETUP)
            .type_hash("inventory", INVENTORY)
            .type_hash("durability", 3)
            .type_hash("fuel", 4)
            .type_hash("owner", 5)
            .constant("inventory_setup.links", 0)
            .constant("inventory_setup.categories", 0x40)
            .constant("inventory_setup.initialized", 0x53)
            .constant("item_stack.count", 4)
            .constant("item_stack.pide", 8)
            .constant("durability.max", 2)
            .constant("fuel.max", 2)
            .type_size("item_stack", 12)
            .component(enshwick::Entity(CHEST), SETUP, &setup)
            .component(enshwick::Entity(41), INVENTORY, &stacks)
    }

    fn run(main: &str) -> Installed {
        *CASE.lock().unwrap() = Some(Scripts::new().with("main.luau", main));
        let loader = world().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn logged(loader: &Installed, text: &str) -> bool {
        loader.logs().iter().any(|(_, line)| line.contains(text))
    }

    #[test]
    fn a_script_reads_an_inventory_s_slots_and_counts() {
        let loader = run(r#"enshwick.on("WorldSaved", function()
                local slots = enshwick.inventory.slots(40)
                print(`slots {#slots} first {slots[1].item} x{slots[1].count} at {slots[1].at}`)
                print(`coins {enshwick.inventory.count(40, 3073)}`)
            end)"#);
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(logged(&loader, "coins 7"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "first 3073 x7 at 41"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_gives_items_and_hears_how_it_ended() {
        let loader = run(r#"enshwick.on("WorldSaved", function()
                enshwick.inventory.give(2, 3073, 5, function(outcome)
                    print(`given {outcome.done} ops {#outcome.ops} left {outcome.ops[1].remaining}`)
                end)
            end)"#);
        let _handled = loader.raise::<WorldSaved>(&());
        let requests = loader.inventory_requests();
        assert_eq!(requests.len(), 1);
        let op = requests[0].ops[0];
        assert_eq!((op.kind, op.target), (InventoryOp::GIVE, 2));
        let ran = InventoryOpResult {
            error: 0,
            remaining: 0,
        };
        assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[ran]));
        assert!(
            logged(&loader, "given Committed ops 1 left 0"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_hears_a_rolled_back_take_and_its_error_by_name() {
        let loader = run(r#"enshwick.on("WorldSaved", function()
                enshwick.inventory.take(2, 3073, 50, function(outcome)
                    print(`taken {outcome.done} error {outcome.ops[1].error} short {outcome.ops[1].remaining}`)
                end)
            end)"#);
        let _handled = loader.raise::<WorldSaved>(&());
        let requests = loader.inventory_requests();
        assert_eq!(requests[0].ops[0].kind, InventoryOp::TAKE);
        let failed = InventoryOpResult {
            error: 0x27,
            remaining: 43,
        };
        assert!(loader.finish_inventory(InventoryResult::ROLLED_BACK, &[failed]));
        assert!(
            logged(&loader, "taken RolledBack error Other short 43"),
            "{:?}",
            loader.logs()
        );
    }
}
