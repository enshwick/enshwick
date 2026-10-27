use core::ffi::c_void;

use crate::abi::{Header, InventoryOp, InventoryRequest, ItemData};
use crate::ecs::{Components, Entity};
use crate::engine_ids as ids;
use crate::raw::read_at;
use crate::schedule::Scheduler;
use crate::world::{ComponentType, World};
use crate::{Host, Load, Off};

use super::ops::{Batch, Op};
use super::results::{Done, OpError, OpErrors, Outcome, Pending, Ticket, deliver};
use super::slots::{Bar, Fill, InventoryCategory, ItemInfo, Slot, SlotRef};
use super::{LINKS, MAX_DEPTH, MAX_OWNERS, SLOTS};

/// Reflected layouts the reads use.
#[derive(Clone, Copy, Debug)]
pub(super) struct Layout {
    setup: ComponentType,
    inventory: ComponentType,
    durability: ComponentType,
    fuel: ComponentType,
    owner: ComponentType,
    links: usize,
    categories: usize,
    initialized: usize,
    stack: usize,
    stack_count: usize,
    stack_pide: usize,
    durability_max: usize,
    fuel_max: usize,
    /// The game's entity that inventory walks skip.
    skip: usize,
}

/// Inventory reads and transactions on the server; get it from
/// [`Load::inventories`]. Copy it anywhere.
///
/// # Examples
///
/// ```
/// use enshwick::ecs::Entity;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let inventories = load.inventories()?;
///     let (corpse, player) = (Entity(40), Entity(2));
///     load.scheduler()?.next_tick(move || {
///         let full = inventories.read().slots(corpse).iter().filter(|s| !s.is_empty()).count();
///         enshwick::info!("{full} stacks to loot");
///         inventories.move_all(corpse, player, |outcome| {
///             enshwick::info!("looted: {}", outcome.is_committed());
///         });
///     });
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Inventories {
    host: Host,
    world: World,
    components: Components,
    scheduler: Scheduler,
    layout: Layout,
    errors: OpErrors,
}

impl Inventories {
    pub(crate) fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        let host = load.host();
        let scheduler = load.scheduler()?;
        let status = (host.api().inventory_open)(host.handle());
        if !status.is_ok() {
            return Err(Off::new(format!(
                "inventories: unavailable on this game build (status {})",
                status.0
            )));
        }
        let world = load.world()?;
        let components = Components::resolve(load)?;
        let offset = |id: &str| {
            load.constant(id)
                .and_then(|v| usize::try_from(v).map_err(|_| Off::new(format!("offset {id}"))))
        };
        let layout = Layout {
            setup: world.component_type(load, ids::INVENTORY_SETUP)?,
            inventory: world.component_type(load, ids::INVENTORY)?,
            durability: world.component_type(load, ids::DURABILITY)?,
            fuel: world.component_type(load, ids::FUEL)?,
            owner: world.component_type(load, ids::OWNER)?,
            links: offset(ids::INVENTORY_SETUP_LINKS)?,
            categories: offset(ids::INVENTORY_SETUP_CATEGORIES)?,
            initialized: offset(ids::INVENTORY_SETUP_INITIALIZED)?,
            stack: usize::try_from(load.type_size(ids::ITEM_STACK)?)
                .map_err(|_| Off::new(ids::ITEM_STACK))?,
            stack_count: offset(ids::ITEM_STACK_COUNT)?,
            stack_pide: offset(ids::ITEM_STACK_PIDE)?,
            durability_max: offset(ids::DURABILITY_MAX)?,
            fuel_max: offset(ids::FUEL_MAX)?,
            skip: usize::try_from(load.symbol("inventory.skip_entity")?)
                .map_err(|_| Off::new("inventory.skip_entity"))?,
        };
        Ok(Self {
            host,
            world,
            components,
            scheduler,
            layout,
            errors: OpErrors::resolve(load, OpError::IDS),
        })
    }

    /// Reads on the server's game thread, between steps: from a game event
    /// or a [`Scheduler`] task. Elsewhere they return nothing.
    ///
    /// # Examples
    ///
    /// See [`Inventories`].
    #[must_use]
    pub const fn read(&self) -> Reader<'_> {
        Reader {
            inventories: self,
            ctx: core::ptr::null_mut(),
        }
    }

    /// Reads from inside a hooked ECS system, through its context.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live query context the hooked system was called with.
    /// - The reader and every copy of it are used on that system's thread
    ///   during the hook only.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn held(inventories: &enshwick::inventory::Inventories, ctx: *mut u64, player: enshwick::ecs::Entity) -> u32 {
    ///     // SAFETY: called from a system's hook with its live context, used at once.
    ///     unsafe { inventories.read_in(ctx) }.count(player, 0x0c01)
    /// }
    /// ```
    #[must_use]
    pub const unsafe fn read_in(&self, ctx: *mut u64) -> Reader<'_> {
        Reader {
            inventories: self,
            ctx,
        }
    }

    /// The static data of `item`; `None` for an unknown item, or while no
    /// world runs. Any thread.
    ///
    /// # Examples
    ///
    /// See [`ItemInfo`].
    #[must_use]
    pub fn item_info(&self, item: u32) -> Option<ItemInfo> {
        let mut out = ItemData {
            header: Header::of::<ItemData>(),
            id: 0,
            max_stack: 0,
            category: 0,
            rarity: 0,
            flags: 0,
            placed_entity: [0; 16],
        };
        let status = (self.host.api().item_info)(self.host.handle(), item, &raw mut out);
        status.is_ok().then_some(ItemInfo {
            id: out.id,
            max_stack_size: out.max_stack,
            flags: out.flags,
            category: out.category,
        })
    }

    /// Submits `batch` from any thread. `done` runs on the game thread after
    /// a later step with the outcome, which the returned [`Ticket`] also
    /// holds. A batch that breaks a limit is refused without reaching the
    /// game. If the mod is turned off first, `done` never runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ecs::Entity;
    /// use enshwick::inventory::{Batch, Inventories, Op};
    ///
    /// fn trade(inventories: &Inventories, player: Entity) {
    ///     let batch = Batch::new()
    ///         .with(Op::Take { from: player, item: 0x0c01, count: 5 })
    ///         .with(Op::Give { to: player, item: 0x5e01, count: 1, fill: None });
    ///     let _ticket = inventories.submit(&batch, |outcome| {
    ///         enshwick::info!("trade: {:?}", outcome.done);
    ///     });
    /// }
    /// ```
    pub fn submit(&self, batch: &Batch, done: impl FnOnce(&Outcome) + Send + 'static) -> Ticket {
        let ticket = Ticket::default();
        let pending = Pending {
            ticket: ticket.clone(),
            done: Box::new(done),
            errors: self.errors,
        };
        if let Err(error) = batch.check() {
            self.later(pending, Done::Refused(error));
            return ticket;
        }
        let ops: Vec<InventoryOp> = batch.ops.iter().map(Op::raw).collect();
        let request = InventoryRequest {
            header: Header::of::<InventoryRequest>(),
            flags: batch.request_flags(),
            actor: batch.actor_entity().0,
            ops: ops.as_ptr(),
            op_count: u32::try_from(ops.len()).unwrap_or(0),
            reserved: 0,
        };
        let user = Box::into_raw(Box::new(pending));
        let status = (self.host.api().inventory_submit)(
            self.host.handle(),
            &raw const request,
            deliver,
            user.cast(),
        );
        if !status.is_ok() {
            // SAFETY: `user` came from this function's `Box::into_raw`, and the loader
            // refused it, so it keeps no copy and this is the only owner.
            let pending = unsafe { Box::from_raw(user) };
            self.later(*pending, Done::Unavailable);
        }
        ticket
    }

    /// Completes `pending` with `done` on the game thread after the next step.
    fn later(&self, pending: Pending, done: Done) {
        self.scheduler
            .next_tick(move || pending.complete(&Outcome::not_run(done)));
    }

    /// Gives `count` new items `item` to the inventory `to`, all or nothing,
    /// with the game's pickup message.
    ///
    /// # Examples
    ///
    /// ```
    /// fn reward(inventories: &enshwick::inventory::Inventories, player: enshwick::ecs::Entity) {
    ///     inventories.give(player, 0x0c01, 10, |outcome| {
    ///         if !outcome.is_committed() {
    ///             enshwick::warn!("no room for the reward");
    ///         }
    ///     });
    /// }
    /// ```
    pub fn give(
        &self,
        to: Entity,
        item: u32,
        count: u32,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        self.submit(
            &Batch::new().with(Op::Give {
                to,
                item,
                count,
                fill: None,
            }),
            done,
        )
    }

    /// Takes `count` items `item` from the inventory `from`, all or nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// fn charge(inventories: &enshwick::inventory::Inventories, player: enshwick::ecs::Entity) {
    ///     inventories.take(player, 0x0c01, 5, |outcome| enshwick::info!("paid: {}", outcome.is_committed()));
    /// }
    /// ```
    pub fn take(
        &self,
        from: Entity,
        item: u32,
        count: u32,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        self.submit(&Batch::new().with(Op::Take { from, item, count }), done)
    }

    /// Destroys the stack in `slot`.
    ///
    /// # Examples
    ///
    /// ```
    /// fn burn(inventories: &enshwick::inventory::Inventories, slot: enshwick::inventory::SlotRef) {
    ///     inventories.destroy_slot(slot, |_| {});
    /// }
    /// ```
    pub fn destroy_slot(
        &self,
        slot: SlotRef,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        self.submit(&Batch::new().with(Op::DestroySlot(slot)), done)
    }

    /// Moves the stack in `slot`, or `amount` of it, into the inventory `to`;
    /// what does not fit stays in the source.
    ///
    /// # Examples
    ///
    /// ```
    /// fn hand_over(inventories: &enshwick::inventory::Inventories, slot: enshwick::inventory::SlotRef, player: enshwick::ecs::Entity) {
    ///     inventories.move_slot(slot, player, None, |_| {});
    /// }
    /// ```
    pub fn move_slot(
        &self,
        slot: SlotRef,
        to: Entity,
        amount: Option<u16>,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        self.submit(&Batch::new().with(Op::MoveSlot { slot, to, amount }), done)
    }

    /// Moves every stack of the inventory `from` into `to`, as the game's
    /// "take all" does; what does not fit stays in the source. The source's
    /// action bar, equipment and locked slots stay; use [`Op::MoveAll`] in a
    /// [`Batch`] to include them.
    ///
    /// # Examples
    ///
    /// See [`Inventories`].
    pub fn move_all(
        &self,
        from: Entity,
        to: Entity,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Ticket {
        let all = Op::MoveAll {
            from,
            to,
            include_special: false,
        };
        self.submit(&Batch::new().with(all), done)
    }

    /// Renews every stack of `items` in the inventory `root` that has no
    /// entity of its own, each with its meter at `fill`, in one
    /// transaction ([`Op::Renew`]): items from before their mod gave them a
    /// meter get one at once. Reads on the game thread, as
    /// [`Inventories::read`]; `None` when nothing needs it, without calling
    /// `done`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::events::PlayerJoined;
    /// use enshwick::inventory::Fill;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     const STAFF: u32 = 0x5e01;
    ///     let inventories = load.inventories()?;
    ///     load.on::<PlayerJoined>(move |joined| {
    ///         let player = enshwick::ecs::Entity(joined.entity);
    ///         let _renewed = inventories.renew(player, &[STAFF], Fill::FULL, |_| {});
    ///     })
    /// }
    /// ```
    pub fn renew(
        &self,
        root: Entity,
        items: &[u32],
        fill: Fill,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Option<Ticket> {
        let mut batch = Batch::new();
        for slot in self.read().slots(root) {
            if !slot.is_empty() && slot.pide.is_none() && items.contains(&slot.item) {
                batch.push(Op::Renew {
                    slot: slot.at,
                    root,
                    item: slot.item,
                    count: slot.count,
                    fill,
                });
            }
        }
        (!batch.ops().is_empty()).then(|| self.submit(&batch, done))
    }
}

/// Synchronous inventory reads; get one from [`Inventories::read`] or
/// [`Inventories::read_in`].
///
/// # Examples
///
/// See [`Inventories`].
#[derive(Clone, Copy, Debug)]
pub struct Reader<'a> {
    inventories: &'a Inventories,
    /// A system's context, or null for the game thread.
    ctx: *mut u64,
}

impl Reader<'_> {
    fn component(&self, entity: Entity, ty: ComponentType) -> Option<*const u8> {
        if entity.is_none() {
            return None;
        }
        if self.ctx.is_null() {
            self.inventories
                .world
                .component(entity, ty)
                .map(<*const c_void>::cast)
        } else {
            // SAFETY: a non-null `ctx` comes only from `read_in`, whose caller
            // guarantees the hooked system's live context, used on its thread
            // during the hook, which is the contract of `component`.
            unsafe {
                self.inventories
                    .components
                    .component(self.ctx, entity, ty.hash())
            }
            .map(<*mut u8>::cast_const)
        }
    }

    /// Every slot of the inventory `root`, its virtual links followed, empty
    /// slots included, in the game's order; empty when `root` has no
    /// inventory.
    ///
    /// # Examples
    ///
    /// See [`Inventories`].
    #[must_use]
    pub fn slots(&self, root: Entity) -> Vec<Slot> {
        let layout = &self.inventories.layout;
        let skip = if layout.skip == 0 {
            0
        } else {
            // SAFETY: `skip` is not 0 in this branch, and is the build table's
            // `inventory.skip_entity`, a u32 entity id in the game's image,
            // mapped for the process.
            unsafe { read_at::<u32>(core::ptr::with_exposed_provenance(layout.skip), 0) }
        };
        let mut out = Vec::new();
        self.walk(root, 0, skip, &mut out);
        out
    }

    fn walk(&self, root: Entity, depth: u32, skip: u32, out: &mut Vec<Slot>) {
        let layout = &self.inventories.layout;
        let Some(setup) = self.component(root, layout.setup) else {
            return;
        };
        // SAFETY: `component` returned the entity's live InventorySetup, whose
        // `isInitialized` byte sits at the reflected offset `initialized`.
        if unsafe { read_at::<u8>(setup, layout.initialized) } == 0 {
            return;
        }
        for i in 0..LINKS {
            // SAFETY: `setup` is the live InventorySetup, whose `LINKS` u32
            // `linksEntities` start at the reflected offset `links`, and
            // `i < LINKS`.
            let link: u32 = unsafe { read_at(setup, layout.links + i * size_of::<u32>()) };
            if link == 0 {
                break;
            }
            if link == skip {
                continue;
            }
            // SAFETY: `setup` is the live InventorySetup, whose `LINKS`
            // `linksCategories` bytes start at the reflected offset
            // `categories`, and `i < LINKS`.
            let category = InventoryCategory(unsafe { read_at(setup, layout.categories + i) });
            if category == InventoryCategory::VIRTUAL {
                if depth < MAX_DEPTH {
                    self.walk(Entity(link), depth + 1, skip, out);
                }
                continue;
            }
            if let Some(slots) = self.component(Entity(link), layout.inventory) {
                out.extend((0..SLOTS).map(|index| {
                    // SAFETY: `component` returned the link's live Inventory,
                    // which holds `SLOTS` stacks, and `index < SLOTS`.
                    unsafe { self.stack(slots, Entity(link), index, category) }
                }));
            }
        }
    }

    /// The stack `index` of the `Inventory` at `slots`.
    ///
    /// # Safety
    ///
    /// - `slots` points at a live `Inventory`, read while it is valid with no
    ///   concurrent writer.
    /// - `index` is below `SLOTS`.
    unsafe fn stack(
        &self,
        slots: *const u8,
        inventory: Entity,
        index: u8,
        category: InventoryCategory,
    ) -> Slot {
        let layout = &self.inventories.layout;
        let at = usize::from(index) * layout.stack;
        Slot {
            at: SlotRef::new(inventory, index),
            category,
            // SAFETY: the caller guarantees a live `Inventory` and an index
            // below `SLOTS`; stack `index` starts at `at`, `index` times the
            // reflected `ItemStack` size, with its u32 item id.
            item: unsafe { read_at(slots, at) },
            // SAFETY: the caller guarantees a live `Inventory` and an index
            // below `SLOTS`; the stack's u32 count sits at the reflected
            // offset of `ItemStack.data`.
            count: unsafe { read_at(slots, at + layout.stack_count) },
            // SAFETY: the caller guarantees a live `Inventory` and an index
            // below `SLOTS`; the stack's u32 item entity follows its count in
            // `ItemStack.data`.
            pide: Entity::from_raw(unsafe { read_at(slots, at + layout.stack_pide) }),
        }
    }

    /// What `slot` holds, with [`InventoryCategory::INVALID`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn holds(reader: &enshwick::inventory::Reader<'_>, slot: enshwick::inventory::SlotRef) -> bool {
    ///     reader.slot(slot).is_some_and(|s| !s.is_empty())
    /// }
    /// ```
    #[must_use]
    pub fn slot(&self, slot: SlotRef) -> Option<Slot> {
        if slot.index() >= SLOTS {
            return None;
        }
        let slots = self.component(slot.inventory(), self.inventories.layout.inventory)?;
        // SAFETY: `component` returned the inventory's live Inventory, and
        // `index < SLOTS` was checked above.
        Some(unsafe {
            self.stack(
                slots,
                slot.inventory(),
                slot.index(),
                InventoryCategory::INVALID,
            )
        })
    }

    /// How many items `item` the inventory `root` holds.
    ///
    /// # Examples
    ///
    /// See [`Inventories::read_in`].
    #[must_use]
    pub fn count(&self, root: Entity, item: u32) -> u32 {
        self.slots(root)
            .iter()
            .filter(|s| s.item == item && !s.is_empty())
            .fold(0, |sum, s| sum.saturating_add(s.count))
    }

    /// The inventory a slot belongs to at the root, the chest or player,
    /// found by climbing `OwnerRelationship` from its sub-inventory; `None`
    /// when the sub-inventory has no owner.
    ///
    /// # Examples
    ///
    /// ```
    /// fn owner(reader: &enshwick::inventory::Reader<'_>, slot: enshwick::inventory::SlotRef) {
    ///     if let Some(root) = reader.root(slot) {
    ///         enshwick::info!("slot of {}", root.0);
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn root(&self, slot: SlotRef) -> Option<Entity> {
        let mut entity = slot.inventory();
        let mut climbed = false;
        for _ in 0..MAX_OWNERS {
            let Some(owner) = self.component(entity, self.inventories.layout.owner) else {
                break;
            };
            // SAFETY: `component` returned the entity's live
            // OwnerRelationship, which is the owner's u32 entity id alone.
            let owner = Entity(unsafe { read_at(owner, 0) });
            if owner.is_none() || owner == entity {
                break;
            }
            entity = owner;
            climbed = true;
        }
        climbed.then_some(entity)
    }

    /// The durability of an item's own entity, its PIDE, as clients see it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn worn(reader: &enshwick::inventory::Reader<'_>, slot: &enshwick::inventory::Slot) -> bool {
    ///     slot.pide.and_then(|pide| reader.durability(pide)).is_some_and(|d| d.current * 4 < d.max)
    /// }
    /// ```
    #[must_use]
    pub fn durability(&self, pide: Entity) -> Option<Bar> {
        let layout = &self.inventories.layout;
        self.bar(pide, layout.durability, layout.durability_max)
    }

    /// The meter of an item's own entity (see
    /// [`FuelSetup`](crate::content::items::FuelSetup)), as clients see it; `None`
    /// for an item without one.
    ///
    /// # Examples
    ///
    /// ```
    /// fn charges(reader: &enshwick::inventory::Reader<'_>, slot: &enshwick::inventory::Slot) -> u16 {
    ///     slot.pide.and_then(|pide| reader.meter(pide)).map_or(0, |m| m.current)
    /// }
    /// ```
    #[must_use]
    pub fn meter(&self, pide: Entity) -> Option<Bar> {
        let layout = &self.inventories.layout;
        self.bar(pide, layout.fuel, layout.fuel_max)
    }

    fn bar(&self, pide: Entity, ty: ComponentType, max: usize) -> Option<Bar> {
        let data = self.component(pide, ty)?;
        Some(Bar {
            // SAFETY: `component` returned the item's live NetworkDurability
            // or NetworkFuel, which starts with its u16 current value.
            current: unsafe { read_at(data, 0) },
            // SAFETY: `data` is the item's live NetworkDurability or
            // NetworkFuel, whose u16 maximum sits at the reflected offset
            // `max`.
            max: unsafe { read_at(data, max) },
        })
    }
}
