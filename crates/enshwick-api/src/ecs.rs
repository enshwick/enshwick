//! Access to the engine's ECS from inside a hooked system.
//!
//! A system's update function receives a query context. [`Ecs`] walks that
//! query with the engine's own iterator, reads the entity of the current row
//! and marks written components changed so replication follows them. Every
//! address and offset comes from the loader for the running build.

use crate::abi::Side;
use crate::{Host, Load, Off};

type IteratorInit = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64);
type IteratorNext = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64) -> u64;
type MarkChanged = unsafe extern "system" fn(
    tracker: u64,
    stamp_a: u64,
    stamp_b: u64,
    entity: u64,
    kind: u16,
    flags: u8,
);

/// An entity id, as the engine numbers entities.
///
/// # Examples
///
/// ```
/// let entity = enshwick_api::ecs::Entity(1210);
/// assert!(!entity.is_none());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity(pub u32);

impl Entity {
    /// No entity.
    pub const NONE: Self = Self(0);

    /// Whether this is [`Entity::NONE`].
    ///
    /// # Examples
    ///
    /// See [`Entity`].
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

/// The engine's ECS functions and layout on the running build.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick_api::Load<'_>) -> Result<enshwick_api::ecs::Ecs, enshwick_api::Off> {
///     enshwick_api::ecs::Ecs::resolve(load)
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Ecs {
    init: IteratorInit,
    next: IteratorNext,
    mark: MarkChanged,
    context_row: usize,
    entity_records: usize,
    entity_record_size: usize,
}

/// Turns a resolved address into a function pointer of type `F`.
///
/// # Safety
///
/// `address` is a function of type `F`.
unsafe fn function<F: Copy>(address: u64) -> F {
    let address = usize::try_from(address).unwrap_or(0);
    // SAFETY: the caller guarantees the type; `F` is a function pointer the size
    // of `usize`.
    unsafe { core::mem::transmute_copy(&address) }
}

/// Reads a `T` at `offset` bytes past `base`.
///
/// # Safety
///
/// A readable `T` lies there.
unsafe fn read_at<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

impl Ecs {
    /// Resolves the ECS functions and layout for the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader lacks any of them for this build.
    ///
    /// # Examples
    ///
    /// See [`Ecs`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let size = |name| {
            load.constant(name)
                .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
        };
        let (init, next, mark) = (
            load.symbol("ecs.iterator_init")?,
            load.symbol("ecs.iterator_next")?,
            load.symbol("ecs.mark_changed")?,
        );
        // SAFETY: the loader pins this id to the iterator's `init`.
        let init = unsafe { function::<IteratorInit>(init) };
        // SAFETY: the loader pins this id to the iterator's `next`.
        let next = unsafe { function::<IteratorNext>(next) };
        // SAFETY: the loader pins this id to `mark_changed`.
        let mark = unsafe { function::<MarkChanged>(mark) };
        Ok(Self {
            init,
            next,
            mark,
            context_row: size("ecs.context_row")?,
            entity_records: size("ecs.descriptor_entity_records")?,
            entity_record_size: size("ecs.entity_record_size")?,
        })
    }

    /// Walks the system's query from its first row; `N` is the number of
    /// iterator words the system declares, its context word included.
    ///
    /// # Safety
    ///
    /// `ctx` is the live query context of the hooked system and `N` matches its
    /// iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// /// Counts the rows of a system's query.
    /// unsafe fn rows(ecs: &enshwick_api::ecs::Ecs, ctx: *mut u64) -> usize {
    ///     // SAFETY: called from the system's hook with its context.
    ///     let mut query = unsafe { ecs.query::<5>(ctx) };
    ///     let mut count = 0;
    ///     while query.next().is_some() {
    ///         count += 1;
    ///     }
    ///     count
    /// }
    /// ```
    #[must_use]
    pub unsafe fn query<const N: usize>(&self, ctx: *mut u64) -> Query<'_, N> {
        let mut words = [0u64; N];
        // SAFETY: the caller passes a live context and a matching size.
        unsafe { (self.init)(ctx, words.as_mut_ptr(), (N * 8) as u64) };
        Query {
            ecs: self,
            ctx,
            words,
        }
    }

    /// The 1-based row the query is on, 0 before the first.
    ///
    /// # Safety
    ///
    /// `ctx` is a live query context.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn rewind(ecs: &enshwick_api::ecs::Ecs, ctx: *mut u64) {
    ///     // SAFETY: called from the system's hook with its context.
    ///     unsafe { ecs.set_row(ctx, 0) };
    /// }
    /// ```
    #[must_use]
    pub unsafe fn row(&self, ctx: *mut u64) -> u32 {
        // SAFETY: the row counter lies inside a live context.
        unsafe { read_at(ctx.cast(), self.context_row) }
    }

    /// Moves the query's row counter, so a pass can start again from the
    /// first row and leave the counter as it found it.
    ///
    /// # Safety
    ///
    /// `ctx` is a live query context.
    ///
    /// # Examples
    ///
    /// See [`Ecs::row`].
    pub unsafe fn set_row(&self, ctx: *mut u64, row: u32) {
        // SAFETY: the row counter lies inside a live context.
        unsafe {
            ctx.cast::<u8>()
                .wrapping_add(self.context_row)
                .cast::<u32>()
                .write_unaligned(row);
        }
    }

    /// The entity of the query's current row, [`Entity::NONE`] before the
    /// first.
    ///
    /// # Safety
    ///
    /// `ctx` is a live query context.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn current(ecs: &enshwick_api::ecs::Ecs, ctx: *mut u64) -> enshwick_api::ecs::Entity {
    ///     // SAFETY: called from the system's hook with its context.
    ///     unsafe { ecs.entity(ctx) }
    /// }
    /// ```
    #[must_use]
    pub unsafe fn entity(&self, ctx: *mut u64) -> Entity {
        // SAFETY: forwarded from the caller.
        let row = unsafe { self.row(ctx) };
        if row == 0 {
            return Entity::NONE;
        }
        // SAFETY: the first word of a context is its query descriptor.
        let descriptor: *const u8 = unsafe { read_at(ctx.cast(), 0) };
        // SAFETY: the descriptor holds a pointer to the per-row entity records.
        let records: *const u8 = unsafe { read_at(descriptor, self.entity_records) };
        let offset = (row as usize - 1) * self.entity_record_size;
        // SAFETY: `row` is a valid 1-based row of this query.
        Entity(unsafe { read_at(records, offset) })
    }

    /// Marks a written component changed, so replication sends it.
    ///
    /// The arguments are the change-tracking words a writing system's iterator
    /// provides.
    ///
    /// # Safety
    ///
    /// The words come from the current row of a live query that writes the
    /// component.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn mark(ecs: &enshwick_api::ecs::Ecs, words: &[u64; 10]) {
    ///     // SAFETY: the words of a writing system's current row.
    ///     unsafe { ecs.mark_changed(words[7], words[3], words[4], words[5], words[6]) };
    /// }
    /// ```
    pub unsafe fn mark_changed(
        &self,
        tracker: u64,
        stamp_a: u64,
        stamp_b: u64,
        entity: u64,
        kind: u64,
    ) {
        if tracker == 0 {
            return;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the engine packs the type and flags"
        )]
        let (type_index, flags) = (kind as u16, (kind >> 16) as u8);
        // SAFETY: forwarded from the caller.
        unsafe { (self.mark)(tracker, stamp_a, stamp_b, entity, type_index, flags) };
    }
}

/// A walk over a system's query; each row's iterator words.
///
/// # Examples
///
/// See [`Ecs::query`].
#[derive(Debug)]
pub struct Query<'e, const N: usize> {
    ecs: &'e Ecs,
    ctx: *mut u64,
    words: [u64; N],
}

impl<const N: usize> Query<'_, N> {
    /// Moves to the next row and returns its iterator words, or `None` at the
    /// end.
    ///
    /// # Examples
    ///
    /// See [`Ecs::query`].
    #[expect(
        clippy::should_implement_trait,
        reason = "rows borrow the query, so this is a lending walk"
    )]
    pub fn next(&mut self) -> Option<&[u64; N]> {
        // SAFETY: the query was created from a live context and a matching size.
        let more = unsafe { (self.ecs.next)(self.ctx, self.words.as_mut_ptr(), (N * 8) as u64) };
        (more & 0xff != 0).then_some(&self.words)
    }
}

type TypeIndex = unsafe extern "system" fn(map: *const u8, hash: *const u32) -> *const u16;
type ReadComponent = unsafe extern "system" fn(
    out: *mut [usize; 2],
    world: *const u8,
    index: *const u16,
    entity: u32,
);
type SlotRoot = unsafe extern "system" fn(
    out: *mut [u64; 2],
    slot: u64,
    owner_index: u64,
    ctx: *mut u64,
) -> *mut [u64; 2];

/// Component reads from inside a hooked ECS system, on the worker thread that
/// runs it: what another system's entities hold, such as a chest's
/// `UsedItem` or position.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick_api::Load<'_>) -> Result<enshwick_api::ecs::Components, enshwick_api::Off> {
///     enshwick_api::ecs::Components::resolve(load)
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Components {
    type_index: TypeIndex,
    read: ReadComponent,
    slot_root: SlotRoot,
    entity_world: usize,
    registry_maps: [usize; 2],
}

impl Components {
    /// Resolves the reads for the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader lacks any of them for this build.
    ///
    /// # Examples
    ///
    /// See [`Components`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let size = |name| {
            load.constant(name)
                .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
        };
        let (type_index, read, slot_root) = (
            load.symbol("ecs.type_index")?,
            load.symbol("ecs.component_read_raw")?,
            load.symbol("ecs.slot_root")?,
        );
        // SAFETY: the loader pins this id to the component type lookup.
        let type_index = unsafe { function::<TypeIndex>(type_index) };
        // SAFETY: the loader pins this id to the side-effect-free component read.
        let read = unsafe { function::<ReadComponent>(read) };
        // SAFETY: the loader pins this id to the slot root walk.
        let slot_root = unsafe { function::<SlotRoot>(slot_root) };
        Ok(Self {
            type_index,
            read,
            slot_root,
            entity_world: size("ecs.context_entity_world")?,
            registry_maps: [
                size("ecs.registry_type_map")?,
                size("ecs.registry_type_map_fallback")?,
            ],
        })
    }

    /// A pointer to `entity`'s component of type `hash` (see
    /// [`World::component_type`](crate::world::World::component_type)), or
    /// `None` when it has none. Writing through it is only right for
    /// components no client sees, which need no change mark.
    ///
    /// # Safety
    ///
    /// `ctx` is the live context of the hooked system, used on its thread.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn used_item(
    ///     c: &enshwick_api::ecs::Components,
    ///     ctx: *mut u64,
    ///     chest: enshwick_api::ecs::Entity,
    /// ) -> Option<u32> {
    ///     // SAFETY: called from a system's hook with its context.
    ///     let data = unsafe { c.component(ctx, chest, 0x0b02_361d) }?;
    ///     // SAFETY: UsedItem is the item id alone.
    ///     Some(unsafe { data.cast::<u32>().read_unaligned() })
    /// }
    /// ```
    #[must_use]
    pub unsafe fn component(&self, ctx: *mut u64, entity: Entity, hash: u32) -> Option<*mut u8> {
        if ctx.is_null() || entity.is_none() {
            return None;
        }
        // SAFETY: the first word of a context is its query descriptor, which
        // points at the entity world.
        let descriptor: *const u8 = unsafe { read_at(ctx.cast(), 0) };
        // SAFETY: as above.
        let world: *const u8 = unsafe { read_at(descriptor, self.entity_world) };
        if world.is_null() {
            return None;
        }
        // SAFETY: the entity world starts with its component registry.
        let registry: *const u8 = unsafe { read_at(world, 0) };
        let index = self.registry_maps.iter().find_map(|&map| {
            // SAFETY: the registry's type maps; lookups do not change them.
            let index = unsafe { (self.type_index)(registry.wrapping_add(map), &raw const hash) };
            (!index.is_null()).then_some(index)
        })?;
        let mut out = [0usize; 2];
        // SAFETY: a read with no side effects, from inside a running system.
        unsafe { (self.read)(&raw mut out, world, index, entity.0) };
        (out[0] != 0).then(|| core::ptr::with_exposed_provenance_mut(out[0]))
    }

    /// The inventory an item slot belongs to at the root: the chest or player
    /// holding it. `slot` is the game's slot id and `owner_index` the
    /// system's `OwnerRelationship` index pointer.
    ///
    /// # Safety
    ///
    /// As for [`Components::component`]; `owner_index` comes from the same
    /// system.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn root(c: &enshwick_api::ecs::Components, ctx: *mut u64, slot: u64, index: u64) {
    ///     // SAFETY: called from the inventory system's hook.
    ///     let _ = unsafe { c.slot_root(ctx, slot, index) };
    /// }
    /// ```
    #[must_use]
    pub unsafe fn slot_root(&self, ctx: *mut u64, slot: u64, owner_index: u64) -> Option<Entity> {
        let mut out = [0u64; 2];
        // SAFETY: the game's walk up `OwnerRelationship`, inside its system.
        unsafe { (self.slot_root)(&raw mut out, slot, owner_index, ctx) };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the error is the low byte, the root a u32"
        )]
        let (error, root) = (out[0] as u8, out[1] as u32);
        (error == 0 && root != 0).then_some(Entity(root))
    }
}

/// Whether a system context of `context_world` belongs to the server's world,
/// in a game binary of side `process` whose running server, if any, has the
/// entity world `server_world` (0 for none).
///
/// The dedicated server has no other world. A client that hosts a game runs
/// two: its own and the built-in server's, whose systems share the same
/// functions, so a hook shared by both tells them apart by the context's
/// entity world.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::Side;
/// use enshwick_api::ecs::server_role;
///
/// assert!(server_role(Side::SERVER, 0x10, 0));
/// assert!(server_role(Side::CLIENT, 0x20, 0x20));
/// assert!(!server_role(Side::CLIENT, 0x10, 0x20));
/// assert!(!server_role(Side::CLIENT, 0, 0));
/// ```
#[must_use]
pub fn server_role(process: Side, context_world: u64, server_world: u64) -> bool {
    process == Side::SERVER || (server_world != 0 && context_world == server_world)
}

/// Tells the server's world from the client's inside a hooked system.
///
/// On a client that hosts a game, functions such as cursor checks run for
/// both worlds; a server feature acts only in the server's. On the dedicated
/// server every context is the server's, without a read.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick_api::Load<'_>) -> Result<enshwick_api::ecs::Worlds, enshwick_api::Off> {
///     enshwick_api::ecs::Worlds::resolve(load)
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Worlds {
    host: Host,
    side: Side,
    entity_world: usize,
}

impl Worlds {
    /// Resolves the test for the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader lacks the context layout for this build.
    ///
    /// # Examples
    ///
    /// See [`Worlds`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let host = load.host();
        let entity_world = load.constant("ecs.context_entity_world")?;
        Ok(Self {
            host,
            side: host.build().side,
            entity_world: usize::try_from(entity_world).unwrap_or(usize::MAX),
        })
    }

    /// Whether this process runs a game server now; see
    /// [`Host::hosting`](crate::Host::hosting).
    ///
    /// # Examples
    ///
    /// ```
    /// fn acts(worlds: &enshwick_api::ecs::Worlds) -> bool {
    ///     worlds.hosting()
    /// }
    /// ```
    #[must_use]
    pub fn hosting(&self) -> bool {
        self.side == Side::SERVER || self.host.hosting()
    }

    /// Whether `ctx`, a system context, belongs to the server's world: always
    /// on the dedicated server; on a client, only for the built-in server's
    /// systems while it hosts a game, from its first step.
    ///
    /// # Safety
    ///
    /// `ctx` is the live context of a running system, or null.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn mine(worlds: &enshwick_api::ecs::Worlds, ctx: *mut u64) -> bool {
    ///     // SAFETY: called from a system's hook with its context.
    ///     unsafe { worlds.is_server(ctx) }
    /// }
    /// ```
    #[must_use]
    pub unsafe fn is_server(&self, ctx: *mut u64) -> bool {
        if self.side == Side::SERVER {
            return true;
        }
        let server_world = self.host.server_world();
        if server_world == 0 || ctx.is_null() {
            return false;
        }
        // SAFETY: the first word of a context is its query descriptor.
        let descriptor: *const u8 = unsafe { read_at(ctx.cast(), 0) };
        if descriptor.is_null() {
            return false;
        }
        // SAFETY: the descriptor points at its entity world here.
        let world: u64 = unsafe { read_at(descriptor, self.entity_world) };
        server_role(self.side, world, server_world)
    }
}

/// Metres from the engine's 32.32 fixed-point world coordinates, as
/// `CurrentTransform` stores them.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick_api::ecs::metres([3_i64 << 32, -(1_i64 << 31), 0]), [3.0, -0.5, 0.0]);
/// ```
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "world coordinates fit an f64")]
pub fn metres(fixed: [i64; 3]) -> [f64; 3] {
    fixed.map(|v| v as f64 / 4_294_967_296.0)
}

type AttributeFind = unsafe extern "system" fn(out: *mut u64, attribute: u32, registry: u64) -> u64;

/// Reads and writes of attribute sets: components such as `Level` whose values are
/// attributes looked up by hash in the row's attribute registry.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick_api::Load<'_>) -> Result<enshwick_api::ecs::Attributes, enshwick_api::Off> {
///     enshwick_api::ecs::Attributes::resolve(load)
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Attributes {
    find: AttributeFind,
    flags: usize,
    data_offset: usize,
    slot: usize,
}

impl Attributes {
    /// Resolves the reads for the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader lacks any of them for this build.
    ///
    /// # Examples
    ///
    /// See [`Attributes`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let size = |name| {
            load.constant(name)
                .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
        };
        let find = load.symbol("ecs.attribute_find")?;
        Ok(Self {
            // SAFETY: the loader pins this id to the attribute lookup.
            find: unsafe { function(find) },
            flags: size("ecs.level_flags")?,
            data_offset: size("ecs.level_data_offset")?,
            slot: size("ecs.attribute_slot")?,
        })
    }

    /// The value of `attribute` in the attribute set `set`, looked up in the
    /// row's attribute `registry`; `None` when the set is not initialised or
    /// does not hold the attribute.
    ///
    /// # Safety
    ///
    /// `set` is a live attribute-set component and `registry` the attribute
    /// registry word of the same row, inside the hooked system.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn level(a: &enshwick_api::ecs::Attributes, registry: u64, set: *const u8) -> Option<u32> {
    ///     // SAFETY: the row's Level component and attribute registry.
    ///     unsafe { a.value(registry, set, 0x724b_f568) }
    /// }
    /// ```
    #[must_use]
    pub unsafe fn value(&self, registry: u64, set: *const u8, attribute: u32) -> Option<u32> {
        // SAFETY: forwarded from the caller.
        let word = unsafe { self.word(registry, set, attribute) }?;
        // SAFETY: the attribute's word inside the set's data.
        Some(unsafe { word.cast::<u32>().read_unaligned() })
    }

    /// Writes `value` to `attribute` in the attribute set `set`; false when the
    /// set is not initialised or does not hold the attribute, and nothing was
    /// written. A component clients see needs its change mark as well
    /// ([`Ecs::mark_changed`]) for replication to send it.
    ///
    /// # Safety
    ///
    /// As for [`Attributes::value`], and the hooked system writes `set`.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn reset(a: &enshwick_api::ecs::Attributes, registry: u64, set: *mut u8) -> bool {
    ///     // SAFETY: a set the hooked system writes, and its row's registry.
    ///     unsafe { a.set(registry, set, 0x1d7d_d102, 0) }
    /// }
    /// ```
    pub unsafe fn set(&self, registry: u64, set: *mut u8, attribute: u32, value: u32) -> bool {
        // SAFETY: forwarded from the caller.
        let Some(word) = (unsafe { self.word(registry, set, attribute) }) else {
            return false;
        };
        // SAFETY: the attribute's word inside a set the system writes.
        unsafe { word.cast_mut().cast::<u32>().write_unaligned(value) };
        true
    }

    /// The address of `attribute`'s word in the set, if it holds it.
    ///
    /// # Safety
    ///
    /// As for [`Attributes::value`].
    unsafe fn word(&self, registry: u64, set: *const u8, attribute: u32) -> Option<*const u8> {
        if set.is_null() {
            return None;
        }
        // SAFETY: the set's flags; bit 0 marks it initialised.
        let flags: u8 = unsafe { read_at(set, self.flags) };
        if flags & 1 == 0 {
            return None;
        }
        let mut found = [0u64; 2];
        // SAFETY: the row's attribute registry, inside its system.
        unsafe { (self.find)(found.as_mut_ptr(), attribute, registry) };
        let definition =
            core::ptr::with_exposed_provenance::<u8>(usize::try_from(found[1]).unwrap_or(0));
        if found[0] & 0xff != 0 || definition.is_null() {
            return None;
        }
        // SAFETY: a found definition starts with its set id.
        let definition_set: u32 = unsafe { read_at(definition, 0) };
        // SAFETY: so does the set.
        let set_id: u32 = unsafe { read_at(set, 0) };
        if definition_set != set_id {
            return None;
        }
        // SAFETY: the definition's slot.
        let slot = usize::from(unsafe { read_at::<u16>(definition, self.slot) });
        // SAFETY: the set's data offset.
        let data = usize::from(unsafe { read_at::<u16>(set, self.data_offset) });
        Some(set.wrapping_add(data + slot * 4))
    }
}
