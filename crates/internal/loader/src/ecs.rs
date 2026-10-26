//! The engine helpers the loader's own links use: walking a system's query,
//! the current row's entity, change marks, attribute reads, component reads
//! and zone sizes.

use crate::pinned;
use crate::pinned::{Constant, Symbol};
use std::sync::OnceLock;

use enshwick::raw::{read_at, read_ptr_at, write_at};

use crate::host::Loader;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::altar::LEVEL_ATTRIBUTE.id(),
    pinned::balancing::ZONE_SIZE.id(),
    pinned::ecs::ATTRIBUTE_FIND.id(),
    pinned::ecs::ATTRIBUTE_SLOT.id(),
    pinned::ecs::COMPONENT_READ.id(),
    pinned::ecs::COMPONENT_READ_RAW.id(),
    pinned::ecs::CONTEXT_ENTITY_WORLD.id(),
    pinned::ecs::CONTEXT_ROW.id(),
    pinned::ecs::DESCRIPTOR_ENTITY_RECORDS.id(),
    pinned::ecs::ENTITY_RECORD_SIZE.id(),
    pinned::ecs::ITERATOR_INIT.id(),
    pinned::ecs::ITERATOR_NEXT.id(),
    pinned::ecs::LEVEL_DATA_OFFSET.id(),
    pinned::ecs::LEVEL_FLAGS.id(),
    pinned::ecs::MARK_CHANGED.id(),
    pinned::ecs::REGISTRY_TYPE_MAP.id(),
    pinned::ecs::REGISTRY_TYPE_MAP_FALLBACK.id(),
    pinned::ecs::SLOT_ROOT.id(),
    pinned::ecs::TYPE_INDEX.id(),
];

static ENGINE: OnceLock<Engine> = OnceLock::new();

/// Checks that `system` still reads and writes the components the loader's
/// iterator layout for it assumes, in that order: the layout moves when
/// they change, on a build found by patterns too.
///
/// # Errors
///
/// A line for the log naming what the build's system has instead.
pub(crate) fn components_match(
    loader: &Loader,
    system: &str,
    reads: &[&str],
    writes: &[&str],
) -> Result<(), String> {
    let found = loader
        .reflection
        .system(system)
        .map_err(|e| format!("no system {system}: {e}"))?;
    if found.reads() == reads && found.writes() == writes {
        Ok(())
    } else {
        Err(format!(
            "{system} reads {:?} and writes {:?}, not the components its iterator layout assumes",
            found.reads(),
            found.writes()
        ))
    }
}

/// The helpers on this build, resolved once; `None` when the table lacks one.
pub(crate) fn engine(loader: &Loader) -> Option<&'static Engine> {
    if let Some(engine) = ENGINE.get() {
        return Some(engine);
    }
    let engine = Engine::resolve(loader)?;
    Some(ENGINE.get_or_init(|| engine))
}

/// The helpers once resolved.
pub(crate) fn resolved() -> Option<&'static Engine> {
    ENGINE.get()
}

type IteratorInit = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64);
type IteratorNext = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64) -> u64;
type MarkChanged = unsafe extern "system" fn(
    tracker: u64,
    stamp_a: u64,
    stamp_b: u64,
    entity: u64,
    type_index: u16,
    flags: u8,
);
type AttributeFind = unsafe extern "system" fn(out: *mut u64, attribute: u32, registry: u64) -> u64;
type ZoneSize = unsafe extern "system" fn(out: *mut f32, balancing: u64, level: u64) -> u64;
type ComponentRead =
    unsafe extern "system" fn(ctx: *mut u64, kind: *const u16, entity: u32) -> *const u8;

/// The engine functions and layout on the running build.
#[derive(Debug)]
pub(crate) struct Engine {
    init: IteratorInit,
    next: IteratorNext,
    mark: MarkChanged,
    find: AttributeFind,
    zone_size: ZoneSize,
    component_read: ComponentRead,
    context_row: usize,
    entity_records: usize,
    entity_record_size: usize,
    entity_world: usize,
    level_flags: usize,
    level_data: usize,
    attribute_slot: usize,
    level_attribute: u32,
}

impl Engine {
    /// The helpers on this build, if the table has them all.
    pub(crate) fn resolve(loader: &Loader) -> Option<Self> {
        let function = |name: Symbol| {
            loader
                .build
                .symbol(name.id())
                .and_then(|rva| usize::try_from(loader.base + rva).ok())
        };
        let size = |name: Constant| {
            loader
                .build
                .constant(name.id())
                .and_then(|v| usize::try_from(v).ok())
        };
        let (init, next, mark, find, zone_size, component_read) = (
            function(pinned::ecs::ITERATOR_INIT)?,
            function(pinned::ecs::ITERATOR_NEXT)?,
            function(pinned::ecs::MARK_CHANGED)?,
            function(pinned::ecs::ATTRIBUTE_FIND)?,
            function(pinned::balancing::ZONE_SIZE)?,
            function(pinned::ecs::COMPONENT_READ)?,
        );
        Some(Self {
            // SAFETY: the build table's `ecs.iterator_init` is the game function with the
            // `IteratorInit` signature on this build.
            init: unsafe { core::mem::transmute::<usize, IteratorInit>(init) },
            // SAFETY: the build table's `ecs.iterator_next` is the game function with the
            // `IteratorNext` signature on this build.
            next: unsafe { core::mem::transmute::<usize, IteratorNext>(next) },
            // SAFETY: the build table's `ecs.mark_changed` is the game function with the
            // `MarkChanged` signature on this build.
            mark: unsafe { core::mem::transmute::<usize, MarkChanged>(mark) },
            // SAFETY: the build table's `ecs.attribute_find` is the game function with the
            // `AttributeFind` signature on this build.
            find: unsafe { core::mem::transmute::<usize, AttributeFind>(find) },
            // SAFETY: the build table's `balancing.zone_size` is the game function with the
            // `ZoneSize` signature on this build.
            zone_size: unsafe { core::mem::transmute::<usize, ZoneSize>(zone_size) },
            // SAFETY: the build table's `ecs.component_read` is the game function with the
            // `ComponentRead` signature on this build.
            component_read: unsafe { core::mem::transmute::<usize, ComponentRead>(component_read) },
            context_row: size(pinned::ecs::CONTEXT_ROW)?,
            entity_records: size(pinned::ecs::DESCRIPTOR_ENTITY_RECORDS)?,
            entity_record_size: size(pinned::ecs::ENTITY_RECORD_SIZE)?,
            entity_world: size(pinned::ecs::CONTEXT_ENTITY_WORLD)?,
            level_flags: size(pinned::ecs::LEVEL_FLAGS)?,
            level_data: size(pinned::ecs::LEVEL_DATA_OFFSET)?,
            attribute_slot: size(pinned::ecs::ATTRIBUTE_SLOT)?,
            level_attribute: u32::try_from(
                loader.build.constant(pinned::altar::LEVEL_ATTRIBUTE.id())?,
            )
            .ok()?,
        })
    }

    /// Walks the system's query from its first row.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live query context the hooked system was given, used
    ///   on that system's thread while it runs.
    /// - `N` words is the iterator size the build's layout for that system
    ///   uses.
    /// - The returned [`Query`] is dropped before the system's run ends.
    pub(crate) unsafe fn query<const N: usize>(&self, ctx: *mut u64) -> Query<'_, N> {
        let mut words = [0u64; N];
        // SAFETY: the caller guarantees `ctx` is the running system's live
        // context and `N` words its iterator size; `words` holds `N`
        // writable words.
        unsafe { (self.init)(ctx, words.as_mut_ptr(), (N * 8) as u64) };
        Query {
            engine: self,
            ctx,
            words,
        }
    }

    /// The 1-based row the query is on, 0 before the first.
    ///
    /// # Safety
    ///
    /// - `ctx` is a live query context, used on its system's thread.
    pub(crate) unsafe fn row(&self, ctx: *mut u64) -> u32 {
        // SAFETY: the caller guarantees a live context; its u32 row counter
        // sits at the build table's `ecs.context_row` offset inside it.
        unsafe { read_at(ctx.cast(), self.context_row) }
    }

    /// Moves the query's row counter.
    ///
    /// # Safety
    ///
    /// - `ctx` is a live query context, used on its system's thread, which
    ///   alone writes it during the call.
    pub(crate) unsafe fn set_row(&self, ctx: *mut u64, row: u32) {
        // SAFETY: the caller guarantees a live context nothing else writes
        // now; its u32 row counter sits at the build table's
        // `ecs.context_row` offset inside it.
        unsafe { write_at(ctx.cast(), self.context_row, row) };
    }

    /// The entity of the query's current row, 0 before the first or for a
    /// null context.
    ///
    /// # Safety
    ///
    /// - `ctx` is null or a live query context, used on its system's thread.
    pub(crate) unsafe fn entity(&self, ctx: *mut u64) -> u32 {
        if ctx.is_null() {
            return 0;
        }
        // SAFETY: `ctx` is non-null by the check before it and a live context by
        // the caller's contract.
        let row = unsafe { self.row(ctx) };
        if row == 0 {
            return 0;
        }
        // SAFETY: `ctx` is a live context by the caller's contract, and the
        // first word of a context is its query descriptor pointer.
        let descriptor = unsafe { read_ptr_at::<u8>(ctx.cast(), 0) }.cast_const();
        if descriptor.is_null() {
            return 0;
        }
        // SAFETY: `descriptor` is non-null by the check before it; its pointer to
        // the per-row entity records sits at the build table's
        // `ecs.descriptor_entity_records` offset.
        let records = unsafe { read_ptr_at::<u8>(descriptor, self.entity_records) }.cast_const();
        if records.is_null() {
            return 0;
        }
        let offset = (row as usize - 1) * self.entity_record_size;
        // SAFETY: `records` is non-null by the check before it and `row` is the
        // query's current 1-based row (non-zero by the check before it), so its
        // record at the build table's `ecs.entity_record_size` stride starts
        // with the row's u32 entity.
        unsafe { read_at(records, offset) }
    }

    /// Whether `ctx` belongs to the entity world `world`, 0 for none.
    ///
    /// # Safety
    ///
    /// - `ctx` is null or a live system context, used on its system's
    ///   thread.
    pub(crate) unsafe fn is_world(&self, ctx: *mut u64, world: u64) -> bool {
        if world == 0 || ctx.is_null() {
            return false;
        }
        // SAFETY: `ctx` is non-null by the check before it and live by the
        // caller's contract; the first word of a context is its query
        // descriptor pointer.
        let descriptor = unsafe { read_ptr_at::<u8>(ctx.cast(), 0) }.cast_const();
        if descriptor.is_null() {
            return false;
        }
        // SAFETY: `descriptor` is non-null by the check before it; its entity
        // world word sits at the build table's `ecs.context_entity_world`
        // offset.
        let own: u64 = unsafe { read_at(descriptor, self.entity_world) };
        own == world
    }

    /// Marks a written component changed, so replication sends it.
    ///
    /// # Safety
    ///
    /// - The words come from the current row of a live query whose system
    ///   writes the component.
    /// - Called on that system's thread while it runs.
    pub(crate) unsafe fn mark_changed(
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
        // SAFETY: the caller guarantees the words come from the current row
        // of a live query writing the component, on its system's thread;
        // `tracker` is non-zero by the check before it.
        unsafe { (self.mark)(tracker, stamp_a, stamp_b, entity, type_index, flags) };
    }

    /// The level attribute of the `Level` set `set`.
    ///
    /// # Safety
    ///
    /// - `set` is null or a live attribute set.
    /// - `registry` is the attribute registry word of the row `set` belongs
    ///   to.
    /// - Called on the running system's thread while it runs.
    pub(crate) unsafe fn level(&self, registry: u64, set: *const u8) -> Option<u32> {
        // SAFETY: the caller guarantees a null or live set, its row's
        // registry and the system's thread, all `attribute` asks for.
        unsafe { self.attribute(registry, set, self.level_attribute) }
    }

    /// The value of `attribute` in the attribute set `set`, looked up in its
    /// row's attribute `registry`; `None` when the set is not initialised or
    /// lacks it.
    ///
    /// # Safety
    ///
    /// - `set` is null or a live attribute set.
    /// - `registry` is the attribute registry word of the row `set` belongs
    ///   to.
    /// - Called on the running system's thread while it runs.
    pub(crate) unsafe fn attribute(
        &self,
        registry: u64,
        set: *const u8,
        attribute: u32,
    ) -> Option<u32> {
        // SAFETY: the caller guarantees a null or live set, its row's
        // registry and the system's thread, all `attribute_word` asks for.
        let word = unsafe { self.attribute_word(registry, set, attribute) }?;
        // SAFETY: `attribute_word` returned the address of the attribute's
        // u32 inside the live set's data.
        Some(unsafe { read_at(word, 0) })
    }

    /// Writes `value` to `attribute` in `set`; false when nothing was written.
    ///
    /// # Safety
    ///
    /// - `set` is null or a live attribute set that the running system
    ///   writes, and nothing else writes it during the call.
    /// - `registry` is the attribute registry word of the row `set` belongs
    ///   to.
    /// - Called on the running system's thread while it runs.
    pub(crate) unsafe fn set_attribute(
        &self,
        registry: u64,
        set: *mut u8,
        attribute: u32,
        value: u32,
    ) -> bool {
        // SAFETY: the caller guarantees a null or live set, its row's
        // registry and the system's thread, all `attribute_word` asks for.
        let Some(word) = (unsafe { self.attribute_word(registry, set, attribute) }) else {
            return false;
        };
        // SAFETY: `word` is the attribute's u32 inside the live set's data,
        // and the caller guarantees the running system alone writes the set.
        unsafe { write_at(word.cast_mut(), 0, value) };
        true
    }

    /// The address of `attribute`'s u32 inside the data of `set`.
    ///
    /// # Safety
    ///
    /// - `set` is null or a live attribute set.
    /// - `registry` is the attribute registry word of the row `set` belongs
    ///   to.
    /// - Called on the running system's thread while it runs.
    unsafe fn attribute_word(
        &self,
        registry: u64,
        set: *const u8,
        attribute: u32,
    ) -> Option<*const u8> {
        if set.is_null() {
            return None;
        }
        // SAFETY: `set` is non-null by the check before it and live by the
        // caller's contract; its flags byte sits at the build table's
        // `ecs.level_flags` offset, and bit 0 marks it initialised.
        let flags: u8 = unsafe { read_at(set, self.level_flags) };
        if flags & 1 == 0 {
            return None;
        }
        let mut found = [0u64; 2];
        // SAFETY: the caller guarantees `registry` is the row's attribute
        // registry and the system's thread; `found` holds the two words the
        // function writes.
        unsafe { (self.find)(found.as_mut_ptr(), attribute, registry) };
        let definition =
            core::ptr::with_exposed_provenance::<u8>(usize::try_from(found[1]).unwrap_or(0));
        if found[0] & 0xff != 0 || definition.is_null() {
            return None;
        }
        // SAFETY: `definition` is non-null by the check before it and is the
        // definition the engine found, which starts with its u32 set id.
        let definition_set: u32 = unsafe { read_at(definition, 0) };
        // SAFETY: `set` is non-null by the check before it and live by the
        // caller's contract; a set starts with its u32 id.
        let set_id: u32 = unsafe { read_at(set, 0) };
        if definition_set != set_id {
            return None;
        }
        // SAFETY: `definition` is the found, non-null definition; its u16
        // slot sits at the build table's `ecs.attribute_slot` offset.
        let slot = usize::from(unsafe { read_at::<u16>(definition, self.attribute_slot) });
        // SAFETY: `set` is non-null by the check before it and live by the
        // caller's contract; its u16 data offset sits at the build table's
        // `ecs.level_data_offset` offset.
        let data = usize::from(unsafe { read_at::<u16>(set, self.level_data) });
        Some(set.wrapping_add(data + slot * 4))
    }

    /// The component of type index `kind` of `entity`, read in the live
    /// context `ctx`; null when it has none.
    ///
    /// # Safety
    ///
    /// - `ctx` is a live query context, used on its system's thread.
    pub(crate) unsafe fn component(&self, ctx: *mut u64, kind: u16, entity: u32) -> *const u8 {
        // SAFETY: the caller guarantees a live context on its system's
        // thread; `kind` lives on this stack for the call.
        unsafe { (self.component_read)(ctx, &raw const kind, entity) }
    }

    /// The full zone size of an altar at `level`, `None` when the balancing
    /// data has none.
    ///
    /// # Safety
    ///
    /// - `balancing` is the balancing registry word of a live row.
    /// - Called on the running system's thread while it runs.
    pub(crate) unsafe fn zone_size(&self, balancing: u64, level: usize) -> Option<[f32; 3]> {
        let mut out = [0f32; 4];
        // SAFETY: the caller guarantees a live row's balancing registry on
        // its system's thread; `out` holds the 16 bytes the function writes,
        // three floats and the valid byte at 0xc.
        unsafe { (self.zone_size)(out.as_mut_ptr(), balancing, level as u64) };
        // SAFETY: `out` is 16 readable bytes on this stack, so the byte at
        // 0xc is inside it.
        let valid = unsafe { read_at::<u8>(out.as_ptr().cast(), 0xc) } != 0;
        valid.then_some([out[0], out[1], out[2]])
    }
}

/// A walk over a system's query; each row's iterator words.
#[derive(Debug)]
pub(crate) struct Query<'e, const N: usize> {
    engine: &'e Engine,
    ctx: *mut u64,
    words: [u64; N],
}

impl<const N: usize> Query<'_, N> {
    /// Moves to the next row and returns its words, or `None` at the end.
    pub(crate) fn next(&mut self) -> Option<&[u64; N]> {
        // SAFETY: a `Query` comes only from `Engine::query`, whose caller
        // guarantees a live context, an iterator of `N` words and a query
        // dropped before the system's run ends; `words` holds `N` writable
        // words.
        let more = unsafe { (self.engine.next)(self.ctx, self.words.as_mut_ptr(), (N * 8) as u64) };
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

/// Component reads by type hash from inside a running system, and the walk
/// from an item slot to the inventory that holds it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Components {
    type_index: TypeIndex,
    read: ReadComponent,
    slot_root: SlotRoot,
    entity_world: usize,
    registry_maps: [usize; 2],
}

impl Components {
    /// The reads on this build, if the table has them all.
    pub(crate) fn resolve(loader: &Loader) -> Option<Self> {
        let function = |name: Symbol| {
            loader
                .build
                .symbol(name.id())
                .and_then(|rva| usize::try_from(loader.base + rva).ok())
        };
        let size = |name: Constant| {
            loader
                .build
                .constant(name.id())
                .and_then(|v| usize::try_from(v).ok())
        };
        let (type_index, read, slot_root) = (
            function(pinned::ecs::TYPE_INDEX)?,
            function(pinned::ecs::COMPONENT_READ_RAW)?,
            function(pinned::ecs::SLOT_ROOT)?,
        );
        Some(Self {
            // SAFETY: the build table's `ecs.type_index` is the game function with the
            // `TypeIndex` signature on this build.
            type_index: unsafe { core::mem::transmute::<usize, TypeIndex>(type_index) },
            // SAFETY: the build table's `ecs.component_read_raw` is the game function with the
            // `ReadComponent` signature on this build.
            read: unsafe { core::mem::transmute::<usize, ReadComponent>(read) },
            // SAFETY: the build table's `ecs.slot_root` is the game function with the
            // `SlotRoot` signature on this build.
            slot_root: unsafe { core::mem::transmute::<usize, SlotRoot>(slot_root) },
            entity_world: size(pinned::ecs::CONTEXT_ENTITY_WORLD)?,
            registry_maps: [
                size(pinned::ecs::REGISTRY_TYPE_MAP)?,
                size(pinned::ecs::REGISTRY_TYPE_MAP_FALLBACK)?,
            ],
        })
    }

    /// `entity`'s component of type `hash`, or `None` when it has none.
    ///
    /// # Safety
    ///
    /// - `ctx` is null or the live context of a running system, used on its
    ///   thread.
    pub(crate) unsafe fn component(
        &self,
        ctx: *mut u64,
        entity: u32,
        hash: u32,
    ) -> Option<*mut u8> {
        if ctx.is_null() || entity == 0 {
            return None;
        }
        // SAFETY: `ctx` is non-null by the check before it and live by the
        // caller's contract; the first word of a context is its query
        // descriptor pointer.
        let descriptor = unsafe { read_ptr_at::<u8>(ctx.cast(), 0) }.cast_const();
        if descriptor.is_null() {
            return None;
        }
        // SAFETY: `descriptor` is non-null by the check before it; its entity
        // world pointer sits at the build table's `ecs.context_entity_world`
        // offset.
        let world = unsafe { read_ptr_at::<u8>(descriptor, self.entity_world) }.cast_const();
        if world.is_null() {
            return None;
        }
        // SAFETY: `world` is non-null by the check before it, and the entity
        // world starts with its component registry pointer.
        let registry = unsafe { read_ptr_at::<u8>(world, 0) }.cast_const();
        let index = self.registry_maps.iter().find_map(|&map| {
            // SAFETY: `registry` is the live world's component registry and
            // the build table's `ecs.registry_type_map` offsets name its type
            // maps; `hash` lives on this stack, and a lookup does not change
            // the maps.
            let index = unsafe { (self.type_index)(registry.wrapping_add(map), &raw const hash) };
            (!index.is_null()).then_some(index)
        })?;
        let mut out = [0usize; 2];
        // SAFETY: `index` is the non-null type index the lookup found and
        // `world` the live, non-null entity world; the caller guarantees the
        // running system's thread; `out` holds the two words the function
        // writes.
        unsafe { (self.read)(&raw mut out, world, index, entity) };
        (out[0] != 0).then(|| core::ptr::with_exposed_provenance_mut(out[0]))
    }

    /// The inventory an item slot belongs to at the root.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live context of a running system, used on its thread.
    /// - `owner_index` comes from the same system.
    pub(crate) unsafe fn slot_root(
        &self,
        ctx: *mut u64,
        slot: u64,
        owner_index: u64,
    ) -> Option<u32> {
        let mut out = [0u64; 2];
        // SAFETY: the caller guarantees a live context on its system's
        // thread and an `owner_index` from that system; `out` holds the two
        // words the function writes.
        unsafe { (self.slot_root)(&raw mut out, slot, owner_index, ctx) };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the error is the low byte, the root a u32"
        )]
        let (error, root) = (out[0] as u8, out[1] as u32);
        (error == 0 && root != 0).then_some(root)
    }
}
