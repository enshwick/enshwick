//! Weather and the loading of the world's chunks, in the server's world.
//!
//! The server keeps its weather outside the ECS, in its `WeatherSystem`: a
//! few layers, each with the time its next phenomenon comes and handles to
//! the current and the previous phenomenon's instance (a front of rain or a
//! blizzard with a place and a radius, or nothing). Every frame its update
//! picks a phenomenon for a layer whose time has come; the add and the
//! removal of instances go to every client, and the save stores the
//! layers. The game's own sampler weighs, for a place, how clear, rainy,
//! snowy or stormy it is there, as its systems ask it.
//!
//! The loader changes the weather as the game's state configurations do:
//! it frees a layer's instances and empties its slot, so the next update
//! picks anew; to force one, it then spawns the layer's phenomenon of the
//! effect asked, or its nothing, for as long as asked.
//!
//! The world loads in square chunks around the players: one function moves
//! one queued chunk on, from loading to loaded once its entities exist, and
//! from unloading to unloaded once they are gone. The loader wraps it and
//! reports the turns of the server's world after the step.

use crate::pinned::{self, Symbol};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{ChunkData, Event, Header, HookFlags, Status, WeatherFront};
use enshwick::raw::read_at;

use crate::chains::Next;
use crate::host::Loader;
use crate::locks::Relock;
use crate::memory::{read, write};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::chunks::HEIGHT.id(),
    pinned::chunks::INDEX.id(),
    pinned::chunks::MANAGER.id(),
    pinned::chunks::STEP.id(),
    pinned::chunks::WIDTH.id(),
    pinned::server::PLAYER_INFOS.id(),
    pinned::weather::CLOCK.id(),
    pinned::weather::ENTRY_DATA.id(),
    pinned::weather::ENTRY_GENERATION.id(),
    pinned::weather::ENTRY_SIZE.id(),
    pinned::weather::FREE.id(),
    pinned::weather::GENERATION.id(),
    pinned::weather::INSTANCE_DESPAWN.id(),
    pinned::weather::INSTANCE_PHENOMENON.id(),
    pinned::weather::INSTANCE_POSITION.id(),
    pinned::weather::INSTANCE_RADIUS.id(),
    pinned::weather::LAYER_COUNT.id(),
    pinned::weather::LAYERS.id(),
    pinned::weather::MANAGER.id(),
    pinned::weather::PAGE_COUNT.id(),
    pinned::weather::PAGE_ENTRIES.id(),
    pinned::weather::PAGE_ENTRY_COUNT.id(),
    pinned::weather::PAGE_MASK.id(),
    pinned::weather::PAGE_SHIFT.id(),
    pinned::weather::PAGE_SIZE.id(),
    pinned::weather::PAGES.id(),
    pinned::weather::RESOURCE.id(),
    pinned::weather::RESOURCE_LOCK.id(),
    pinned::weather::RESOURCE_RELEASE.id(),
    pinned::weather::SAMPLE.id(),
    pinned::weather::SLOT_CURRENT.id(),
    pinned::weather::SLOT_NEXT.id(),
    pinned::weather::SLOT_PHENOMENON.id(),
    pinned::weather::SLOT_SIZE.id(),
    pinned::weather::SLOT_TIME.id(),
    pinned::weather::SPAWN.id(),
    pinned::weather::SYSTEM.id(),
];

/// The effect value that asks the game to choose.
pub(crate) const ANY: u32 = u32::MAX;
/// The side of a chunk, in metres.
const CHUNK_METRES: u32 = 256;
/// A chunk's states.
const LOADING: u8 = 1;
const LOADED: u8 = 2;
const UNLOADING: u8 = 3;
const UNLOADED: u8 = 0;

type SampleFn = unsafe extern "system" fn(
    out: *mut [f32; 4],
    weather: *const u8,
    position: *const [i64; 3],
    now: i64,
) -> *mut [f32; 4];
type ClockFn = unsafe extern "system" fn(game: *const u8) -> i64;
type LockFn = unsafe extern "system" fn(cache: *mut [u64; 4], manager: u64, handle: u32) -> u8;
type ReleaseFn = unsafe extern "system" fn(manager: u64, handle: u32);
type FreeFn = unsafe extern "system" fn(weather: *mut u8, handle: u32);
type SpawnFn = unsafe extern "system" fn(
    weather: *mut u8,
    resource: *const u8,
    now: i64,
    layer: u64,
    phenomenon: u8,
    duration: *const [u64; 2],
);
type ChunkFn = unsafe extern "system" fn(entities: *mut u8, chunk: *mut u8, budget: *mut u32) -> u8;

/// Where everything lies, from the build table and reflection.
#[derive(Debug)]
struct Layout {
    sample: SampleFn,
    clock: ClockFn,
    lock: LockFn,
    release: ReleaseFn,
    free: FreeFn,
    spawn: SpawnFn,
    system: usize,
    game: usize,
    manager: usize,
    resource: usize,
    generation: usize,
    pages: usize,
    page_count: usize,
    shift: usize,
    mask: usize,
    /// A page descriptor of the instance pool: its size, its entries and
    /// their number.
    page_size: usize,
    page_entries: usize,
    page_entry_count: usize,
    /// An entry of a page: its size, its generation and its data.
    entry_size: usize,
    entry_generation: usize,
    entry_data: usize,
    layers: usize,
    layer_count: usize,
    /// A layer slot: its size, the time of its next phenomenon, its last
    /// phenomenon and the handles of its current and next instances.
    slot_size: usize,
    slot_time: usize,
    slot_phenomenon: usize,
    slot_current: usize,
    slot_next: usize,
    instance_phenomenon: usize,
    instance_position: usize,
    instance_radius: usize,
    instance_despawn: usize,
    resource_layers: usize,
    resource_phenomena: usize,
    layer_size: usize,
    layer_range: usize,
    phenomenon_size: usize,
    phenomenon_effect: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let reflection = &loader.reflection;
            let field = |ty: &str, name: &str| {
                reflection
                    .field_offset(ty, name)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let size = |ty: &str| {
                reflection
                    .type_info(ty)
                    .ok()
                    .and_then(|info| usize::try_from(info.size()).ok())
            };
            let function = |name: Symbol| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let resource = "keen::WeatherSystemResource";
            let layer = "keen::WeatherSystemLayerResource";
            let phenomenon = "keen::WeatherSystemPhenomenonResource";
            let (sample, clock, lock, release, free, spawn) = (
                function(pinned::weather::SAMPLE)?,
                function(pinned::weather::CLOCK)?,
                function(pinned::weather::RESOURCE_LOCK)?,
                function(pinned::weather::RESOURCE_RELEASE)?,
                function(pinned::weather::FREE)?,
                function(pinned::weather::SPAWN)?,
            );
            Some(Layout {
                // SAFETY: the build table's `weather.sample` is the game function with
                // the `SampleFn` signature on this build.
                sample: unsafe { core::mem::transmute::<usize, SampleFn>(sample) },
                // SAFETY: the build table's `weather.clock` is the game function with
                // the `ClockFn` signature on this build.
                clock: unsafe { core::mem::transmute::<usize, ClockFn>(clock) },
                // SAFETY: the build table's `weather.resource_lock` is the game function with
                // the `LockFn` signature on this build.
                lock: unsafe { core::mem::transmute::<usize, LockFn>(lock) },
                // SAFETY: the build table's `weather.resource_release` is the game function with
                // the `ReleaseFn` signature on this build.
                release: unsafe { core::mem::transmute::<usize, ReleaseFn>(release) },
                // SAFETY: the build table's `weather.free` is the game function with
                // the `FreeFn` signature on this build.
                free: unsafe { core::mem::transmute::<usize, FreeFn>(free) },
                // SAFETY: the build table's `weather.spawn` is the game function with
                // the `SpawnFn` signature on this build.
                spawn: unsafe { core::mem::transmute::<usize, SpawnFn>(spawn) },
                system: loader.offset(pinned::weather::SYSTEM)?,
                game: loader.offset(pinned::server::PLAYER_INFOS)?,
                manager: loader.offset(pinned::weather::MANAGER)?,
                resource: loader.offset(pinned::weather::RESOURCE)?,
                generation: loader.offset(pinned::weather::GENERATION)?,
                pages: loader.offset(pinned::weather::PAGES)?,
                page_count: loader.offset(pinned::weather::PAGE_COUNT)?,
                shift: loader.offset(pinned::weather::PAGE_SHIFT)?,
                mask: loader.offset(pinned::weather::PAGE_MASK)?,
                page_size: loader.offset(pinned::weather::PAGE_SIZE)?,
                page_entries: loader.offset(pinned::weather::PAGE_ENTRIES)?,
                page_entry_count: loader.offset(pinned::weather::PAGE_ENTRY_COUNT)?,
                entry_size: loader.offset(pinned::weather::ENTRY_SIZE)?,
                entry_generation: loader.offset(pinned::weather::ENTRY_GENERATION)?,
                entry_data: loader.offset(pinned::weather::ENTRY_DATA)?,
                layers: loader.offset(pinned::weather::LAYERS)?,
                layer_count: loader.offset(pinned::weather::LAYER_COUNT)?,
                slot_size: loader.offset(pinned::weather::SLOT_SIZE)?,
                slot_time: loader.offset(pinned::weather::SLOT_TIME)?,
                slot_phenomenon: loader.offset(pinned::weather::SLOT_PHENOMENON)?,
                slot_current: loader.offset(pinned::weather::SLOT_CURRENT)?,
                slot_next: loader.offset(pinned::weather::SLOT_NEXT)?,
                instance_phenomenon: loader.offset(pinned::weather::INSTANCE_PHENOMENON)?,
                instance_position: loader.offset(pinned::weather::INSTANCE_POSITION)?,
                instance_radius: loader.offset(pinned::weather::INSTANCE_RADIUS)?,
                instance_despawn: loader.offset(pinned::weather::INSTANCE_DESPAWN)?,
                resource_layers: field(resource, "layers")?,
                resource_phenomena: field(resource, "phenomenons")?,
                layer_size: size(layer)?,
                layer_range: field(layer, "phenomenonIndexRange")?,
                phenomenon_size: size(phenomenon)?,
                phenomenon_effect: field(phenomenon, "effect")?,
            })
        })
        .as_ref()
}

/// A blob array at `header`: where its elements start and how many.
///
/// # Safety
///
/// - `header` is a live blob array header, readable for 8 bytes: an `i32`
///   offset from itself, then a `u32` count.
/// - Nothing writes the header during the call.
unsafe fn blob_array(header: usize) -> (usize, usize) {
    // SAFETY: the caller guarantees 8 readable bytes of a live header,
    // starting with its i32 offset.
    let offset: i32 = unsafe { read(header, 0) };
    // SAFETY: the caller guarantees 8 readable bytes of a live header, with
    // its u32 count at 4.
    let count: u32 = unsafe { read(header, 4) };
    if offset == 0 {
        return (0, 0);
    }
    (header.wrapping_add_signed(offset as isize), count as usize)
}

/// The weather system and the weather clock, on the game thread.
fn system(layout: &Layout) -> Result<(usize, i64), Status> {
    let server = crate::world::server()?;
    // SAFETY: `world::server` returned the live server on the game thread at
    // a quiet moment; its weather system pointer sits at the build table's
    // `weather.system` offset.
    let system: usize = unsafe { read(server, layout.system) };
    // SAFETY: `world::server` returned the live server on the game thread at
    // a quiet moment; its game object pointer sits at the build table's
    // `server.player_infos` offset.
    let game: usize = unsafe { read(server, layout.game) };
    if system == 0 || game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `game` is the server's non-null game object by the check
    // before it, and `world::server` put this call on the game thread.
    let now = unsafe { (layout.clock)(core::ptr::with_exposed_provenance(game)) };
    Ok((system, now))
}

/// The weather resource, held for the life of the guard.
struct Resource<'a> {
    layout: &'a Layout,
    cache: [u64; 4],
}

impl Resource<'_> {
    /// Locks the weather resource of `system`, the live weather system
    /// `system()` returned on the game thread; `None` when it is not
    /// loaded.
    fn lock(layout: &Layout, system: usize) -> Option<Resource<'_>> {
        let mut cache = [0u64; 4];
        // SAFETY: `system` is the live weather system `system()` found on the
        // game thread; its resource manager word sits at the build table's
        // `weather.manager` offset.
        let manager: u64 = unsafe { read(system, layout.manager) };
        // SAFETY: `system` is the live weather system `system()` found on the
        // game thread; its u32 resource handle sits at the build table's
        // `weather.resource` offset.
        let handle: u32 = unsafe { read(system, layout.resource) };
        // SAFETY: `system` is the live weather system `system()` found on the
        // game thread; its u32 resource generation sits at the build table's
        // `weather.generation` offset.
        let generation: u32 = unsafe { read(system, layout.generation) };
        // SAFETY: the build table's `weather.resource_lock` with the
        // system's own manager and handle and a zeroed four-word cache, as
        // the game's sampler calls it, on the game thread.
        let locked = unsafe { (layout.lock)(&raw mut cache, manager, handle) };
        let resource = Resource { layout, cache };
        let cached_generation = u32::try_from(cache[2] >> 32).unwrap_or(0);
        (locked != 0 && cache[1] != 0 && cached_generation == generation).then_some(resource)
    }

    fn address(&self) -> usize {
        usize::try_from(self.cache[1]).unwrap_or(0)
    }
}

impl Drop for Resource<'_> {
    fn drop(&mut self) {
        if self.cache[1] != 0 {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the handle is the low half"
            )]
            let handle = self.cache[2] as u32;
            // SAFETY: `cache` holds the lock `Resource::lock` took (its word
            // 1 is non-zero by the check before it), and its manager and handle
            // are the ones the game's release takes for it.
            unsafe { (self.layout.release)(self.cache[0], handle) };
        }
    }
}

/// How clear, rainy, snowy and stormy it is at `position`.
pub(crate) fn sample(loader: &Loader, position: [i64; 3]) -> Result<[f32; 4], Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (system, now) = system(layout)?;
    let mut out = [0.0f32; 4];
    // SAFETY: `system` is the live weather system `system()` found on the
    // game thread at a quiet moment; the sampler only reads it and locks the
    // resource itself; `out` and `position` live on this stack for the call.
    unsafe {
        (layout.sample)(
            &raw mut out,
            core::ptr::with_exposed_provenance(system),
            &raw const position,
            now,
        );
    };
    Ok(out)
}

/// The instance data a handle names, or `None` for none.
fn instance(layout: &Layout, system: usize, handle: u32) -> Option<usize> {
    if handle == 0 {
        return None;
    }
    let slot = (handle & 0xffff) as usize;
    // SAFETY: `system` is the live weather system `system()` found on the
    // game thread; its pool's u8 page shift sits at the build table's
    // `weather.page_shift` offset.
    let shift: u8 = unsafe { read(system, layout.shift) };
    // SAFETY: `system` is the live weather system `system()` found on the
    // game thread; its pool's index mask sits at the build table's
    // `weather.page_mask` offset.
    let mask: usize = unsafe { read(system, layout.mask) };
    // SAFETY: `system` is the live weather system `system()` found on the
    // game thread; its pool's page array pointer sits at the build table's
    // `weather.pages` offset.
    let pages: usize = unsafe { read(system, layout.pages) };
    // SAFETY: `system` is the live weather system `system()` found on the
    // game thread; its pool's page count sits at the build table's
    // `weather.page_count` offset.
    let page_count: usize = unsafe { read(system, layout.page_count) };
    let page = slot >> (shift & 63);
    if pages == 0 || page >= page_count {
        return None;
    }
    let descriptor = pages + page * layout.page_size;
    // SAFETY: `pages` is non-null and `page` below `page_count` by the check
    // before it, so `descriptor` is a page descriptor of the pool, of the build
    // table's `weather.page_size`; its entries pointer sits at
    // `weather.page_entries`.
    let entries: usize = unsafe { read(descriptor, layout.page_entries) };
    // SAFETY: `descriptor` is a page descriptor of the pool by the bounds
    // check before it; its entry count sits at the build table's
    // `weather.page_entry_count` offset.
    let entry_count: usize = unsafe { read(descriptor, layout.page_entry_count) };
    let index = slot & mask;
    if entries == 0 || index >= entry_count {
        return None;
    }
    let entry = entries + index * layout.entry_size;
    // SAFETY: `entries` is non-null and `index` below `entry_count` by the
    // check before it, so `entry` is an entry of the page, of the build table's
    // `weather.entry_size`; its u32 generation sits at
    // `weather.entry_generation`.
    let generation: u32 = unsafe { read(entry, layout.entry_generation) };
    ((generation & 0x7fff_ffff) == handle >> 16).then_some(entry + layout.entry_data)
}

/// The current front of each layer.
pub(crate) fn fronts(loader: &Loader) -> Result<Vec<WeatherFront>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (system, now) = system(layout)?;
    let resource = Resource::lock(layout, system).ok_or(Status::MISSING)?;
    // SAFETY: `resource` holds the weather resource locked, and its
    // reflected `phenomenons` field is a blob array header.
    let (phenomena, phenomenon_count) =
        unsafe { blob_array(resource.address() + layout.resource_phenomena) };
    // SAFETY: `system` is the live weather system `system()` found on the
    // game thread; its layer array pointer sits at the build table's
    // `weather.layers` offset.
    let layers: usize = unsafe { read(system, layout.layers) };
    // SAFETY: `system` is the live weather system `system()` found on the
    // game thread; its u64 layer count sits at the build table's
    // `weather.layer_count` offset.
    let layer_count: u64 = unsafe { read(system, layout.layer_count) };
    let mut out = Vec::new();
    for layer in 0..usize::try_from(layer_count).unwrap_or(0) {
        // SAFETY: `layer` is below the system's layer count, so this is a
        // slot of its layer array, of the build table's `weather.slot_size`;
        // its u32 current handle sits at `weather.slot_current`.
        let current: u32 = unsafe { read(layers + layer * layout.slot_size, layout.slot_current) };
        let Some(data) = instance(layout, system, current) else {
            continue;
        };
        // SAFETY: `instance` checked the handle's page, index and generation,
        // so `data` is a live instance; its u8 phenomenon index sits at the
        // build table's `weather.instance_phenomenon` offset.
        let index: u8 = unsafe { read(data, layout.instance_phenomenon) };
        let index = usize::from(index);
        let effect = if index < phenomenon_count {
            // SAFETY: `index` is below the resource's phenomenon count by the
            // check before it, and the resource stays locked; the u8 effect sits
            // at the reflected `effect` field of a phenomenon.
            u32::from(unsafe {
                read::<u8>(
                    phenomena + index * layout.phenomenon_size,
                    layout.phenomenon_effect,
                )
            })
        } else {
            WeatherFront::NONE
        };
        // SAFETY: `data` is a live instance, checked by `instance`; its i64
        // despawn time sits at the build table's `weather.instance_despawn`
        // offset.
        let despawn: i64 = unsafe { read(data, layout.instance_despawn) };
        out.push(WeatherFront {
            header: Header::of::<WeatherFront>(),
            layer: u32::try_from(layer).unwrap_or(u32::MAX),
            effect,
            // SAFETY: `data` is a live instance, checked by `instance`; its
            // position sits at the build table's `weather.instance_position`
            // offset.
            position: unsafe { read(data, layout.instance_position) },
            // SAFETY: `data` is a live instance, checked by `instance`; its
            // radius sits at the build table's `weather.instance_radius`
            // offset.
            radius: unsafe { read(data, layout.instance_radius) },
            reserved: 0,
            remaining: despawn.saturating_sub(now).max(0),
        });
    }
    Ok(out)
}

/// Starts new weather in every layer: `effect` where a layer has it,
/// nothing elsewhere, for `duration` game nanoseconds (0 for the game's
/// own); [`ANY`] lets the game choose at its next update.
pub(crate) fn set(loader: &Loader, effect: u32, duration: i64) -> Result<(), Status> {
    if duration < 0 || (effect > WeatherFront::BLIZZARD && effect != ANY) {
        return Err(Status::BAD_ARGS);
    }
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (system, now) = system(layout)?;
    let resource = Resource::lock(layout, system).ok_or(Status::MISSING)?;
    // SAFETY: `resource` holds the weather resource locked, and its
    // reflected `layers` field is a blob array header.
    let (layer_defs, layer_def_count) =
        unsafe { blob_array(resource.address() + layout.resource_layers) };
    // SAFETY: `resource` holds the weather resource locked, and its
    // reflected `phenomenons` field is a blob array header.
    let (phenomena, phenomenon_count) =
        unsafe { blob_array(resource.address() + layout.resource_phenomena) };
    // SAFETY: `system` is the live weather system `system()` found on the
    // game thread; its layer array pointer sits at the build table's
    // `weather.layers` offset.
    let layers: usize = unsafe { read(system, layout.layers) };
    // SAFETY: `system` is the live weather system `system()` found on the
    // game thread; its u64 layer count sits at the build table's
    // `weather.layer_count` offset.
    let layer_count: u64 = unsafe { read(system, layout.layer_count) };
    let layer_count = usize::try_from(layer_count)
        .unwrap_or(0)
        .min(layer_def_count);
    let system_mut = core::ptr::with_exposed_provenance_mut::<u8>(system);
    for layer in 0..layer_count {
        let slot = layers + layer * layout.slot_size;
        for at in [layout.slot_current, layout.slot_next] {
            // SAFETY: `layer` is below the system's layer count, so `slot` is
            // a slot of its layer array; its u32 handles sit at the build
            // table's `weather.slot_current` and `weather.slot_next` offsets.
            let handle: u32 = unsafe { read(slot, at) };
            if handle != 0 {
                // SAFETY: `handle` is a non-zero handle the slot holds, and
                // `system()` put this call on the game thread at a quiet
                // moment, where the game's own free runs.
                unsafe { (layout.free)(system_mut, handle) };
            }
        }
        // SAFETY: `slot` is a slot of the live layer array, written here on
        // the game thread at a quiet moment as the game's reset writes it;
        // its i64 time sits at the build table's `weather.slot_time` offset.
        unsafe { write(slot, layout.slot_time, i64::MIN) };
        // SAFETY: `slot` is a slot of the live layer array, written on the
        // game thread at a quiet moment; its u64 last phenomenon sits at the
        // build table's `weather.slot_phenomenon` offset.
        unsafe { write(slot, layout.slot_phenomenon, u64::MAX) };
        for at in [layout.slot_current, layout.slot_next] {
            // SAFETY: `slot` is a slot of the live layer array, written on
            // the game thread at a quiet moment; its u32 handles sit at the
            // build table's `weather.slot_current` and `weather.slot_next`
            // offsets.
            unsafe { write(slot, at, 0u32) };
        }
        if effect == ANY {
            continue;
        }
        let def = layer_defs + layer * layout.layer_size;
        // SAFETY: `layer` is below the resource's layer count (the `min`
        // before it) and the resource stays locked; the u8 first phenomenon
        // starts the reflected `phenomenonIndexRange` field.
        let first: u8 = unsafe { read(def, layout.layer_range) };
        // SAFETY: `layer` is below the resource's layer count and the
        // resource stays locked; the u8 count follows the first in the
        // reflected `phenomenonIndexRange` field.
        let count: u8 = unsafe { read(def, layout.layer_range + 1) };
        let pick = (0..count)
            .find(|&local| {
                let index = usize::from(first) + usize::from(local);
                index < phenomenon_count
                    // SAFETY: `index` is below the resource's phenomenon
                    // count by the check before it, and the resource stays
                    // locked; the u8 effect sits at the reflected `effect`
                    // field of a phenomenon.
                    && u32::from(unsafe { read::<u8>(phenomena + index * layout.phenomenon_size, layout.phenomenon_effect) })
                        == effect
                    && effect != WeatherFront::NONE
            })
            .unwrap_or(count);
        let override_duration = [duration.cast_unsigned(), u64::from(duration > 0)];
        // SAFETY: the game's spawn of a layer's phenomenon with the live
        // system, its locked resource, the weather clock and a local index
        // (`count` is the layer's nothing), on the game thread at a quiet
        // moment as `system()` checked; `override_duration` lives on this
        // stack for the call.
        unsafe {
            (layout.spawn)(
                system_mut,
                core::ptr::with_exposed_provenance(resource.address()),
                now,
                layer as u64,
                pick,
                &raw const override_duration,
            );
        };
    }
    Ok(())
}

static CHUNKS: OnceLock<ChunkLayout> = OnceLock::new();
static CHUNK_EVENTS: Mutex<Vec<(Event, ChunkData)>> = Mutex::new(Vec::new());
// SAFETY: `ChunkFn` is the signature of the build table's `chunks.step`, and
// `link` uses this slot only as that function's next.
static CHUNK_NEXT: Next<ChunkFn> = unsafe { Next::new() };

/// Where a chunk keeps its state and index, and the manager its width.
#[derive(Debug, Clone, Copy)]
struct ChunkLayout {
    manager: usize,
    width: usize,
    height: usize,
    index: usize,
}

/// Links the chunk step when a mod asked for chunk events.
pub(crate) fn link(loader: &'static Loader) {
    if !loader.subscribed(Event::CHUNK_LOADED) && !loader.subscribed(Event::CHUNK_UNLOADED) {
        return;
    }
    let found = (|| {
        Some(ChunkLayout {
            manager: loader.offset(pinned::chunks::MANAGER)?,
            width: loader.offset(pinned::chunks::WIDTH)?,
            height: loader.offset(pinned::chunks::HEIGHT)?,
            index: loader.offset(pinned::chunks::INDEX)?,
        })
    })();
    let Some(chunks) = found else {
        crate::log!(WARN, "chunk events off: not in this build");
        return;
    };
    let _first = CHUNKS.set(chunks);
    loader.link_feature(
        "chunk events",
        loader.address(pinned::chunks::STEP),
        &CHUNK_NEXT,
        chunk_step as ChunkFn,
        HookFlags::NONE,
    );
}

/// The switch of the chunk events.
static CHUNKS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("chunk events");

unsafe extern "system" fn chunk_step(entities: *mut u8, chunk: *mut u8, budget: *mut u32) -> u8 {
    let before = if chunk.is_null() {
        0xff
    } else {
        // SAFETY: `chunk` is non-null by the check before it and is the step's
        // chunk argument, which starts with its u8 state.
        unsafe { read_at::<u8>(chunk, 0) }
    };
    // SAFETY: this detour runs only as the `chunks.step` link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    let done = unsafe { CHUNK_NEXT.get()(entities, chunk, budget) };
    if chunk.is_null() {
        return done;
    }
    // SAFETY: `chunk` is non-null by the check before it and is the step's
    // chunk argument, live for the call, which starts with its u8 state.
    let after = unsafe { read_at::<u8>(chunk, 0) };
    let event = match (before, after) {
        (LOADING, LOADED) => Event::CHUNK_LOADED,
        (UNLOADING, UNLOADED) => Event::CHUNK_UNLOADED,
        _ => return done,
    };
    let _caught = CHUNKS_FUSE.run("the chunk step", || {
        // SAFETY: `entities` and `chunk` are the step's own arguments,
        // `chunk` non-null by the check before it, and this runs on the step's
        // thread inside it.
        unsafe { record(event, entities, chunk) };
    });
    done
}

/// Queues a chunk's turn in the server's world for after the step.
///
/// # Safety
///
/// - `entities` is the chunk step's entity world argument and `chunk` its
///   non-null chunk argument, both live for the call.
/// - Called on the step's thread, inside the step.
unsafe fn record(event: Event, entities: *mut u8, chunk: *mut u8) {
    let Some(layout) = CHUNKS.get() else {
        return;
    };
    if entities.addr() as u64 != crate::world::server_world() {
        return;
    }
    // SAFETY: `entities` is the server's entity world by the check before it and
    // live by the caller's contract; its chunk manager pointer sits at the
    // build table's `chunks.manager` offset.
    let manager: usize = unsafe { read(entities.expose_provenance(), layout.manager) };
    if manager == 0 {
        return;
    }
    // SAFETY: `manager` is the world's non-null chunk manager by the check
    // before it; its u32 grid width sits at the build table's `chunks.width`
    // offset.
    let width: u32 = unsafe { read(manager, layout.width) };
    // SAFETY: `manager` is the world's non-null chunk manager by the check
    // before it; its u32 grid height sits at the build table's `chunks.height`
    // offset.
    let height: u32 = unsafe { read(manager, layout.height) };
    // SAFETY: the caller guarantees a live, non-null chunk; its u32 grid
    // index sits at the build table's `chunks.index` offset.
    let index: u32 = unsafe { read(chunk.expose_provenance(), layout.index) };
    if width == 0 || index >= width.saturating_mul(height) {
        return;
    }
    let data = ChunkData {
        header: Header::of::<ChunkData>(),
        index,
        x: index % width,
        z: index / width,
        size: CHUNK_METRES,
    };
    CHUNK_EVENTS.relock().push((event, data));
}

/// Raises the chunk turns of the last step, on the game thread after it.
pub(crate) fn after_step(loader: &Loader) {
    if CHUNKS.get().is_none() {
        return;
    }
    let events = std::mem::take(&mut *CHUNK_EVENTS.relock());
    for (event, data) in &events {
        loader.emit(*event, None, (&raw const *data).cast());
    }
}

/// Drops the chunk turns of a hosted game that stopped.
pub(crate) fn forget() {
    CHUNK_EVENTS.relock().clear();
}
