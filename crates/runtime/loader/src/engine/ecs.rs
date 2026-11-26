//! The engine helpers the loader's own links use: walking a system's query,
//! the current row's entity, change marks, attribute reads, component reads
//! and zone sizes.

use std::sync::OnceLock;

use enshwick_core::Entity;
use enshwick_core::raw::{read_at, read_ptr_at, write_at};

use crate::engine::loader::Loader;
use crate::engine::pinned;
use crate::engine::pinned::{Constant, Symbol};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::altar::LEVEL_ATTRIBUTE.id(),
    pinned::balancing::ZONE_SIZE.id(),
    pinned::balancing::ZONE_SIZE_VALID.id(),
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

#[cfg(test)]
pub(crate) mod double;
#[cfg(test)]
mod tests;

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
) -> Result<(), MatchComponentsError> {
    let found =
        loader
            .reflection
            .system(system)
            .map_err(|source| MatchComponentsError::NoSystem {
                system: system.to_owned(),
                source,
            })?;
    if found.reads() == reads && found.writes() == writes {
        Ok(())
    } else {
        let owned = |names: &[&str]| names.iter().map(|&n| n.to_owned()).collect();
        Err(MatchComponentsError::Changed {
            system: system.to_owned(),
            reads: owned(found.reads()),
            writes: owned(found.writes()),
        })
    }
}

/// Why a system no longer fits the iterator layout the loader has for it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum MatchComponentsError {
    /// The build's reflection has no such system.
    #[error("no system {system}: {source}")]
    NoSystem {
        system: String,
        #[source]
        source: enshwick_gamedata::reflect::LookupError,
    },
    /// The system reads or writes other components.
    #[error(
        "{system} reads {reads:?} and writes {writes:?}, not the components its iterator layout assumes"
    )]
    Changed {
        system: String,
        reads: Vec<String>,
        writes: Vec<String>,
    },
}

/// The helpers on this build, resolved once; `None` when the table lacks one.
pub(crate) fn engine(loader: &Loader) -> Option<&'static Engine> {
    engine_found(loader).ok()
}

/// The helpers on this build, resolved once, or the id of the first table
/// entry it lacks, for a line that names it.
pub(crate) fn engine_found(loader: &Loader) -> Result<&'static Engine, &'static str> {
    if let Some(engine) = ENGINE.get() {
        return Ok(engine);
    }
    let engine = Engine::found(loader)?;
    Ok(ENGINE.get_or_init(|| engine))
}

/// The query walk on this build, resolved once, or the id of the first
/// table entry it lacks: for a feature that only walks a system's query,
/// which then does not turn off with an unrelated helper.
pub(crate) fn walk(loader: &Loader) -> Result<&'static Walk, &'static str> {
    if let Some(walk) = WALK.get() {
        return Ok(walk);
    }
    let walk = Walk::found(loader)?;
    Ok(WALK.get_or_init(|| walk))
}

/// The query walk once resolved, alone or with the other helpers.
pub(crate) fn walked() -> Option<&'static Walk> {
    WALK.get()
        .or_else(|| ENGINE.get().map(|engine| &engine.walk))
}

static WALK: OnceLock<Walk> = OnceLock::new();

/// The function at table symbol `name` on this build, or its id.
fn function(loader: &Loader, name: Symbol) -> Result<usize, &'static str> {
    loader
        .build
        .symbol(name.id())
        .and_then(|rva| usize::try_from(loader.base + rva).ok())
        .ok_or(name.id())
}

/// The table constant `name` as a size on this build, or its id.
fn size(loader: &Loader, name: Constant) -> Result<usize, &'static str> {
    loader
        .build
        .constant(name.id())
        .and_then(|v| usize::try_from(v).ok())
        .ok_or(name.id())
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
/// The answer the zone size writes: three floats and its valid byte after
/// them, 16 bytes in all (server 399, client 426).
type ZoneSizeOut = [f32; 4];
type ComponentRead =
    unsafe extern "system" fn(ctx: *mut u64, kind: *const u16, entity: u32) -> *const u8;

/// The walk over a running system's query: its iterator, row counter and
/// row entities, all a feature that only reads rows needs.
#[derive(Debug)]
pub(crate) struct Walk {
    init: IteratorInit,
    next: IteratorNext,
    context_row: usize,
    entity_records: usize,
    entity_record_size: usize,
}

impl Walk {
    /// The walk on this build, or the id of the first table entry it lacks.
    pub(crate) fn found(loader: &Loader) -> Result<Self, &'static str> {
        let (init, next) = (
            function(loader, pinned::ecs::ITERATOR_INIT)?,
            function(loader, pinned::ecs::ITERATOR_NEXT)?,
        );
        Ok(Self {
            // SAFETY: the build table's `ecs.iterator_init` is the game function with the
            // `IteratorInit` signature on this build.
            init: unsafe { core::mem::transmute::<usize, IteratorInit>(init) },
            // SAFETY: the build table's `ecs.iterator_next` is the game function with the
            // `IteratorNext` signature on this build.
            next: unsafe { core::mem::transmute::<usize, IteratorNext>(next) },
            context_row: size(loader, pinned::ecs::CONTEXT_ROW)?,
            entity_records: size(loader, pinned::ecs::DESCRIPTOR_ENTITY_RECORDS)?,
            entity_record_size: size(loader, pinned::ecs::ENTITY_RECORD_SIZE)?,
        })
    }

    /// A walk over test functions and a context laid out at these offsets,
    /// as a running system's would be.
    #[cfg(test)]
    pub(crate) const fn for_test(
        init: IteratorInit,
        next: IteratorNext,
        context_row: usize,
        entity_records: usize,
        entity_record_size: usize,
    ) -> Self {
        Self {
            init,
            next,
            context_row,
            entity_records,
            entity_record_size,
        }
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
            walk: self,
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
}

/// The engine functions and layout on the running build.
#[derive(Debug)]
pub(crate) struct Engine {
    walk: Walk,
    mark: MarkChanged,
    find: AttributeFind,
    zone_size: ZoneSize,
    /// Where the zone size's answer holds its valid byte, inside its
    /// [`ZoneSizeOut`].
    zone_size_valid: usize,
    component_read: ComponentRead,
    entity_world: usize,
    level_flags: usize,
    level_data: usize,
    attribute_slot: usize,
    level_attribute: u32,
}

impl Engine {
    /// The helpers on this build, or the id of the first table entry it
    /// lacks.
    pub(crate) fn found(loader: &Loader) -> Result<Self, &'static str> {
        let walk = Walk::found(loader)?;
        let (mark, find, zone_size, component_read) = (
            function(loader, pinned::ecs::MARK_CHANGED)?,
            function(loader, pinned::ecs::ATTRIBUTE_FIND)?,
            function(loader, pinned::balancing::ZONE_SIZE)?,
            function(loader, pinned::ecs::COMPONENT_READ)?,
        );
        let level_attribute = pinned::altar::LEVEL_ATTRIBUTE;
        Ok(Self {
            walk,
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
            entity_world: size(loader, pinned::ecs::CONTEXT_ENTITY_WORLD)?,
            level_flags: size(loader, pinned::ecs::LEVEL_FLAGS)?,
            level_data: size(loader, pinned::ecs::LEVEL_DATA_OFFSET)?,
            attribute_slot: size(loader, pinned::ecs::ATTRIBUTE_SLOT)?,
            zone_size_valid: size(loader, pinned::balancing::ZONE_SIZE_VALID)
                .ok()
                .filter(|&at| at < size_of::<ZoneSizeOut>())
                .ok_or(pinned::balancing::ZONE_SIZE_VALID.id())?,
            level_attribute: loader
                .build
                .constant(level_attribute.id())
                .and_then(|v| u32::try_from(v).ok())
                .ok_or(level_attribute.id())?,
        })
    }

    /// [`Walk::query`].
    ///
    /// # Safety
    ///
    /// As for [`Walk::query`].
    pub(crate) unsafe fn query<const N: usize>(&self, ctx: *mut u64) -> Query<'_, N> {
        // SAFETY: the caller's contract is `Walk::query`'s.
        unsafe { self.walk.query(ctx) }
    }

    /// [`Walk::row`].
    ///
    /// # Safety
    ///
    /// As for [`Walk::row`].
    pub(crate) unsafe fn row(&self, ctx: *mut u64) -> u32 {
        // SAFETY: the caller's contract is `Walk::row`'s.
        unsafe { self.walk.row(ctx) }
    }

    /// [`Walk::set_row`].
    ///
    /// # Safety
    ///
    /// As for [`Walk::set_row`].
    pub(crate) unsafe fn set_row(&self, ctx: *mut u64, row: u32) {
        // SAFETY: the caller's contract is `Walk::set_row`'s.
        unsafe { self.walk.set_row(ctx, row) };
    }

    /// [`Walk::entity`].
    ///
    /// # Safety
    ///
    /// As for [`Walk::entity`].
    pub(crate) unsafe fn entity(&self, ctx: *mut u64) -> u32 {
        // SAFETY: the caller's contract is `Walk::entity`'s.
        unsafe { self.walk.entity(ctx) }
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

    /// Writes `level` to the level attribute of the `Level` set `set`; false
    /// when nothing was written.
    ///
    /// # Safety
    ///
    /// As for [`Engine::set_attribute`].
    pub(crate) unsafe fn set_level(&self, registry: u64, set: *mut u8, level: u32) -> bool {
        // SAFETY: the caller's contract is `set_attribute`'s.
        unsafe { self.set_attribute(registry, set, self.level_attribute, level) }
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
    pub(crate) unsafe fn component(&self, ctx: *mut u64, kind: u16, entity: Entity) -> *const u8 {
        // SAFETY: the caller guarantees a live context on its system's
        // thread; `kind` lives on this stack for the call.
        unsafe { (self.component_read)(ctx, &raw const kind, entity.get()) }
    }

    /// The full zone size of an altar at `level`, `None` when the balancing
    /// data has none.
    ///
    /// # Safety
    ///
    /// - `balancing` is the balancing registry word of a live row.
    /// - Called on the running system's thread while it runs.
    pub(crate) unsafe fn zone_size(&self, balancing: u64, level: usize) -> Option<[f32; 3]> {
        let mut out: ZoneSizeOut = [0f32; 4];
        // SAFETY: the caller guarantees a live row's balancing registry on
        // its system's thread; `out` holds the 16 bytes the function writes,
        // three floats and the valid byte (server 399, client 426).
        unsafe { (self.zone_size)(out.as_mut_ptr(), balancing, level as u64) };
        // SAFETY: `out` is readable on this stack, and `resolve` keeps the
        // build table's valid byte inside it.
        let valid = unsafe { read_at::<u8>(out.as_ptr().cast(), self.zone_size_valid) } != 0;
        valid.then_some([out[0], out[1], out[2]])
    }
}

/// A walk over a system's query; each row's iterator words.
#[derive(Debug)]
pub(crate) struct Query<'e, const N: usize> {
    walk: &'e Walk,
    ctx: *mut u64,
    words: [u64; N],
}

impl<const N: usize> Query<'_, N> {
    /// Moves to the next row and returns its words, or `None` at the end.
    pub(crate) fn next(&mut self) -> Option<&[u64; N]> {
        // SAFETY: a `Query` comes only from `Walk::query`, whose caller
        // guarantees a live context, an iterator of `N` words and a query
        // dropped before the system's run ends; `words` holds `N` writable
        // words.
        let more = unsafe { (self.walk.next)(self.ctx, self.words.as_mut_ptr(), (N * 8) as u64) };
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

/// Where a component read inside a running system found the data.
#[derive(Debug, Clone, Copy)]
enum Found {
    /// The static half of the type, the template's set-up data.
    Static(*const u8),
    /// The entity's own data, the dynamic half.
    Dynamic(*mut u8),
}

/// Component reads by type hash from inside a running system, and the walk
/// from an item slot to the inventory that holds it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Components {
    type_index: TypeIndex,
    read: ReadComponent,
    slot_root: SlotRoot,
    entity_world: usize,
    registry_maps: [usize; 2],
    /// The static reads, on builds whose table has them.
    halves: Option<crate::engine::static_halves::StaticHalves>,
}

impl Components {
    /// The reads on this build, if the table has them all.
    pub(crate) fn resolve(loader: &Loader) -> Option<Self> {
        Self::found(loader).ok()
    }

    /// The reads on this build, or the id of the first table entry it
    /// lacks, for a report that names it.
    pub(crate) fn found(loader: &Loader) -> Result<Self, &'static str> {
        let function = |name: Symbol| function(loader, name);
        let size = |name: Constant| size(loader, name);
        let (type_index, read, slot_root) = (
            function(pinned::ecs::TYPE_INDEX)?,
            function(pinned::ecs::COMPONENT_READ_RAW)?,
            function(pinned::ecs::SLOT_ROOT)?,
        );
        Ok(Self {
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
            halves: crate::engine::static_halves::halves(loader).copied(),
        })
    }

    /// `entity`'s component of type `hash`, read-only, or `None` when it
    /// has none: for the static half of an ECS type, the set-up data its
    /// template holds.
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
    ) -> Option<*const u8> {
        // SAFETY: the caller's contract is `find`'s.
        match unsafe { self.find(ctx, entity, hash) }? {
            Found::Static(data) => Some(data),
            Found::Dynamic(data) => Some(data.cast_const()),
        }
    }

    /// `entity`'s own component of type `hash` for a write, or `None` when it
    /// has none or `hash` names the static half of its type, which belongs
    /// to the template.
    ///
    /// # Safety
    ///
    /// - `ctx` is null or the live context of a running system, used on its
    ///   thread.
    pub(crate) unsafe fn component_mut(
        &self,
        ctx: *mut u64,
        entity: u32,
        hash: u32,
    ) -> Option<*mut u8> {
        // SAFETY: the caller's contract is `find`'s.
        match unsafe { self.find(ctx, entity, hash) }? {
            Found::Static(_) => None,
            Found::Dynamic(data) => Some(data),
        }
    }

    /// The entity world a running system's context reads, or `None` for a
    /// null context or one without a world.
    ///
    /// # Safety
    ///
    /// - `ctx` is null or the live context of a running system, used on its
    ///   thread.
    pub(crate) unsafe fn world(&self, ctx: *mut u64) -> Option<usize> {
        if ctx.is_null() {
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
        let world = unsafe { read_ptr_at::<u8>(descriptor, self.entity_world) };
        (!world.is_null()).then(|| world.expose_provenance())
    }

    /// Where `entity`'s component of type `hash` is, or `None` when it has
    /// none.
    ///
    /// # Safety
    ///
    /// - `ctx` is null or the live context of a running system, used on its
    ///   thread.
    unsafe fn find(&self, ctx: *mut u64, entity: u32, hash: u32) -> Option<Found> {
        if entity == 0 {
            return None;
        }
        // SAFETY: the caller's contract is `world`'s.
        let world = unsafe { self.world(ctx) }?;
        // SAFETY: `world` is the live entity world of the running system's
        // context, read on its thread (the caller's contract).
        unsafe { self.find_in(world, entity, hash) }
    }

    /// `entity`'s component of type `hash` in the entity world `world`,
    /// read-only, or `None` when it has none: for a system whose context
    /// does not name the world its rows belong to.
    ///
    /// # Safety
    ///
    /// - `world` is 0 or a live entity world, read on the thread of one of
    ///   its running systems or the game thread, while no structural change
    ///   of the world runs.
    pub(crate) unsafe fn component_in(
        &self,
        world: usize,
        entity: u32,
        hash: u32,
    ) -> Option<*const u8> {
        if entity == 0 || world == 0 {
            return None;
        }
        // SAFETY: the caller's contract is `find_in`'s.
        match unsafe { self.find_in(world, entity, hash) }? {
            Found::Static(data) => Some(data),
            Found::Dynamic(data) => Some(data.cast_const()),
        }
    }

    /// Where `entity`'s component of type `hash` is in the entity world
    /// `world`, or `None` when it has none.
    ///
    /// # Safety
    ///
    /// - `world` is a non-zero live entity world, read on the thread of one
    ///   of its running systems or the game thread.
    unsafe fn find_in(&self, world: usize, entity: u32, hash: u32) -> Option<Found> {
        let world: *const u8 = core::ptr::with_exposed_provenance(world);
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
        if let Some(halves) = &self.halves
            && !registry.is_null()
            // SAFETY: `registry` is the running system's live entity world's
            // non-null component registry and `index` the non-null type index
            // the lookup found for `hash`; the caller guarantees the system's
            // thread.
            && unsafe { halves.is_static(registry.expose_provenance(), index, hash) }
        {
            // SAFETY: `world` is the running system's live entity world and
            // `index` the type index of `hash`, whose type has a static half
            // (`is_static` returned true); the caller guarantees the
            // system's thread.
            let data = unsafe { halves.read(world.expose_provenance(), index, entity) };
            return data.ok().map(Found::Static);
        }
        let mut out = [0usize; 2];
        // SAFETY: `index` is the non-null type index the lookup found and
        // `world` the live, non-null entity world; the caller guarantees the
        // running system's thread; `out` holds the two words the function
        // writes.
        unsafe { (self.read)(&raw mut out, world, index, entity) };
        (out[0] != 0).then(|| Found::Dynamic(core::ptr::with_exposed_provenance_mut(out[0])))
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
