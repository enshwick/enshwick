//! Inventories in the fake: the requests the mod submits and their
//! outcomes, items, stats, damage, effects, loot and survivors.

use enshwick_core::__fake::ids;
use enshwick_core::Entity;
use enshwick_core::abi::Header;
use enshwick_core::abi::InventoryOp;
use enshwick_core::abi::InventoryOpResult;
use enshwick_core::abi::InventoryResult;

use crate::test::{FakeHost, Installed, table};

pub(crate) mod items;
pub(crate) mod stats;
pub(crate) mod survivors;

impl FakeHost {
    /// Inventories on the server: the component types, stack size and
    /// offsets [`Inventories`](enshwick_core::inventory::Inventories) reads slots,
    /// durability and meters by. The ECS reads and the skipped entity stay
    /// the test's own symbols.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().inventory();
    /// let stack = host.size(facts::ITEM_STACK.id())?;
    /// assert!(host.offset(facts::ITEM_STACK_PIDE.id())? + 4 <= stack);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn inventory(self) -> Self {
        self.preset(
            &[
                ids::INVENTORY_SETUP,
                ids::INVENTORY,
                ids::DURABILITY,
                ids::FUEL,
                ids::OWNER,
            ],
            &[(ids::ITEM_STACK, 12)],
            &[
                (ids::INVENTORY_SETUP_LINKS, 0),
                (ids::INVENTORY_SETUP_CATEGORIES, 0x40),
                (ids::INVENTORY_SETUP_INITIALIZED, 0x53),
                (ids::ITEM_STACK_COUNT, 4),
                (ids::ITEM_STACK_PIDE, 8),
                (ids::DURABILITY_MAX, 2),
                (ids::FUEL_MAX, 2),
            ],
        )
    }

    /// [`inventory`](Self::inventory) with the symbols and constants
    /// [`Load::inventories`](enshwick_core::Load::inventories) resolves, so a test
    /// submits and finishes inventory requests.
    ///
    /// # Examples
    ///
    /// ```
    /// let host = enshwick::test::FakeHost::new().inventory_requests();
    /// # let _ = host;
    /// ```
    #[must_use]
    pub fn inventory_requests(self) -> Self {
        /// The entity the game's inventory system skips, which the loader
        /// reads and writes through `inventory.skip_entity`.
        static SKIPPED: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(44);
        self.symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.skip_entity", SKIPPED.as_ptr().addr() as u64)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .inventory()
    }

    /// The events [`Effects`](enshwick_core::effects::Effects) raises for an NPC
    /// command, a threat and a ping; the toggle event stays off.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().effect_events();
    /// let position = host.offset(facts::UI_PING_EVENT_POSITION.id())?;
    /// assert!(position + 24 <= host.size(facts::UI_PING_EVENT.id())?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn effect_events(self) -> Self {
        self.preset(
            &[
                ids::NPC_COMMAND_EVENT,
                ids::ADD_THREAT_TARGET_EVENT,
                ids::UI_PING_EVENT,
            ],
            &[
                (ids::NPC_COMMAND_EVENT, 72),
                (ids::ADD_THREAT_TARGET_EVENT, 24),
                (ids::UI_PING_EVENT, 40),
            ],
            &[
                (ids::NPC_COMMAND_EVENT_TYPE, 8),
                (ids::NPC_COMMAND_EVENT_NPC_ID, 12),
                (ids::NPC_COMMAND_EVENT_TARGET_TRANSFORM, 16),
                (ids::ADD_THREAT_TARGET_EVENT_ATTACKER_ID, 8),
                (ids::ADD_THREAT_TARGET_EVENT_TARGET_ID, 12),
                (ids::ADD_THREAT_TARGET_EVENT_THREAT_VALUE, 16),
                (ids::UI_PING_EVENT_SENDER_ENTITY_ID, 8),
                (ids::UI_PING_POSITION, 16),
            ],
        )
    }

    /// The components loot from destroyed things checks: destroyed on loot,
    /// boss and the interaction lock with its item. The loot slot stays the
    /// test's own, as the single roll links only where a build names it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().loot();
    /// assert!(host.size(facts::LOOT_SLOT.id()).is_err());
    /// ```
    #[must_use]
    pub fn loot(self) -> Self {
        self.preset(
            &[ids::DESTROY_ON_LOOT, ids::BOSS, ids::INTERACTION_LOCK],
            &[],
            &[(ids::INTERACTION_LOCK_ITEM, 0)],
        )
    }

    /// Item meters: the meter's own component and, through
    /// [`FakeHost::templates`], the water pouch's template every meter
    /// template is copied from.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    ///
    /// let host = enshwick::test::FakeHost::new().meters();
    /// assert!(host.offset("fuel_setup.start")? + 4 <= host.size("fuel_setup")?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn meters(self) -> Self {
        self.preset(
            &[ids::FUEL_SETUP, ids::TEMPLATE, ids::TEMPLATE_COLLECTION],
            &[(ids::FUEL_SETUP, 40)],
            &[
                (ids::FUEL_SETUP_MAX, 32),
                (ids::FUEL_SETUP_START, 36),
                (ids::TEMPLATE_COMPONENTS, 0x10),
                (ids::TEMPLATE_COLLECTION_TEMPLATES, 0),
            ],
        )
        .templates()
    }

    /// The markers `ContainerKinds` tells holders apart by: characters, NPCs, animals, drops, loot chests and
    /// plants. Placed things carry [`FakeHost::world`]'s used item, and every
    /// holder [`FakeHost::inventory`]'s setup.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().inventory().container_kinds();
    /// let setup = host.hash(facts::INVENTORY_SETUP.id())?;
    /// let loot = host.hash(facts::LOOT_FILL.id())?;
    /// let loader = host
    ///     .placed(setup, enshwick::Entity(9), [0.0; 3])
    ///     .component(enshwick::Entity(9), loot, &[])
    ///     .install();
    /// # let _ = loader;
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn container_kinds(self) -> Self {
        self.preset(
            &[
                ids::PLAYER_INPUT,
                ids::NPC,
                ids::ANIMAL,
                ids::DESTROY_ON_LOOT,
                ids::LOOT_FILL,
                ids::RESOURCE_NODE,
            ],
            &[],
            &[],
        )
    }

    /// Plays a game build without inventory transactions: `inventory_open`
    /// answers [`Status::MISSING`](enshwick_core::abi::Status::MISSING).
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().without_inventories().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn without_inventories(mut self) -> Self {
        self.no_inventories = true;
        self
    }

    /// Plays a game build without the item-use site: `item_filter`, behind
    /// `Load::on_item_action` and `Load::on_item`, answers
    /// [`Status::MISSING`](enshwick_core::abi::Status::MISSING).
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().without_item_filters().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn without_item_filters(mut self) -> Self {
        self.no_item_filters = true;
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The inventory requests the mod submitted that are not finished yet, in
    /// order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.inventory_requests().is_empty());
    /// ```
    #[must_use]
    pub fn inventory_requests(&self) -> Vec<SubmittedRequest> {
        table::with(Vec::new(), |s| {
            s.inventory_requests
                .iter()
                .map(|r| r.request.clone())
                .collect()
        })
    }

    /// Finishes the oldest open inventory request as the game would:
    /// `outcome` is an `InventoryResult` constant and `ops` a result per
    /// operation. The mod's callback runs here, as on the game thread.
    /// Returns false when no request is open.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::InventoryResult;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.finish_inventory(InventoryResult::COMMITTED, &[]));
    /// ```
    #[must_use = "false means no request was open"]
    pub fn finish_inventory(&self, outcome: u32, ops: &[InventoryOpResult]) -> bool {
        let open = table::with(None, |s| {
            (!s.inventory_requests.is_empty()).then(|| s.inventory_requests.remove(0))
        });
        let Some(open) = open else {
            return false;
        };
        let result = InventoryResult {
            header: Header::of::<InventoryResult>(),
            outcome,
            op_count: u32::try_from(ops.len()).unwrap_or(0),
            ops: ops.as_ptr(),
        };
        // SAFETY: the mod's callback with its pointer, handed back once; the
        // result lives for the call.
        open.done
            .run(|done, user| unsafe { done(&raw const result, user) });
        true
    }

    /// Runs the mod's damage filters on `check` as the loader does before
    /// the game applies damage; returns what they left.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{DamageCheck, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let fall = DamageCheck {
    ///     header: Header::of::<DamageCheck>(),
    ///     cause: DamageCheck::FALL_DAMAGE,
    ///     target: enshwick::Entity(2),
    ///     source: enshwick::Entity(0),
    ///     attacker: enshwick::Entity(0),
    ///     amounts: [0.0; enshwick::abi::DAMAGE_TYPES],
    ///     total: 40.0,
    /// };
    /// assert_eq!(loader.damage(fall), fall, "no filters, no change");
    /// ```
    #[must_use]
    pub fn damage(
        &self,
        mut check: enshwick_core::abi::DamageCheck,
    ) -> enshwick_core::abi::DamageCheck {
        // One filter at a time, outside the lock, without copying the list:
        // a bench of a filter counts its own allocations alone.
        let mut at = 0;
        while let Some(filter) = table::with(None, |s| s.damage_filters.get(at).copied()) {
            // SAFETY: the filter the mod registered, with its pointer.
            filter.run(|callback, user| unsafe { callback(&raw mut check, user) });
            at += 1;
        }
        check
    }

    /// Puts `item` on `entity`, as worn equipment the stats count.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::inventory::ItemInstance;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.wear(Entity(2), ItemInstance { pide: Entity(900), item: enshwick::ItemId::new(0x5e01) });
    /// ```
    pub fn wear(&self, entity: Entity, item: enshwick_core::inventory::ItemInstance) {
        table::with((), |s| s.stats.worn.entry(entity.0).or_default().push(item));
    }

    /// Works out the stat values of every watched entity and runs the mod's
    /// [`on_stats_changed`](enshwick_core::Load::on_stats_changed) handlers for each
    /// that changed since the last call, as the loader does after a step;
    /// how many changes it raised.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.settle_stats(), 0);
    /// ```
    #[must_use]
    pub fn settle_stats(&self) -> usize {
        stats::settle()
    }

    /// The modifiers the mod keeps on entities: `(entity, key hash, stat
    /// index, operation, value, flags, duration in ms)`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.stat_modifiers().is_empty());
    /// ```
    #[must_use]
    pub fn stat_modifiers(&self) -> Vec<(u32, u32, u32, u8, f32, u32, u64)> {
        stats::modifiers()
    }
}

/// An inventory request the mod submitted, as the loader received it.
///
/// # Examples
///
/// See [`Installed::inventory_requests`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SubmittedRequest {
    /// The operations, in order.
    pub ops: Vec<InventoryOp>,
    /// `InventoryRequest` flags.
    pub flags: u32,
    /// The player credited, 0 for the game's choice.
    pub actor: u32,
}
