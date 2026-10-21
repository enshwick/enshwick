//! The engine helpers the loader's own links use: walking a system's query,
//! the current row's entity, change marks, attribute reads, component reads
//! and zone sizes.

use std::sync::OnceLock;

use crate::host::Loader;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "altar.level_attribute",
    "balancing.zone_size",
    "ecs.attribute_find",
    "ecs.attribute_slot",
    "ecs.component_read",
    "ecs.component_read_raw",
    "ecs.context_entity_world",
    "ecs.context_row",
    "ecs.descriptor_entity_records",
    "ecs.entity_record_size",
    "ecs.iterator_init",
    "ecs.iterator_next",
    "ecs.level_data_offset",
    "ecs.level_flags",
    "ecs.mark_changed",
    "ecs.registry_type_map",
    "ecs.registry_type_map_fallback",
    "ecs.slot_root",
    "ecs.type_index",
];

static ENGINE: OnceLock<Engine> = OnceLock::new();

/// The helpers on this build, resolved once; `None` when the table lacks one.
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

/// Reads a `T` at `offset` bytes past `base`.
///
/// # Safety
///
/// A readable `T` lies there.
pub(crate) unsafe fn read_at<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

impl Engine {
    /// The helpers on this build, if the table has them all.
    pub(crate) fn resolve(loader: &Loader) -> Option<Self> {
        let function = |name: &str| {
            loader
                .build
                .symbol(name)
                .and_then(|rva| usize::try_from(loader.base + rva).ok())
        };
        let size = |name: &str| {
            loader
                .build
                .constant(name)
                .and_then(|v| usize::try_from(v).ok())
        };
        let (init, next, mark, find, zone_size, component_read) = (
            function("ecs.iterator_init")?,
            function("ecs.iterator_next")?,
            function("ecs.mark_changed")?,
            function("ecs.attribute_find")?,
            function("balancing.zone_size")?,
            function("ecs.component_read")?,
        );
        Some(Self {
            // SAFETY: each symbol is the game function with this signature
            // on this build.
            init: unsafe { core::mem::transmute::<usize, IteratorInit>(init) },
            // SAFETY: as above.
            next: unsafe { core::mem::transmute::<usize, IteratorNext>(next) },
            // SAFETY: as above.
            mark: unsafe { core::mem::transmute::<usize, MarkChanged>(mark) },
            // SAFETY: as above.
            find: unsafe { core::mem::transmute::<usize, AttributeFind>(find) },
            // SAFETY: as above.
            zone_size: unsafe { core::mem::transmute::<usize, ZoneSize>(zone_size) },
            // SAFETY: as above.
            component_read: unsafe { core::mem::transmute::<usize, ComponentRead>(component_read) },
            context_row: size("ecs.context_row")?,
            entity_records: size("ecs.descriptor_entity_records")?,
            entity_record_size: size("ecs.entity_record_size")?,
            entity_world: size("ecs.context_entity_world")?,
            level_flags: size("ecs.level_flags")?,
            level_data: size("ecs.level_data_offset")?,
            attribute_slot: size("ecs.attribute_slot")?,
            level_attribute: u32::try_from(loader.build.constant("altar.level_attribute")?).ok()?,
        })
    }

    /// Walks the system's query from its first row.
    ///
    /// # Safety
    ///
    /// `ctx` is the live query context of the hooked system and `N` matches
    /// its iterator.
    pub(crate) unsafe fn query<const N: usize>(&self, ctx: *mut u64) -> Query<'_, N> {
        let mut words = [0u64; N];
        // SAFETY: the caller passes a live context and a matching size.
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
    /// `ctx` is a live query context.
    pub(crate) unsafe fn row(&self, ctx: *mut u64) -> u32 {
        // SAFETY: the row counter lies inside a live context.
        unsafe { read_at(ctx.cast(), self.context_row) }
    }

    /// Moves the query's row counter.
    ///
    /// # Safety
    ///
    /// `ctx` is a live query context.
    pub(crate) unsafe fn set_row(&self, ctx: *mut u64, row: u32) {
        // SAFETY: the row counter lies inside a live context.
        unsafe {
            ctx.cast::<u8>()
                .wrapping_add(self.context_row)
                .cast::<u32>()
                .write_unaligned(row);
        }
    }

    /// The entity of the query's current row, 0 before the first or for a
    /// null context.
    ///
    /// # Safety
    ///
    /// `ctx` is null or a live query context.
    pub(crate) unsafe fn entity(&self, ctx: *mut u64) -> u32 {
        if ctx.is_null() {
            return 0;
        }
        // SAFETY: forwarded from the caller.
        let row = unsafe { self.row(ctx) };
        if row == 0 {
            return 0;
        }
        // SAFETY: the first word of a context is its query descriptor.
        let descriptor: *const u8 = unsafe { read_at(ctx.cast(), 0) };
        if descriptor.is_null() {
            return 0;
        }
        // SAFETY: the descriptor holds a pointer to the per-row entity records.
        let records: *const u8 = unsafe { read_at(descriptor, self.entity_records) };
        if records.is_null() {
            return 0;
        }
        let offset = (row as usize - 1) * self.entity_record_size;
        // SAFETY: `row` is a valid 1-based row of this query.
        unsafe { read_at(records, offset) }
    }

    /// Whether `ctx` belongs to the entity world `world`, 0 for none.
    ///
    /// # Safety
    ///
    /// `ctx` is a live system context.
    pub(crate) unsafe fn is_world(&self, ctx: *mut u64, world: u64) -> bool {
        if world == 0 || ctx.is_null() {
            return false;
        }
        // SAFETY: the first word of a context is its query descriptor.
        let descriptor: *const u8 = unsafe { read_at(ctx.cast(), 0) };
        if descriptor.is_null() {
            return false;
        }
        // SAFETY: the descriptor points at its entity world here.
        let own: u64 = unsafe { read_at(descriptor, self.entity_world) };
        own == world
    }

    /// Marks a written component changed, so replication sends it.
    ///
    /// # Safety
    ///
    /// The words come from the current row of a live query that writes the
    /// component.
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
        // SAFETY: forwarded from the caller.
        unsafe { (self.mark)(tracker, stamp_a, stamp_b, entity, type_index, flags) };
    }

    /// The level attribute of the `Level` set `set`.
    ///
    /// # Safety
    ///
    /// As for [`Engine::attribute`].
    pub(crate) unsafe fn level(&self, registry: u64, set: *const u8) -> Option<u32> {
        // SAFETY: forwarded from the caller.
        unsafe { self.attribute(registry, set, self.level_attribute) }
    }

    /// The value of `attribute` in the attribute set `set`, looked up in its
    /// row's attribute `registry`; `None` when the set is not initialised or
    /// lacks it.
    ///
    /// # Safety
    ///
    /// `set` is a live attribute set and `registry` its row's registry word,
    /// inside the running system.
    pub(crate) unsafe fn attribute(
        &self,
        registry: u64,
        set: *const u8,
        attribute: u32,
    ) -> Option<u32> {
        // SAFETY: forwarded from the caller.
        let word = unsafe { self.attribute_word(registry, set, attribute) }?;
        // SAFETY: the attribute's word inside the set's data.
        Some(unsafe { read_at(word, 0) })
    }

    /// Writes `value` to `attribute` in `set`; false when nothing was written.
    ///
    /// # Safety
    ///
    /// As for [`Engine::attribute`], and the running system writes `set`.
    pub(crate) unsafe fn set_attribute(
        &self,
        registry: u64,
        set: *mut u8,
        attribute: u32,
        value: u32,
    ) -> bool {
        // SAFETY: forwarded from the caller.
        let Some(word) = (unsafe { self.attribute_word(registry, set, attribute) }) else {
            return false;
        };
        // SAFETY: the attribute's word inside a set the system writes.
        unsafe { word.cast_mut().cast::<u32>().write_unaligned(value) };
        true
    }

    /// # Safety
    ///
    /// As for [`Engine::attribute`].
    unsafe fn attribute_word(
        &self,
        registry: u64,
        set: *const u8,
        attribute: u32,
    ) -> Option<*const u8> {
        if set.is_null() {
            return None;
        }
        // SAFETY: the set's flags; bit 0 marks it initialised.
        let flags: u8 = unsafe { read_at(set, self.level_flags) };
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
        let slot = usize::from(unsafe { read_at::<u16>(definition, self.attribute_slot) });
        // SAFETY: the set's data offset.
        let data = usize::from(unsafe { read_at::<u16>(set, self.level_data) });
        Some(set.wrapping_add(data + slot * 4))
    }

    /// The component of type index `kind` of `entity`, read in the live
    /// context `ctx`; null when it has none.
    ///
    /// # Safety
    ///
    /// `ctx` is a live query context.
    pub(crate) unsafe fn component(&self, ctx: *mut u64, kind: u16, entity: u32) -> *const u8 {
        // SAFETY: forwarded from the caller.
        unsafe { (self.component_read)(ctx, &raw const kind, entity) }
    }

    /// The full zone size of an altar at `level`, `None` when the balancing
    /// data has none.
    ///
    /// # Safety
    ///
    /// `balancing` is the balancing registry word of a live row.
    pub(crate) unsafe fn zone_size(&self, balancing: u64, level: usize) -> Option<[f32; 3]> {
        let mut out = [0f32; 4];
        // SAFETY: the row's balancing registry; `out` has room for the three
        // floats and the valid byte.
        unsafe { (self.zone_size)(out.as_mut_ptr(), balancing, level as u64) };
        // SAFETY: the valid byte at 0xc of `out`.
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
        // SAFETY: the query was created from a live context and a matching size.
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
        let function = |name: &str| {
            loader
                .build
                .symbol(name)
                .and_then(|rva| usize::try_from(loader.base + rva).ok())
        };
        let size = |name: &str| {
            loader
                .build
                .constant(name)
                .and_then(|v| usize::try_from(v).ok())
        };
        let (type_index, read, slot_root) = (
            function("ecs.type_index")?,
            function("ecs.component_read_raw")?,
            function("ecs.slot_root")?,
        );
        Some(Self {
            // SAFETY: each symbol is the game function with this signature
            // on this build.
            type_index: unsafe { core::mem::transmute::<usize, TypeIndex>(type_index) },
            // SAFETY: as above.
            read: unsafe { core::mem::transmute::<usize, ReadComponent>(read) },
            // SAFETY: as above.
            slot_root: unsafe { core::mem::transmute::<usize, SlotRoot>(slot_root) },
            entity_world: size("ecs.context_entity_world")?,
            registry_maps: [
                size("ecs.registry_type_map")?,
                size("ecs.registry_type_map_fallback")?,
            ],
        })
    }

    /// `entity`'s component of type `hash`, or `None` when it has none.
    ///
    /// # Safety
    ///
    /// `ctx` is the live context of a running system, used on its thread.
    pub(crate) unsafe fn component(
        &self,
        ctx: *mut u64,
        entity: u32,
        hash: u32,
    ) -> Option<*mut u8> {
        if ctx.is_null() || entity == 0 {
            return None;
        }
        // SAFETY: the first word of a context is its query descriptor, which
        // points at the entity world.
        let descriptor: *const u8 = unsafe { read_at(ctx.cast(), 0) };
        if descriptor.is_null() {
            return None;
        }
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
        unsafe { (self.read)(&raw mut out, world, index, entity) };
        (out[0] != 0).then(|| core::ptr::with_exposed_provenance_mut(out[0]))
    }

    /// The inventory an item slot belongs to at the root.
    ///
    /// # Safety
    ///
    /// As for [`Components::component`]; `owner_index` comes from the same
    /// system.
    pub(crate) unsafe fn slot_root(
        &self,
        ctx: *mut u64,
        slot: u64,
        owner_index: u64,
    ) -> Option<u32> {
        let mut out = [0u64; 2];
        // SAFETY: the game's walk up `OwnerRelationship`, inside its system.
        unsafe { (self.slot_root)(&raw mut out, slot, owner_index, ctx) };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the error is the low byte, the root a u32"
        )]
        let (error, root) = (out[0] as u8, out[1] as u32);
        (error == 0 && root != 0).then_some(root)
    }
}
