//! Access to the engine's ECS from inside a hooked system.
//!
//! A system's update function receives a query context. [`Ecs`] walks that
//! query with the engine's own iterator, reads the entity of the current row
//! and marks written components changed so replication follows them. Every
//! address and offset comes from the loader for the running build.

use crate::abi::Side;
use crate::raw::{read_at, read_ptr_at, write_at};
use crate::{Entity, Host, Load, Off};

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

/// The engine's ECS functions and layout on the running build.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick::Load<'_>) -> Result<enshwick::ecs::Ecs, enshwick::Off> {
///     enshwick::ecs::Ecs::resolve(load)
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
/// - `address` is the entry of a function with the signature and ABI of `F`.
unsafe fn function<F: crate::FnPtr>(address: u64) -> F {
    let address = usize::try_from(address).unwrap_or(0);
    // SAFETY: `F: FnPtr` is a function pointer the size of `usize`, and the
    // caller guarantees `address` is a function of type `F`.
    unsafe { core::mem::transmute_copy(&address) }
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
        // SAFETY: the build table's `ecs.iterator_init` is the query
        // iterator's init, with the signature of `IteratorInit`.
        let init = unsafe { function::<IteratorInit>(init) };
        // SAFETY: the build table's `ecs.iterator_next` is the query
        // iterator's step, with the signature of `IteratorNext`.
        let next = unsafe { function::<IteratorNext>(next) };
        // SAFETY: the build table's `ecs.mark_changed` is the engine's change
        // mark, with the signature of `MarkChanged`.
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
    /// - `ctx` is the live query context the hooked system was called with,
    ///   used on that system's thread during the hook.
    /// - `N` is the number of iterator words the system declares.
    /// - The [`Query`] is dropped before the hook returns: its safe
    ///   [`Query::next`] walks `ctx`, so a query kept past the hook's return,
    ///   in a static or another thread's hands, walks freed memory.
    ///
    /// # Examples
    ///
    /// ```
    /// /// Counts the rows of a system's query.
    /// unsafe fn rows(ecs: &enshwick::ecs::Ecs, ctx: *mut u64) -> usize {
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
        // SAFETY: the caller guarantees `ctx` is the hooked system's live
        // context and `N` its iterator's word count; `words` holds those
        // `N * 8` writable bytes.
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
    /// - `ctx` is the live query context the hooked system was called with,
    ///   used on that system's thread during the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// /// Walks the query again from its first row, leaving the counter as found.
    /// unsafe fn again(ecs: &enshwick::ecs::Ecs, ctx: *mut u64) {
    ///     // SAFETY: called from the system's hook with its live context.
    ///     let row = unsafe { ecs.row(ctx) };
    ///     // SAFETY: `ctx` is the hooked system's live context, on its thread.
    ///     unsafe { ecs.set_row(ctx, 0) };
    ///     // ... a second pass over the rows ...
    ///     // SAFETY: `ctx` is the hooked system's live context, and `row` its own counter.
    ///     unsafe { ecs.set_row(ctx, row) };
    /// }
    /// ```
    #[must_use]
    pub unsafe fn row(&self, ctx: *mut u64) -> u32 {
        // SAFETY: the caller guarantees a live context, which keeps its u32
        // row counter at the build table's `ecs.context_row`.
        unsafe { read_at(ctx.cast(), self.context_row) }
    }

    /// Moves the query's row counter, so a pass can start again from the
    /// first row and leave the counter as it found it.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live query context the hooked system was called with,
    ///   used on that system's thread during the hook.
    /// - `row` is 0 or a row of the query, such as one [`Ecs::row`] read.
    ///
    /// # Examples
    ///
    /// See [`Ecs::row`].
    pub unsafe fn set_row(&self, ctx: *mut u64, row: u32) {
        // SAFETY: the caller guarantees a live context on the system's thread,
        // which keeps its u32 row counter at the build table's
        // `ecs.context_row`.
        unsafe { write_at(ctx.cast(), self.context_row, row) };
    }

    /// The entity of the query's current row, `None` before the first.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live query context the hooked system was called with,
    ///   used on that system's thread during the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn current(ecs: &enshwick::ecs::Ecs, ctx: *mut u64) -> Option<enshwick::Entity> {
    ///     // SAFETY: called from the system's hook with its context.
    ///     unsafe { ecs.entity(ctx) }
    /// }
    /// ```
    #[must_use]
    pub unsafe fn entity(&self, ctx: *mut u64) -> Option<Entity> {
        // SAFETY: the caller guarantees `ctx` is the hooked system's live
        // context, used on its thread, which is the contract of `row`.
        let row = unsafe { self.row(ctx) };
        if row == 0 {
            return None;
        }
        // SAFETY: the caller guarantees a live context, whose first word is
        // its query descriptor (engine layout).
        let descriptor = unsafe { read_ptr_at::<u8>(ctx.cast(), 0) }.cast_const();
        // SAFETY: a live query descriptor keeps the pointer to its per-row
        // entity records at the build table's `ecs.descriptor_entity_records`.
        let records = unsafe { read_ptr_at::<u8>(descriptor, self.entity_records) }.cast_const();
        let offset = (row as usize - 1) * self.entity_record_size;
        // SAFETY: `row` is the context's own 1-based row, not 0 as checked
        // above, and the records hold one of `ecs.entity_record_size` bytes
        // per row, each starting with the row's u32 entity id.
        Entity::from_raw(unsafe { read_at(records, offset) })
    }

    /// Marks a written component changed, so replication sends it.
    ///
    /// The arguments are the change-tracking words a writing system's iterator
    /// provides.
    ///
    /// # Safety
    ///
    /// - The words come from the current row of a live query of the hooked
    ///   system, which writes the component, used on that system's thread
    ///   during the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn mark(ecs: &enshwick::ecs::Ecs, words: &[u64; 10]) {
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
        // SAFETY: the caller guarantees the words of the current row of a live
        // query that writes the component, on its thread, which is what the
        // build table's `ecs.mark_changed` takes; `tracker` is not 0, checked
        // above.
        unsafe { (self.mark)(tracker, stamp_a, stamp_b, entity, type_index, flags) };
    }
}

/// A walk over a system's query; each row's iterator words.
///
/// Made by [`Ecs::query`], whose safety contract keeps it inside the hooked
/// call; it holds the context as a raw pointer, so it is neither `Send` nor
/// `Sync`.
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
        // SAFETY: only `Ecs::query` makes a `Query`, whose caller guarantees a
        // live `ctx` of the hooked system, `N` its iterator's word count, and
        // that the query is dropped before the hook returns; `words` holds
        // `N * 8` writable bytes.
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

/// A reflected ECS component type, resolved once at init, which component
/// reads take.
///
/// # Examples
///
/// ```
/// use enshwick::ecs::ComponentType;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<ComponentType, enshwick::Off> {
///     ComponentType::resolve(load, "keen::ecs::UsedItem")
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComponentType(u32);

impl ComponentType {
    /// The component type named `name`, such as `keen::ecs::UsedItem`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the type is not reflected on this build.
    ///
    /// # Examples
    ///
    /// See [`ComponentType`].
    pub fn resolve(load: &Load<'_>, name: &str) -> Result<Self, Off> {
        Self::named(load.host(), name)
    }

    /// The component type named `name` on the host's build.
    pub(crate) fn named(host: Host, name: &str) -> Result<Self, Off> {
        let mut hash = 0;
        let status = (host.api().type_hash)(crate::abi::Str::new(name), &raw mut hash);
        crate::load::ok(status, Self(hash), || format!("component type {name}"))
    }

    /// The type whose hash is `hash`, for a hash the loader already checked.
    pub(crate) const fn from_hash(hash: u32) -> Self {
        Self(hash)
    }

    /// The type's hash, the engine's id for it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn hash(load: &enshwick::Load<'_>) -> Result<u32, enshwick::Off> {
    ///     Ok(enshwick::ecs::ComponentType::resolve(load, "keen::ecs::UsedItem")?.hash())
    /// }
    /// ```
    #[must_use]
    pub const fn hash(self) -> u32 {
        self.0
    }

    /// A pointer to `entity`'s component of this type on the server's game
    /// thread, valid until the game thread returns to the game; `None` when
    /// the entity has none.
    pub(crate) fn read(self, host: Host, entity: Entity) -> Option<*const core::ffi::c_void> {
        let mut out: *const core::ffi::c_void = core::ptr::null();
        let status = (host.api().component)(host.handle(), entity, self.0, &raw mut out);
        (status == crate::abi::Status::OK && !out.is_null()).then_some(out)
    }
}

/// Component reads from inside a hooked ECS system, on the worker thread that
/// runs it: what another system's entities hold, such as a chest's
/// `UsedItem` or position.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick::Load<'_>) -> Result<enshwick::ecs::Components, enshwick::Off> {
///     enshwick::ecs::Components::resolve(load)
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
        // SAFETY: the build table's `ecs.type_index` is the component type
        // lookup, with the signature of `TypeIndex`.
        let type_index = unsafe { function::<TypeIndex>(type_index) };
        // SAFETY: the build table's `ecs.component_read_raw` is the component
        // read without side effects, with the signature of `ReadComponent`.
        let read = unsafe { function::<ReadComponent>(read) };
        // SAFETY: the build table's `ecs.slot_root` is the walk up
        // `OwnerRelationship`, with the signature of `SlotRoot`.
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

    /// A pointer to `entity`'s component of type `ty`, or `None` when it has
    /// none. Writing through it is only right for components no client sees,
    /// which need no change mark.
    ///
    /// # Safety
    ///
    /// - `ctx` is null or the live query context the hooked system was called
    ///   with, used on that system's thread during the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ecs::{ComponentType, Components};
    /// use enshwick::raw::read_at;
    ///
    /// unsafe fn used_item(
    ///     c: &Components,
    ///     ctx: *mut u64,
    ///     chest: enshwick::Entity,
    ///     used_item: ComponentType,
    /// ) -> Option<u32> {
    ///     // SAFETY: called from a system's hook with its live context.
    ///     let data = unsafe { c.component(ctx, chest, used_item) }?;
    ///     // SAFETY: a live UsedItem, inside the hook, is its u32 item id alone.
    ///     Some(unsafe { read_at(data, 0) })
    /// }
    /// ```
    #[must_use]
    pub unsafe fn component(
        &self,
        ctx: *mut u64,
        entity: Entity,
        ty: ComponentType,
    ) -> Option<*mut u8> {
        if ctx.is_null() || entity.is_none() {
            return None;
        }
        let hash = ty.0;
        // SAFETY: `ctx` is not null, as the early return ensures, and the
        // caller guarantees a live context, whose first word is its query
        // descriptor (engine layout).
        let descriptor = unsafe { read_ptr_at::<u8>(ctx.cast(), 0) }.cast_const();
        // SAFETY: a live query descriptor keeps the pointer to its entity
        // world at the build table's `ecs.context_entity_world`.
        let world = unsafe { read_ptr_at::<u8>(descriptor, self.entity_world) }.cast_const();
        if world.is_null() {
            return None;
        }
        // SAFETY: `world` is not null, as the early return ensures, and an
        // entity world starts with the pointer to its component registry
        // (engine layout).
        let registry = unsafe { read_ptr_at::<u8>(world, 0) }.cast_const();
        let index = self.registry_maps.iter().find_map(|&map| {
            // SAFETY: the registry keeps its type maps at the build table's
            // `ecs.registry_type_map` and `ecs.registry_type_map_fallback`;
            // the lookup only reads them and `hash`, a local.
            let index = unsafe { (self.type_index)(registry.wrapping_add(map), &raw const hash) };
            (!index.is_null()).then_some(index)
        })?;
        let mut out = [0usize; 2];
        // SAFETY: `world` is the live entity world of the hooked system and
        // `index` its registry's entry for `hash`; the read has no side
        // effects and writes two words to `out`, a local.
        unsafe { (self.read)(&raw mut out, world, index, entity.0) };
        (out[0] != 0).then(|| core::ptr::with_exposed_provenance_mut(out[0]))
    }

    /// The inventory an item slot belongs to at the root: the chest or player
    /// holding it. `slot` is the game's slot id and `owner_index` the
    /// system's `OwnerRelationship` index pointer.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live query context the hooked system was called with,
    ///   used on that system's thread during the hook.
    /// - `owner_index` is that system's `OwnerRelationship` index pointer,
    ///   from the same call.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn root(c: &enshwick::ecs::Components, ctx: *mut u64, slot: u64, index: u64) {
    ///     // SAFETY: the inventory system's live context and index, inside its hook.
    ///     let _ = unsafe { c.slot_root(ctx, slot, index) };
    /// }
    /// ```
    #[must_use]
    pub unsafe fn slot_root(&self, ctx: *mut u64, slot: u64, owner_index: u64) -> Option<Entity> {
        let mut out = [0u64; 2];
        // SAFETY: the caller guarantees the hooked system's live context and
        // its own `OwnerRelationship` index, on its thread, which the build
        // table's `ecs.slot_root` takes; it writes two words to `out`, a
        // local.
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
/// use enshwick::abi::Side;
/// use enshwick::ecs::server_role;
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
/// fn init(load: &mut enshwick::Load<'_>) -> Result<enshwick::ecs::Worlds, enshwick::Off> {
///     enshwick::ecs::Worlds::resolve(load)
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
    /// fn acts(worlds: &enshwick::ecs::Worlds) -> bool {
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
    /// - `ctx` is null or the live query context a hooked system was called
    ///   with, used on that system's thread during the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn mine(worlds: &enshwick::ecs::Worlds, ctx: *mut u64) -> bool {
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
        // SAFETY: `ctx` is not null, as the early return ensures, and the
        // caller guarantees a live context, whose first word is its query
        // descriptor (engine layout).
        let descriptor = unsafe { read_ptr_at::<u8>(ctx.cast(), 0) }.cast_const();
        if descriptor.is_null() {
            return false;
        }
        // SAFETY: `descriptor` is not null, as the early return ensures, and a
        // live query descriptor keeps its entity world's address at the build
        // table's `ecs.context_entity_world`.
        let world: u64 = unsafe { read_at(descriptor, self.entity_world) };
        server_role(self.side, world, server_world)
    }
}

/// Engine units per metre: world coordinates are 32.32 fixed point.
///
/// # Examples
///
/// ```
/// use enshwick::ecs::FIXED_PER_METRE;
///
/// let half_metre = (0.5 * FIXED_PER_METRE) as i64;
/// assert_eq!(half_metre, 1 << 31);
/// ```
pub const FIXED_PER_METRE: f64 = 4_294_967_296.0;

/// Metres from the engine's 32.32 fixed-point world coordinates, as
/// `CurrentTransform` stores them.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick::ecs::metres([3_i64 << 32, -(1_i64 << 31), 0]), [3.0, -0.5, 0.0]);
/// ```
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "world coordinates fit an f64")]
pub fn metres(fixed: [i64; 3]) -> [f64; 3] {
    fixed.map(|v| v as f64 / FIXED_PER_METRE)
}

/// The engine's 32.32 fixed-point world coordinates from metres, the inverse
/// of [`metres`]: fractions below a unit are cut toward zero, values past the
/// range saturate and NaN becomes 0.
///
/// # Examples
///
/// ```
/// use enshwick::ecs::{fixed, metres};
///
/// assert_eq!(fixed([3.0, -0.5, 0.0]), [3_i64 << 32, -(1_i64 << 31), 0]);
/// assert_eq!(metres(fixed([12.25, 0.0, -7.75])), [12.25, 0.0, -7.75]);
/// ```
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "a float-to-int cast saturates, as documented"
)]
pub fn fixed(metres: [f64; 3]) -> [i64; 3] {
    metres.map(|m| (m * FIXED_PER_METRE) as i64)
}

/// The position a `CurrentTransform` component holds, in metres: x, y and z
/// in 32.32 fixed point come first, y up.
///
/// # Safety
///
/// - `transform` is readable for 24 bytes, such as a live `CurrentTransform`
///   that [`Components::component`] or
///   [`World::component`](crate::world::World::component) returned, while it
///   is valid; no alignment is needed.
/// - No other thread writes those bytes during the read.
///
/// # Examples
///
/// ```
/// use enshwick::ecs::transform_position;
///
/// let transform: [i64; 3] = [2_i64 << 32, 1 << 31, -(3_i64 << 32)];
/// // SAFETY: `transform` is 24 readable bytes, three fixed-point coordinates.
/// let at = unsafe { transform_position(transform.as_ptr().cast()) };
/// assert_eq!(at, [2.0, 0.5, -3.0]);
/// ```
#[must_use]
pub unsafe fn transform_position(transform: *const u8) -> [f64; 3] {
    // SAFETY: the caller guarantees 24 readable bytes with no concurrent
    // writer; a CurrentTransform starts with its three i64 coordinates.
    metres(unsafe { read_at(transform, 0) })
}

type AttributeFind = unsafe extern "system" fn(out: *mut u64, attribute: u32, registry: u64) -> u64;

/// Reads and writes of attribute sets: components such as `Level` whose values are
/// attributes looked up by hash in the row's attribute registry.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick::Load<'_>) -> Result<enshwick::ecs::Attributes, enshwick::Off> {
///     enshwick::ecs::Attributes::resolve(load)
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
            // SAFETY: the build table's `ecs.attribute_find` is the attribute
            // lookup, with the signature of `AttributeFind`.
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
    /// - `set` is null or a live attribute-set component of the hooked
    ///   system's current row, used on its thread during the hook.
    /// - `registry` is the attribute registry word of the same row.
    /// - No other thread writes `set` during the call.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn level(a: &enshwick::ecs::Attributes, registry: u64, set: *const u8) -> Option<u32> {
    ///     // SAFETY: the row's Level component and attribute registry.
    ///     unsafe { a.value(registry, set, 0x724b_f568) }
    /// }
    /// ```
    #[must_use]
    pub unsafe fn value(&self, registry: u64, set: *const u8, attribute: u32) -> Option<u32> {
        // SAFETY: the caller guarantees a live set of the current row, its
        // row's registry and no concurrent writer, which is the contract of
        // `word`.
        let word = unsafe { self.word(registry, set, attribute) }?;
        // SAFETY: `word` found the attribute's 32-bit word inside the live
        // set's data.
        Some(unsafe { read_at(word, 0) })
    }

    /// Writes `value` to `attribute` in the attribute set `set`; false when the
    /// set is not initialised or does not hold the attribute, and nothing was
    /// written. A component clients see needs its change mark as well
    /// ([`Ecs::mark_changed`]) for replication to send it.
    ///
    /// # Safety
    ///
    /// - `set` is null or a live attribute-set component of the hooked
    ///   system's current row, which that system writes, used on its thread
    ///   during the hook.
    /// - `registry` is the attribute registry word of the same row.
    /// - Nothing else reads or writes `set` during the call.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn reset(a: &enshwick::ecs::Attributes, registry: u64, set: *mut u8) -> bool {
    ///     // SAFETY: a set the hooked system writes, and its row's registry.
    ///     unsafe { a.set(registry, set, 0x1d7d_d102, 0) }
    /// }
    /// ```
    pub unsafe fn set(&self, registry: u64, set: *mut u8, attribute: u32, value: u32) -> bool {
        // SAFETY: the caller guarantees a live set of the current row, its
        // row's registry and no concurrent writer, which is the contract of
        // `word`.
        let Some(word) = (unsafe { self.word(registry, set, attribute) }) else {
            return false;
        };
        // SAFETY: `word` found the attribute's 32-bit word inside the live
        // set's data, which the hooked system writes and nothing else touches
        // during the call, as the caller guarantees.
        unsafe { write_at(word.cast_mut(), 0, value) };
        true
    }

    /// The address of `attribute`'s word in the set, if it holds it.
    ///
    /// # Safety
    ///
    /// - `set` is null or a live attribute-set component of the hooked
    ///   system's current row, used on its thread during the hook.
    /// - `registry` is the attribute registry word of the same row.
    /// - No other thread writes `set` during the call.
    unsafe fn word(&self, registry: u64, set: *const u8, attribute: u32) -> Option<*const u8> {
        if set.is_null() {
            return None;
        }
        // SAFETY: `set` is not null, as the early return ensures, and the
        // caller guarantees a live attribute set, whose flag byte sits at the
        // build table's `ecs.level_flags`.
        let flags: u8 = unsafe { read_at(set, self.flags) };
        if flags & 1 == 0 {
            return None;
        }
        let mut found = [0u64; 2];
        // SAFETY: the caller guarantees `registry` is the current row's
        // attribute registry, on the hooked system's thread, which the build
        // table's `ecs.attribute_find` takes; it writes two words to `found`,
        // a local.
        unsafe { (self.find)(found.as_mut_ptr(), attribute, registry) };
        let definition =
            core::ptr::with_exposed_provenance::<u8>(usize::try_from(found[1]).unwrap_or(0));
        if found[0] & 0xff != 0 || definition.is_null() {
            return None;
        }
        // SAFETY: `definition` is not null, as the early return ensures, and an
        // attribute definition the lookup found starts with its u32 set id
        // (engine layout).
        let definition_set: u32 = unsafe { read_at(definition, 0) };
        // SAFETY: `set` is the live attribute set the caller guarantees, which
        // starts with its u32 set id (engine layout).
        let set_id: u32 = unsafe { read_at(set, 0) };
        if definition_set != set_id {
            return None;
        }
        // SAFETY: `definition` is the found attribute definition, which keeps
        // its u16 slot at the build table's `ecs.attribute_slot`.
        let slot = usize::from(unsafe { read_at::<u16>(definition, self.slot) });
        // SAFETY: `set` is the live, initialised attribute set (flag checked
        // above), which keeps its u16 data offset at the build table's
        // `ecs.level_data_offset`.
        let data = usize::from(unsafe { read_at::<u16>(set, self.data_offset) });
        Some(set.wrapping_add(data + slot * 4))
    }
}
