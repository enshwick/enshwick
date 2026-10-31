//! A mod's inventory reads and requests against the fake loader: slots
//! through virtual links, roots, durability and item data; a take-all
//! request and its outcome; a give that fills meters; a refused batch
//! answered after a step.

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;
    use std::sync::{Mutex, OnceLock};

    use enshwick::Entity;
    use enshwick::abi::{
        InventoryOp, InventoryOpResult, InventoryResult, ItemData, Status, TickData,
    };
    use enshwick::content::EngineFacts;
    use enshwick::events::Ticked;
    use enshwick::inventory::{
        Bar, Batch, BatchError, Done, Fill, Inventories, InventoryCategory, Op, Outcome, SlotRef,
    };
    use enshwick::test::FakeHost;
    use enshwick::test::facts::{self, Fact};

    const CORPSE: u32 = 40;
    const PLAYER: u32 = 2;
    const SWORD: u32 = 0x5e01;
    const COIN: u32 = 0x0c01;

    static INVENTORIES: OnceLock<Inventories> = OnceLock::new();
    static SKIPPED: AtomicU32 = AtomicU32::new(44);
    static OUTCOMES: Mutex<Vec<Outcome>> = Mutex::new(Vec::new());

    struct Looter;

    impl enshwick::Mod for Looter {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let _first = INVENTORIES.set(load.inventories()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Looter,
        r#"
        id = "you.looter"
        side = "server"
        "#
    );

    /// Links a setup holds and stacks a sub-inventory holds, as the reads
    /// walk them.
    const LINKS: usize = 16;
    const SLOTS: usize = 8;

    fn at(host: &FakeHost, fact: Fact) -> usize {
        host.offset(fact.id()).unwrap()
    }

    fn hash(host: &FakeHost, fact: Fact) -> u32 {
        host.hash(fact.id()).unwrap()
    }

    fn put(bytes: &mut [u8], at: usize, value: &[u8]) {
        bytes[at..at + value.len()].copy_from_slice(value);
    }

    /// An inventory setup with `links` to sub-inventories and their
    /// categories, initialized.
    fn setup(host: &FakeHost, links: &[(u32, u8)]) -> Vec<u8> {
        let (entities, categories) = (
            at(host, facts::INVENTORY_SETUP_LINKS),
            at(host, facts::INVENTORY_SETUP_CATEGORIES),
        );
        let initialized = at(host, facts::INVENTORY_SETUP_INITIALIZED);
        let mut bytes = vec![0u8; (categories + LINKS).max(initialized + 1)];
        for (i, &(entity, category)) in links.iter().enumerate() {
            put(&mut bytes, entities + i * 4, &entity.to_le_bytes());
            bytes[categories + i] = category;
        }
        bytes[initialized] = 1;
        bytes
    }

    /// A sub-inventory's stacks of `(item, count, pide)`, the rest empty.
    fn stacks(host: &FakeHost, stacks: &[(u32, u32, u32)]) -> Vec<u8> {
        let size = host.size(facts::ITEM_STACK.id()).unwrap();
        let (count_at, pide_at) = (
            at(host, facts::ITEM_STACK_COUNT),
            at(host, facts::ITEM_STACK_PIDE),
        );
        let mut bytes = vec![0u8; size * SLOTS];
        for (i, &(item, count, pide)) in stacks.iter().enumerate() {
            let stack = i * size;
            put(&mut bytes, stack, &item.to_le_bytes());
            put(&mut bytes, stack + count_at, &count.to_le_bytes());
            put(&mut bytes, stack + pide_at, &pide.to_le_bytes());
        }
        bytes
    }

    /// A bar of `current` out of `max`, its maximum where `max_at` says.
    fn bar(host: &FakeHost, max_at: Fact, current: u16, max: u16) -> Vec<u8> {
        let max_offset = at(host, max_at);
        let mut bytes = vec![0u8; max_offset + 2];
        put(&mut bytes, 0, &current.to_le_bytes());
        put(&mut bytes, max_offset, &max.to_le_bytes());
        bytes
    }

    fn world() -> FakeHost {
        let host = FakeHost::new()
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.skip_entity", SKIPPED.as_ptr().addr() as u64)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .world()
            .inventory();
        let (setup_type, inventory) = (
            hash(&host, facts::INVENTORY_SETUP),
            hash(&host, facts::INVENTORY),
        );
        let (durability, fuel, owner) = (
            hash(&host, facts::DURABILITY),
            hash(&host, facts::FUEL),
            hash(&host, facts::OWNER),
        );
        // The corpse: a generic sub-inventory, the skipped global one and a
        // virtual link to another inventory with one more.
        let corpse = setup(&host, &[(41, 4), (44, 4), (42, 5)]);
        let linked = setup(&host, &[(43, 4)]);
        let first = stacks(&host, &[(SWORD, 1, 900), (COIN, 7, 0)]);
        let second = stacks(&host, &[(0, 0, 0), (COIN, 5, 0)]);
        let global = stacks(&host, &[(COIN, 99, 0)]);
        let worn = bar(&host, facts::DURABILITY_MAX, 40, 160);
        let lamp = bar(&host, facts::FUEL_MAX, 2, 3);
        host.component(Entity(CORPSE), setup_type, &corpse)
            .component(Entity(42), setup_type, &linked)
            .component(Entity(41), inventory, &first)
            .component(Entity(43), inventory, &second)
            .component(Entity(44), inventory, &global)
            .component(Entity(41), owner, &CORPSE.to_le_bytes())
            .component(Entity(900), durability, &worn)
            .component(Entity(901), fuel, &lamp)
            .item(enshwick::ItemId::new(SWORD), 1, ItemData::HAS_PIDE)
    }

    fn record(outcome: &Outcome) {
        OUTCOMES.lock().unwrap().push(outcome.clone());
    }

    #[test]
    fn a_mod_reads_a_corpse_and_takes_everything() {
        let loader = world().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Looter)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let inventories = INVENTORIES.get().unwrap();

        let reader = inventories.read();
        let slots = reader.slots(Entity(CORPSE));
        assert_eq!(slots.len(), 16);
        assert_eq!(slots[0].at, SlotRef::new(Entity(41), 0));
        assert_eq!(
            (slots[0].item, slots[0].pide, slots[0].category),
            (
                enshwick::ItemId::new(SWORD),
                Some(Entity(900)),
                InventoryCategory::GENERIC
            )
        );
        assert_eq!(slots[9].at, SlotRef::new(Entity(43), 1));
        assert_eq!(slots.iter().filter(|s| !s.is_empty()).count(), 3);
        assert_eq!(
            reader.count(Entity(CORPSE), enshwick::ItemId::new(COIN)),
            12
        );
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
                .item_info(enshwick::ItemId::new(SWORD))
                .is_some_and(|i| i.has_pide() && i.max_stack_size == 1)
        );
        assert!(inventories.item_info(enshwick::ItemId::new(COIN)).is_none());

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
            item: enshwick::ItemId::new(SWORD),
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
        let tick = TickData::new(1);
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
