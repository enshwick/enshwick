use core::ffi::c_void;
use core::ops::ControlFlow;

use enshwick_base::load::StatusError;

use crate::abi::{InventoryOp, InventoryRequest};
use crate::ecs::{ComponentType, Components};
use crate::engine_ids as ids;
use crate::raw::read_at;
use crate::schedule::Scheduler;
use crate::{BaseLoad, Host, Off, WorldError};
use crate::{Entity, ItemId};

use super::links::{SetupLayout, walk};
use super::ops::{Batch, Op};
use super::results::{Done, OpErrors, Outcome, Pending, RunOpError, Ticket, deliver};
use super::slots::{Bar, Fill, InventoryCategory, ItemInfo, Slot, SlotRef};
use super::{MAX_DEPTH, MAX_OWNERS, SLOTS};

/// Reflected layouts the reads use.
#[derive(Clone, Copy, Debug)]
pub(super) struct Layout {
    setup: ComponentType,
    inventory: ComponentType,
    durability: ComponentType,
    fuel: ComponentType,
    owner: ComponentType,
    links: SetupLayout,
    stack: usize,
    stack_count: usize,
    stack_pide: usize,
    durability_max: usize,
    fuel_max: usize,
    /// The game's entity that inventory walks skip.
    skip: usize,
}

/// Inventory reads and transactions on the server; get it from
/// [`Load::inventories`](../struct.Load.html#method.inventories). Copy it anywhere.
///
/// # Examples
///
/// ```
/// use enshwick::Entity;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let inventories = load.inventories()?;
///     let (corpse, player) = (Entity::from_raw(40).ok_or(enshwick::Off::new("no entity"))?, Entity::from_raw(2).ok_or(enshwick::Off::new("no entity"))?);
///     load.scheduler()?.next_tick(move || {
///         if let Ok(slots) = inventories.read().slots(corpse) {
///             let full = slots.iter().filter(|s| !s.is_empty()).count();
///             enshwick::info!("{full} stacks to loot");
///         }
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
    components: Components,
    scheduler: Scheduler,
    layout: Layout,
    errors: OpErrors,
}

#[crate::script(reads)]
impl Inventories {
    /// The stack in `slot`, as `Reader::slot` reads it.
    #[script(name = "slot")]
    pub(crate) fn slot_script(&self, slot: SlotRef) -> Result<Option<Slot>, WorldError> {
        self.read().slot(slot)
    }

    /// The inventory at the root of `slot`'s, as `Reader::root` reads it.
    #[script(name = "root")]
    pub(crate) fn root_script(&self, slot: SlotRef) -> Result<Option<Entity>, WorldError> {
        self.read().root(slot)
    }

    /// The durability of the item `pide`, as `Reader::durability` reads it.
    #[script(name = "durability")]
    pub(crate) fn durability_script(&self, pide: Entity) -> Result<Option<Bar>, WorldError> {
        self.read().durability(pide)
    }

    /// The meter of the item `pide`, as `Reader::meter` reads it.
    #[script(name = "meter")]
    pub(crate) fn meter_script(&self, pide: Entity) -> Result<Option<Bar>, WorldError> {
        self.read().meter(pide)
    }
}

#[crate::script(stock)]
impl Inventories {
    pub(crate) fn resolve(load: &mut BaseLoad) -> Result<Self, Off> {
        let host = load.host();
        let scheduler = load.scheduler()?;
        let status = (host.api().request_inventory)(host.handle());
        if !status.is_ok() {
            return Err(Off::new(format!(
                "inventories: unavailable on this game build (status {})",
                status.0
            )));
        }
        let components = Components::resolve(load)?;
        let offset = |id: &str| {
            load.constant(id)
                .and_then(|v| usize::try_from(v).map_err(|_| Off::new(format!("offset {id}"))))
        };
        let layout = Layout {
            setup: ComponentType::resolve(load, ids::INVENTORY_SETUP)?,
            inventory: ComponentType::resolve(load, ids::INVENTORY)?,
            durability: ComponentType::resolve(load, ids::DURABILITY)?,
            fuel: ComponentType::resolve(load, ids::FUEL)?,
            owner: ComponentType::resolve(load, ids::OWNER)?,
            links: SetupLayout {
                links: offset(ids::INVENTORY_SETUP_LINKS)?,
                categories: offset(ids::INVENTORY_SETUP_CATEGORIES)?,
                initialized: offset(ids::INVENTORY_SETUP_INITIALIZED)?,
            },
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
            components,
            scheduler,
            layout,
            errors: OpErrors::resolve(load, RunOpError::IDS),
        })
    }

    /// Reads on the server's game thread, between steps: from a game event
    /// or a [`Scheduler`] task. Elsewhere they fail with
    /// [`WorldError::WrongThread`] or, where no world runs,
    /// [`WorldError::Unavailable`].
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

    /// Reads from inside a hooked ECS system, through its context; its reads
    /// never fail.
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
    /// unsafe fn held(inventories: &enshwick::inventory::Inventories, ctx: *mut u64, player: enshwick::Entity) -> Result<u32, enshwick::WorldError> {
    ///     // SAFETY: called from a system's hook with its live context, used at once.
    ///     unsafe { inventories.read_in(ctx) }.count(player, enshwick::ItemId::new(0x0c01))
    /// }
    /// ```
    #[must_use]
    pub const unsafe fn read_in(&self, ctx: *mut u64) -> Reader<'_> {
        Reader {
            inventories: self,
            ctx,
        }
    }

    /// The static data of `item`; none for an unknown item, or while no
    /// world runs. Any thread.
    ///
    /// # Examples
    ///
    /// See [`ItemInfo`].
    #[must_use]
    #[script]
    pub fn item_info(&self, item: ItemId) -> Option<ItemInfo> {
        let out = enshwick_content::__internal::item_data(&self.host, item)?;
        Some(ItemInfo {
            id: out.id,
            max_stack_size: out.max_stack,
            flags: out.flags,
            category: out.category,
            placed_entity_template: (out.placed_entity != crate::content::Guid::NIL)
                .then_some(out.placed_entity),
        })
    }

    /// Shows `character`'s player the game's pickup message for `count` of
    /// `item`, with nothing entering the inventory: a reward that is no
    /// item, or a give made elsewhere. The server raises the game's pickup
    /// event, which every reader of it hears, the item filters but; a count
    /// above 65535 shows 65535, the game's most. On the server's game
    /// thread between steps (a game event or a
    /// [`Scheduler`](crate::schedule::Scheduler) task).
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`](crate::WorldError::Refused) for item 0, count
    /// 0 or an entity that is no player in game;
    /// [`WorldError::WrongThread`](crate::WorldError::WrongThread) off the
    /// server's game thread; [`WorldError::Unavailable`](crate::WorldError::Unavailable)
    /// outside a server or on a build without the event.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::inventory::Inventories;
    ///
    /// fn reward(inventories: &Inventories, player: Entity) -> Result<(), enshwick::WorldError> {
    ///     // The quest pays in reputation; the message shows a token.
    ///     inventories.show_pickup(player, enshwick::ItemId::new(0x5e01), 3)
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[script]
    pub fn show_pickup(
        &self,
        character: Entity,
        item: ItemId,
        count: u32,
    ) -> Result<(), WorldError> {
        let host = self.host;
        let status = (host.api().show_pickup)(host.handle(), character, item, count);
        enshwick_world::__internal::world_status(status)
    }

    /// Submits `batch` from any thread. `done` runs on the game thread after
    /// a later step with the outcome; in Rust the returned [`Ticket`] holds
    /// it as well, and Luau returns nothing. A batch that breaks a limit is
    /// refused without reaching the game. If the mod is turned off first,
    /// `done` never runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::inventory::{Batch, Inventories, Op};
    ///
    /// fn trade(inventories: &Inventories, player: Entity) {
    ///     let batch = Batch::new()
    ///         .with(Op::Take { from: player, item: enshwick::ItemId::new(0x0c01), count: 5 })
    ///         .with(Op::Give { to: player, item: enshwick::ItemId::new(0x5e01), count: 1, fill: None });
    ///     let _ticket = inventories.submit(&batch, |outcome| {
    ///         enshwick::info!("trade: {:?}", outcome.done);
    ///     });
    /// }
    /// ```
    #[script]
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
        let request = enshwick_abi::headed!(InventoryRequest {
            flags: batch.request_flags(),
            actor: batch.actor_entity(),
            ops: ops.as_ptr(),
            op_count: u32::try_from(ops.len()).unwrap_or(0),
            reserved: 0,
        });
        let user = Box::into_raw(Box::new(pending));
        let status = (self.host.api().inventory_submit)(
            self.host.handle(),
            &raw const request,
            Some(deliver),
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

    /// Every slot of the inventory `root`, as a [reader](Reader::slots) reads it.
    ///
    /// # Errors
    ///
    /// As [`Inventories::read`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn first_item(inventories: &enshwick::inventory::Inventories, player: enshwick::Entity) -> Result<Option<enshwick::ItemId>, enshwick::WorldError> {
    ///     Ok(inventories.slots(player)?.into_iter().find(|s| !s.is_empty()).map(|s| s.item))
    /// }
    /// ```
    #[script]
    pub fn slots(&self, root: Entity) -> Result<Vec<Slot>, WorldError> {
        self.read().slots(root)
    }

    /// How many items `item` the inventory `root` holds, as a [reader](Reader::count)
    /// reads it.
    ///
    /// # Errors
    ///
    /// As [`Inventories::read`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn coins(inventories: &enshwick::inventory::Inventories, player: enshwick::Entity) -> Result<u32, enshwick::WorldError> {
    ///     inventories.count(player, enshwick::ItemId::new(0x0c01))
    /// }
    /// ```
    #[script]
    pub fn count(&self, root: Entity, item: ItemId) -> Result<u32, WorldError> {
        self.read().count(root, item)
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
    /// fn reward(inventories: &enshwick::inventory::Inventories, player: enshwick::Entity) {
    ///     inventories.give(player, enshwick::ItemId::new(0x0c01), 10, |outcome| {
    ///         if !outcome.is_committed() {
    ///             enshwick::warn!("no room for the reward");
    ///         }
    ///     });
    /// }
    /// ```
    #[script]
    pub fn give(
        &self,
        to: Entity,
        item: ItemId,
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

    /// Gives as many of `count` new items `item` as the inventory `to` has
    /// room for, with the game's pickup message, and tells `done` how many
    /// were not given and the request's [`Outcome`]: a committed one left
    /// them for want of room, any other gave none (`count`), for the reason
    /// its [`Done`] names. What becomes of the rest is the caller's to decide.
    ///
    /// # Examples
    ///
    /// ```
    /// fn reward(inventories: &enshwick::inventory::Inventories, player: enshwick::Entity) {
    ///     inventories.give_what_fits(player, enshwick::ItemId::new(0x0c01), 10, |left, outcome| {
    ///         if !outcome.is_committed() {
    ///             enshwick::warn!("reward not given: {:?}", outcome.done);
    ///         } else if left > 0 {
    ///             enshwick::warn!("no room for {left} of the reward");
    ///         }
    ///     });
    /// }
    /// ```
    #[script]
    pub fn give_what_fits(
        &self,
        to: Entity,
        item: ItemId,
        count: u32,
        done: impl FnOnce(u32, &Outcome) + Send + 'static,
    ) -> Ticket {
        let give = Op::Give {
            to,
            item,
            count,
            fill: None,
        };
        self.submit(&Batch::new().with(give).allow_partial(), move |outcome| {
            done(outcome.left_of(count), outcome);
        })
    }

    /// Takes `count` items `item` from the inventory `from`, all or nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// fn charge(inventories: &enshwick::inventory::Inventories, player: enshwick::Entity) {
    ///     inventories.take(player, enshwick::ItemId::new(0x0c01), 5, |outcome| enshwick::info!("paid: {}", outcome.is_committed()));
    /// }
    /// ```
    #[script]
    pub fn take(
        &self,
        from: Entity,
        item: ItemId,
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
    #[script]
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
    /// fn hand_over(inventories: &enshwick::inventory::Inventories, slot: enshwick::inventory::SlotRef, player: enshwick::Entity) {
    ///     inventories.move_slot(slot, player, None, |_| {});
    /// }
    /// ```
    #[script]
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
    /// action bar, equipment and locked slots stay; [a move in a batch](Op::MoveAll)
    /// can include them.
    ///
    /// # Examples
    ///
    /// See [`Inventories`].
    #[script]
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
    /// # Errors
    ///
    /// As [`Inventories::read`], without calling `done`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::events::PlayerJoined;
    /// use enshwick::inventory::Fill;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     const STAFF: enshwick::ItemId = enshwick::ItemId::new(0x5e01);
    ///     let inventories = load.inventories()?;
    ///     load.on::<PlayerJoined>(move |joined| {
    ///         let player = joined.entity;
    ///         if let Err(error) = inventories.renew(player, &[STAFF], Fill::FULL, |_| {}) {
    ///             enshwick::warn!("staff not renewed: {error}");
    ///         }
    ///     })
    /// }
    /// ```
    #[script]
    pub fn renew(
        &self,
        root: Entity,
        items: &[ItemId],
        fill: Fill,
        done: impl FnOnce(&Outcome) + Send + 'static,
    ) -> Result<Option<Ticket>, WorldError> {
        let mut batch = Batch::new();
        for slot in self.read().slots(root)? {
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
        Ok((!batch.ops().is_empty()).then(|| self.submit(&batch, done)))
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
    /// `entity`'s component of type `ty`, or why the loader cannot read now,
    /// which holds for every read of the same moment: a walk checks its first
    /// read here and takes the rest through [`Self::component`].
    fn checked(&self, entity: Entity, ty: ComponentType) -> Result<Option<*const u8>, WorldError> {
        if entity.is_none() {
            return Ok(None);
        }
        if self.ctx.is_null() {
            enshwick_base::__internal::component_status(ty, self.inventories.host, entity)
                .map(|data| data.map(<*const c_void>::cast))
                .map_err(WorldError::of)
        } else {
            // SAFETY: a non-null `ctx` comes only from `read_in`, whose caller
            // guarantees the hooked system's live context, used on its thread
            // during the hook, which is the contract of `component`.
            Ok(
                unsafe { self.inventories.components.component(self.ctx, entity, ty) }
                    .map(<*mut u8>::cast_const),
            )
        }
    }

    fn component(&self, entity: Entity, ty: ComponentType) -> Option<*const u8> {
        self.checked(entity, ty).ok().flatten()
    }

    /// Every slot of the inventory `root`, its virtual links followed, empty
    /// slots included, in the game's order; empty when `root` has no
    /// inventory.
    ///
    /// # Errors
    ///
    /// As [`Inventories::read`].
    ///
    /// # Examples
    ///
    /// See [`Inventories`].
    pub fn slots(&self, root: Entity) -> Result<Vec<Slot>, WorldError> {
        let layout = &self.inventories.layout;
        if self.checked(root, layout.setup)?.is_none() {
            return Ok(Vec::new());
        }
        let skip = if layout.skip == 0 {
            0
        } else {
            // SAFETY: `skip` is not 0 in this branch, and is the build table's
            // `inventory.skip_entity`, a u32 entity id in the game's image,
            // mapped for the process.
            unsafe { read_at::<u32>(core::ptr::with_exposed_provenance(layout.skip), 0) }
        };
        let mut out = Vec::new();
        let mut setup = |entity| self.component(entity, layout.setup);
        let mut visit = |link, category| {
            if category != InventoryCategory::VIRTUAL
                && let Some(slots) = self.component(link, layout.inventory)
            {
                out.extend((0..SLOTS).map(|index| {
                    // SAFETY: `component` returned the link's live Inventory,
                    // which holds `SLOTS` stacks, and `index < SLOTS`.
                    unsafe { self.stack(slots, link, index, category) }
                }));
            }
            ControlFlow::<()>::Continue(())
        };
        // SAFETY: `component` returns only the entity's live InventorySetup,
        // laid out as the reflected `layout.links` says, read while no
        // inventory changes (the reader's contract).
        let _walked = unsafe {
            walk(
                root,
                MAX_DEPTH,
                Entity::from_engine(skip),
                &layout.links,
                &mut setup,
                &mut visit,
            )
        };
        Ok(out)
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

    /// What `slot` holds, with [`InventoryCategory::INVALID`]; `None` for a
    /// slot of no inventory.
    ///
    /// # Errors
    ///
    /// As [`Inventories::read`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn holds(reader: &enshwick::inventory::Reader<'_>, slot: enshwick::inventory::SlotRef) -> Result<bool, enshwick::WorldError> {
    ///     Ok(reader.slot(slot)?.is_some_and(|s| !s.is_empty()))
    /// }
    /// ```
    pub fn slot(&self, slot: SlotRef) -> Result<Option<Slot>, WorldError> {
        let Some(slots) = self.checked(slot.inventory(), self.inventories.layout.inventory)? else {
            return Ok(None);
        };
        if slot.index() >= SLOTS {
            return Ok(None);
        }
        // SAFETY: `component` returned the inventory's live Inventory, and
        // `index < SLOTS` was checked above.
        Ok(Some(unsafe {
            self.stack(
                slots,
                slot.inventory(),
                slot.index(),
                InventoryCategory::INVALID,
            )
        }))
    }

    /// How many items `item` the inventory `root` holds.
    ///
    /// # Errors
    ///
    /// As [`Inventories::read`].
    ///
    /// # Examples
    ///
    /// See [`Inventories::read_in`].
    pub fn count(&self, root: Entity, item: ItemId) -> Result<u32, WorldError> {
        Ok(self
            .slots(root)?
            .iter()
            .filter(|s| s.item == item && !s.is_empty())
            .fold(0, |sum, s| sum.saturating_add(s.count)))
    }

    /// The inventory a slot belongs to at the root, the chest or player,
    /// found by climbing `OwnerRelationship` from its sub-inventory; `None`
    /// when the sub-inventory has no owner.
    ///
    /// # Errors
    ///
    /// As [`Inventories::read`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn owner(reader: &enshwick::inventory::Reader<'_>, slot: enshwick::inventory::SlotRef) -> Result<(), enshwick::WorldError> {
    ///     if let Some(root) = reader.root(slot)? {
    ///         enshwick::info!("slot of {}", root);
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn root(&self, slot: SlotRef) -> Result<Option<Entity>, WorldError> {
        let mut entity = slot.inventory();
        let mut climbed = false;
        for _ in 0..MAX_OWNERS {
            let Some(owner) = self.checked(entity, self.inventories.layout.owner)? else {
                break;
            };
            // SAFETY: `component` returned the entity's live
            // OwnerRelationship, which is the owner's u32 entity id alone.
            let owner = Entity::from_engine(unsafe { read_at(owner, 0) });
            if owner.is_none() || owner == entity {
                break;
            }
            entity = owner;
            climbed = true;
        }
        Ok(climbed.then_some(entity))
    }

    /// The durability of an item's own entity, its PIDE, as clients see it;
    /// `None` for an item without one.
    ///
    /// # Errors
    ///
    /// As [`Inventories::read`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn worn(reader: &enshwick::inventory::Reader<'_>, slot: &enshwick::inventory::Slot) -> Result<bool, enshwick::WorldError> {
    ///     let Some(pide) = slot.pide else { return Ok(false) };
    ///     Ok(reader.durability(pide)?.is_some_and(|d| d.current * 4 < d.max))
    /// }
    /// ```
    pub fn durability(&self, pide: Entity) -> Result<Option<Bar>, WorldError> {
        let layout = &self.inventories.layout;
        self.bar(pide, layout.durability, layout.durability_max)
    }

    /// The meter of an item's own entity (see
    /// [`FuelSetup`](crate::content::items::FuelSetup)), as clients see it; `None`
    /// for an item without one.
    ///
    /// # Errors
    ///
    /// As [`Inventories::read`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn charges(reader: &enshwick::inventory::Reader<'_>, slot: &enshwick::inventory::Slot) -> Result<u16, enshwick::WorldError> {
    ///     let Some(pide) = slot.pide else { return Ok(0) };
    ///     Ok(reader.meter(pide)?.map_or(0, |m| m.current))
    /// }
    /// ```
    pub fn meter(&self, pide: Entity) -> Result<Option<Bar>, WorldError> {
        let layout = &self.inventories.layout;
        self.bar(pide, layout.fuel, layout.fuel_max)
    }

    fn bar(&self, pide: Entity, ty: ComponentType, max: usize) -> Result<Option<Bar>, WorldError> {
        let Some(data) = self.checked(pide, ty)? else {
            return Ok(None);
        };
        Ok(Some(Bar {
            // SAFETY: `component` returned the item's live NetworkDurability
            // or NetworkFuel, which starts with its u16 current value.
            current: unsafe { read_at(data, 0) },
            // SAFETY: `data` is the item's live NetworkDurability or
            // NetworkFuel, whose u16 maximum sits at the reflected offset
            // `max`.
            max: unsafe { read_at(data, max) },
        }))
    }
}
