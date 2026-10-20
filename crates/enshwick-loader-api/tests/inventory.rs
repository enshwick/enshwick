//! A mod's inventory reads and requests against the fake loader: slots
//! through virtual links, roots, durability and item data; a take-all
//! request and its outcome; a give that fills meters; a refused batch
//! answered after a step.

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;
    use std::sync::{Mutex, OnceLock};

    use enshwick_loader_api::abi::{
        InventoryOp, InventoryOpResult, InventoryResult, ItemData, Status, TickData,
    };
    use enshwick_loader_api::ecs::Entity;
    use enshwick_loader_api::events::Ticked;
    use enshwick_loader_api::inventory::{
        Bar, Batch, BatchError, Category, Done, Fill, Inventories, Op, Outcome, SlotRef,
    };
    use enshwick_loader_api::test::FakeHost;

    const SETUP: u32 = 0xc24d_f82a;
    const INVENTORY: u32 = 0xb195_28a9;
    const DURABILITY: u32 = 0x43c1_2f80;
    const OWNER: u32 = 0xd736_1788;
    const FUEL: u32 = 0x1555_7d63;
    const CORPSE: u32 = 40;
    const PLAYER: u32 = 2;
    const SWORD: u32 = 0x5e01;
    const COIN: u32 = 0x0c01;

    static INVENTORIES: OnceLock<Inventories> = OnceLock::new();
    static SKIPPED: AtomicU32 = AtomicU32::new(44);
    static OUTCOMES: Mutex<Vec<Outcome>> = Mutex::new(Vec::new());

    struct Looter;

    impl enshwick_loader_api::Mod for Looter {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "looter", side: SERVER)
        }

        fn init(
            load: &mut enshwick_loader_api::Load<'_>,
        ) -> Result<Self, enshwick_loader_api::Off> {
            let _first = INVENTORIES.set(load.inventories()?);
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Looter);

    /// Leaked bytes for a component.
    fn data(bytes: Vec<u8>) -> *const core::ffi::c_void {
        Box::leak(bytes.into_boxed_slice()).as_ptr().cast()
    }

    fn setup(links: &[(u32, u8)]) -> *const core::ffi::c_void {
        let mut bytes = vec![0u8; 0x54];
        for (i, &(entity, category)) in links.iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&entity.to_le_bytes());
            bytes[0x40 + i] = category;
        }
        bytes[0x53] = 1;
        data(bytes)
    }

    fn stacks(stacks: &[(u32, u32, u32)]) -> *const core::ffi::c_void {
        let mut bytes = vec![0u8; 0x60];
        for (i, &(item, count, pide)) in stacks.iter().enumerate() {
            let at = i * 12;
            bytes[at..at + 4].copy_from_slice(&item.to_le_bytes());
            bytes[at + 4..at + 8].copy_from_slice(&count.to_le_bytes());
            bytes[at + 8..at + 12].copy_from_slice(&pide.to_le_bytes());
        }
        data(bytes)
    }

    fn world() -> FakeHost {
        FakeHost::new()
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.skip_entity", SKIPPED.as_ptr().addr() as u64)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
            .type_hash("keen::ecs::InventorySetup", SETUP)
            .type_hash("keen::ecs::Inventory", INVENTORY)
            .type_hash("keen::ecs::NetworkDurability", DURABILITY)
            .type_hash("keen::ecs::NetworkFuel", FUEL)
            .type_hash("keen::ecs::OwnerRelationship", OWNER)
            .field_offset("keen::ecs::InventorySetup", "linksEntities", 0)
            .field_offset("keen::ecs::InventorySetup", "linksCategories", 0x40)
            .field_offset("keen::ecs::InventorySetup", "isInitialized", 0x53)
            .field_offset("keen::ItemStack", "data", 4)
            .field_offset("keen::ecs::NetworkDurability", "durabilityMax", 2)
            .field_offset("keen::ecs::NetworkFuel", "fuelMax", 2)
            .type_size("keen::ItemStack", 12)
            // The corpse: a generic sub-inventory, the skipped global one and
            // a virtual link to another inventory with one more.
            .component(CORPSE, SETUP, setup(&[(41, 4), (44, 4), (42, 5)]))
            .component(42, SETUP, setup(&[(43, 4)]))
            .component(41, INVENTORY, stacks(&[(SWORD, 1, 900), (COIN, 7, 0)]))
            .component(43, INVENTORY, stacks(&[(0, 0, 0), (COIN, 5, 0)]))
            .component(44, INVENTORY, stacks(&[(COIN, 99, 0)]))
            .component(41, OWNER, data(CORPSE.to_le_bytes().to_vec()))
            .component(900, DURABILITY, data(vec![40, 0, 160, 0]))
            .component(901, FUEL, data(vec![2, 0, 3, 0]))
            .item(SWORD, 1, ItemData::HAS_PIDE)
    }

    fn record(outcome: &Outcome) {
        OUTCOMES.lock().unwrap().push(outcome.clone());
    }

    #[test]
    fn a_mod_reads_a_corpse_and_takes_everything() {
        let loader = world().install();
        // SAFETY: the export of `declare_mod!` above.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let inventories = INVENTORIES.get().unwrap();

        let reader = inventories.read();
        let slots = reader.slots(Entity(CORPSE));
        assert_eq!(slots.len(), 16);
        assert_eq!(slots[0].at, SlotRef::new(Entity(41), 0));
        assert_eq!(
            (slots[0].item, slots[0].pide, slots[0].category),
            (SWORD, Entity(900), Category::GENERIC)
        );
        assert_eq!(slots[9].at, SlotRef::new(Entity(43), 1));
        assert_eq!(slots.iter().filter(|s| !s.is_empty()).count(), 3);
        assert_eq!(reader.count(Entity(CORPSE), COIN), 12);
        assert_eq!(
            reader.slot(SlotRef::new(Entity(41), 1)).map(|s| s.count),
            Some(7)
        );
        assert_eq!(
            reader.root(SlotRef::new(Entity(41), 1)),
            Some(Entity(CORPSE))
        );
        assert_eq!(reader.root(SlotRef::new(Entity(43), 1)), None);
        let durability = reader.durability(Entity(900)).unwrap();
        assert_eq!((durability.current, durability.max), (40, 160));
        assert_eq!(reader.meter(Entity(900)), None);
        assert_eq!(reader.meter(Entity(901)), Some(Bar { current: 2, max: 3 }));
        assert!(
            inventories
                .item_info(SWORD)
                .is_some_and(|i| i.has_pide() && i.max_stack == 1)
        );
        assert!(inventories.item_info(COIN).is_none());

        let ticket = inventories.move_all(Entity(CORPSE), Entity(PLAYER), record);
        let requests = loader.inventory_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!((requests[0].flags, requests[0].actor), (0, PLAYER));
        let op = requests[0].ops[0];
        assert_eq!(
            (op.kind, op.target, op.other),
            (InventoryOp::MOVE_ALL, 40, 2)
        );
        assert!(!ticket.is_done());
        let ran = InventoryOpResult {
            error: 0,
            remaining: 0,
        };
        assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[ran]));
        assert!(ticket.outcome().is_some_and(|o| o.is_committed()));

        let filled = Op::Give {
            to: Entity(PLAYER),
            item: SWORD,
            count: 1,
            fill: Some(Fill::of(2, 3)),
        };
        let _ticket = inventories.submit(&Batch::new().with(filled), record);
        let op = loader.inventory_requests()[0].ops[0];
        assert_eq!(
            (op.kind, op.flags),
            (InventoryOp::GIVE, InventoryOp::FILLED)
        );
        let fill = f32::from_bits(u32::try_from(op.other).unwrap());
        assert!((fill * 3.0 - 2.0).abs() < 1e-3);
        assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[ran]));

        let refused = inventories.submit(&Batch::new(), record);
        assert!(loader.inventory_requests().is_empty());
        assert!(!refused.is_done());
        let tick = TickData {
            header: enshwick_loader_api::abi::Header::of::<TickData>(),
            step: 1,
        };
        assert_eq!(loader.raise::<Ticked>(&tick), 1);
        assert_eq!(
            refused.outcome().map(|o| o.done),
            Some(Done::Refused(BatchError::Empty))
        );
        let outcomes = OUTCOMES.lock().unwrap();
        assert_eq!(outcomes.len(), 3);
        assert_eq!(outcomes[0].done, Done::Committed);
    }
}
