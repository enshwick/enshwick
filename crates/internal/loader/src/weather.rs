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

use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{ChunkData, Event, Header, HookFlags, Status, WeatherFront};

use crate::chains::Next;
use crate::host::Loader;

/// The effect value that asks the game to choose.
pub(crate) const ANY: u32 = u32::MAX;
/// A layer slot's bytes: next time, last phenomenon, two handles.
const SLOT: usize = 0x18;
/// An instance pool page descriptor's bytes, and an entry's.
const PAGE: usize = 0x40;
const ENTRY: usize = 0x48;
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
    layers: usize,
    layer_count: usize,
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
            let function = |name: &str| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let resource = "keen::WeatherSystemResource";
            let layer = "keen::WeatherSystemLayerResource";
            let phenomenon = "keen::WeatherSystemPhenomenonResource";
            let (sample, clock, lock, release, free, spawn) = (
                function("weather.sample")?,
                function("weather.clock")?,
                function("weather.resource_lock")?,
                function("weather.resource_release")?,
                function("weather.free")?,
                function("weather.spawn")?,
            );
            Some(Layout {
                // SAFETY: each symbol is the game function with this
                // signature on this build.
                sample: unsafe { core::mem::transmute::<usize, SampleFn>(sample) },
                // SAFETY: as above.
                clock: unsafe { core::mem::transmute::<usize, ClockFn>(clock) },
                // SAFETY: as above.
                lock: unsafe { core::mem::transmute::<usize, LockFn>(lock) },
                // SAFETY: as above.
                release: unsafe { core::mem::transmute::<usize, ReleaseFn>(release) },
                // SAFETY: as above.
                free: unsafe { core::mem::transmute::<usize, FreeFn>(free) },
                // SAFETY: as above.
                spawn: unsafe { core::mem::transmute::<usize, SpawnFn>(spawn) },
                system: loader.offset("weather.system")?,
                game: loader.offset("server.player_infos")?,
                manager: loader.offset("weather.manager")?,
                resource: loader.offset("weather.resource")?,
                generation: loader.offset("weather.generation")?,
                pages: loader.offset("weather.pages")?,
                page_count: loader.offset("weather.page_count")?,
                shift: loader.offset("weather.page_shift")?,
                mask: loader.offset("weather.page_mask")?,
                layers: loader.offset("weather.layers")?,
                layer_count: loader.offset("weather.layer_count")?,
                instance_phenomenon: loader.offset("weather.instance_phenomenon")?,
                instance_position: loader.offset("weather.instance_position")?,
                instance_radius: loader.offset("weather.instance_radius")?,
                instance_despawn: loader.offset("weather.instance_despawn")?,
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

/// Reads a `T` at `base + at`.
///
/// # Safety
///
/// The address holds a readable `T`.
unsafe fn read<T: Copy>(base: usize, at: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { crate::world::read(base, at) }
}

/// Writes a `T` at `base + at`.
///
/// # Safety
///
/// The address holds a writable `T`.
unsafe fn write<T: Copy>(base: usize, at: usize, value: T) {
    // SAFETY: forwarded from the caller.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<T>(base.wrapping_add(at)).write_unaligned(value);
    };
}

/// A blob array at `header`: where its elements start and how many.
///
/// # Safety
///
/// `header` is a live blob array header: an offset from itself and a count.
unsafe fn blob_array(header: usize) -> (usize, usize) {
    // SAFETY: forwarded from the caller.
    let offset: i32 = unsafe { read(header, 0) };
    // SAFETY: as above.
    let count: u32 = unsafe { read(header, 4) };
    if offset == 0 {
        return (0, 0);
    }
    (header.wrapping_add_signed(offset as isize), count as usize)
}

/// The weather system and the weather clock, on the game thread.
fn system(layout: &Layout) -> Result<(usize, i64), Status> {
    let server = crate::world::server()?;
    // SAFETY: the server holds its weather system and its game object.
    let system: usize = unsafe { read(server, layout.system) };
    // SAFETY: as above.
    let game: usize = unsafe { read(server, layout.game) };
    if system == 0 || game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the game's clock of its live game object, on the game thread.
    let now = unsafe { (layout.clock)(core::ptr::with_exposed_provenance(game)) };
    Ok((system, now))
}

/// The weather resource, held for the life of the guard.
struct Resource<'a> {
    layout: &'a Layout,
    cache: [u64; 4],
}

impl Resource<'_> {
    fn lock(layout: &Layout, system: usize) -> Option<Resource<'_>> {
        let mut cache = [0u64; 4];
        // SAFETY: the system's resource manager and handle.
        let manager: u64 = unsafe { read(system, layout.manager) };
        // SAFETY: as above.
        let handle: u32 = unsafe { read(system, layout.resource) };
        // SAFETY: as above.
        let generation: u32 = unsafe { read(system, layout.generation) };
        // SAFETY: the game's lock with a zeroed cache, as its sampler does.
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
            // SAFETY: the game's release of the lock taken in `lock`.
            unsafe { (self.layout.release)(self.cache[0], handle) };
        }
    }
}

/// How clear, rainy, snowy and stormy it is at `position`.
pub(crate) fn sample(loader: &Loader, position: [i64; 3]) -> Result<[f32; 4], Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (system, now) = system(layout)?;
    let mut out = [0.0f32; 4];
    // SAFETY: the game's read-only sampler on its live system, on the game
    // thread between steps; it locks the resource itself.
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
    // SAFETY: the system's pool fields.
    let shift: u8 = unsafe { read(system, layout.shift) };
    // SAFETY: as above.
    let mask: usize = unsafe { read(system, layout.mask) };
    // SAFETY: as above.
    let pages: usize = unsafe { read(system, layout.pages) };
    // SAFETY: as above.
    let page_count: usize = unsafe { read(system, layout.page_count) };
    let page = slot >> (shift & 63);
    if pages == 0 || page >= page_count {
        return None;
    }
    let descriptor = pages + page * PAGE;
    // SAFETY: a page descriptor of the pool: its entries and their number.
    let entries: usize = unsafe { read(descriptor, 8) };
    // SAFETY: as above.
    let entry_count: usize = unsafe { read(descriptor, 0x10) };
    let index = slot & mask;
    if entries == 0 || index >= entry_count {
        return None;
    }
    let entry = entries + index * ENTRY;
    // SAFETY: an entry of the page: its generation, then its data.
    let generation: u32 = unsafe { read(entry, 0) };
    ((generation & 0x7fff_ffff) == handle >> 16).then_some(entry + 8)
}

/// The current front of each layer.
pub(crate) fn fronts(loader: &Loader) -> Result<Vec<WeatherFront>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (system, now) = system(layout)?;
    let resource = Resource::lock(layout, system).ok_or(Status::MISSING)?;
    // SAFETY: the resource's phenomenon array.
    let (phenomena, phenomenon_count) =
        unsafe { blob_array(resource.address() + layout.resource_phenomena) };
    // SAFETY: the system's layers and their number.
    let layers: usize = unsafe { read(system, layout.layers) };
    // SAFETY: as above.
    let layer_count: u64 = unsafe { read(system, layout.layer_count) };
    let mut out = Vec::new();
    for layer in 0..usize::try_from(layer_count).unwrap_or(0) {
        // SAFETY: a slot of the layer array: the current instance's handle.
        let current: u32 = unsafe { read(layers + layer * SLOT, 0x10) };
        let Some(data) = instance(layout, system, current) else {
            continue;
        };
        // SAFETY: the live instance's data.
        let index: u8 = unsafe { read(data, layout.instance_phenomenon) };
        let index = usize::from(index);
        let effect = if index < phenomenon_count {
            // SAFETY: a phenomenon of the resource.
            u32::from(unsafe {
                read::<u8>(
                    phenomena + index * layout.phenomenon_size,
                    layout.phenomenon_effect,
                )
            })
        } else {
            WeatherFront::NONE
        };
        // SAFETY: as above.
        let despawn: i64 = unsafe { read(data, layout.instance_despawn) };
        out.push(WeatherFront {
            header: Header::of::<WeatherFront>(),
            layer: u32::try_from(layer).unwrap_or(u32::MAX),
            effect,
            // SAFETY: as above.
            position: unsafe { read(data, layout.instance_position) },
            // SAFETY: as above.
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
    // SAFETY: the resource's layer and phenomenon arrays.
    let (layer_defs, layer_def_count) =
        unsafe { blob_array(resource.address() + layout.resource_layers) };
    // SAFETY: as above.
    let (phenomena, phenomenon_count) =
        unsafe { blob_array(resource.address() + layout.resource_phenomena) };
    // SAFETY: the system's layers and their number.
    let layers: usize = unsafe { read(system, layout.layers) };
    // SAFETY: as above.
    let layer_count: u64 = unsafe { read(system, layout.layer_count) };
    let layer_count = usize::try_from(layer_count)
        .unwrap_or(0)
        .min(layer_def_count);
    let system_mut = core::ptr::with_exposed_provenance_mut::<u8>(system);
    for layer in 0..layer_count {
        let slot = layers + layer * SLOT;
        for at in [0x10, 0x14] {
            // SAFETY: a handle of the slot.
            let handle: u32 = unsafe { read(slot, at) };
            if handle != 0 {
                // SAFETY: the game's removal of a live instance, which tells
                // the clients, on the game thread between steps.
                unsafe { (layout.free)(system_mut, handle) };
            }
        }
        // SAFETY: the slot, emptied as the game's reset empties it.
        unsafe {
            write(slot, 0, i64::MIN);
        };
        // SAFETY: as above.
        unsafe { write(slot, 8, u64::MAX) };
        // SAFETY: as above.
        unsafe { write(slot, 0x10, 0u64) };
        if effect == ANY {
            continue;
        }
        let def = layer_defs + layer * layout.layer_size;
        // SAFETY: the layer's range of phenomena: first and count.
        let first: u8 = unsafe { read(def, layout.layer_range) };
        // SAFETY: as above.
        let count: u8 = unsafe { read(def, layout.layer_range + 1) };
        let pick = (0..count)
            .find(|&local| {
                let index = usize::from(first) + usize::from(local);
                index < phenomenon_count
                    // SAFETY: a phenomenon of the resource.
                    && u32::from(unsafe { read::<u8>(phenomena + index * layout.phenomenon_size, layout.phenomenon_effect) })
                        == effect
                    && effect != WeatherFront::NONE
            })
            .unwrap_or(count);
        let override_duration = [duration.cast_unsigned(), u64::from(duration > 0)];
        // SAFETY: the game's spawn of a layer's phenomenon, with its locked
        // resource, the weather clock and a local index (`count` is the
        // layer's nothing), on the game thread between steps.
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
// SAFETY: the slot joins the chain of the chunk step, typed `ChunkFn`.
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
            manager: loader.offset("chunks.manager")?,
            width: loader.offset("chunks.width")?,
            height: loader.offset("chunks.height")?,
            index: loader.offset("chunks.index")?,
        })
    })();
    let Some(chunks) = found else {
        crate::log!(WARN, "chunk events off: not in this build");
        return;
    };
    let _first = CHUNKS.set(chunks);
    loader.link_feature(
        "chunk events",
        loader.address("chunks.step"),
        &CHUNK_NEXT,
        chunk_step as ChunkFn,
        HookFlags::NONE,
    );
}

unsafe extern "system" fn chunk_step(entities: *mut u8, chunk: *mut u8, budget: *mut u32) -> u8 {
    let before = if chunk.is_null() {
        0xff
    } else {
        // SAFETY: the game's chunk, its state first.
        unsafe { chunk.read() }
    };
    // SAFETY: forwarded unchanged.
    let done = unsafe { CHUNK_NEXT.get()(entities, chunk, budget) };
    if chunk.is_null() {
        return done;
    }
    // SAFETY: as above.
    let after = unsafe { chunk.read() };
    let event = match (before, after) {
        (LOADING, LOADED) => Event::CHUNK_LOADED,
        (UNLOADING, UNLOADED) => Event::CHUNK_UNLOADED,
        _ => return done,
    };
    let _caught = std::panic::catch_unwind(|| {
        // SAFETY: the game's entity world and chunk, as passed to the step.
        unsafe { record(event, entities, chunk) };
    });
    done
}

/// Queues a chunk's turn in the server's world for after the step.
///
/// # Safety
///
/// `entities` and `chunk` are the chunk step's arguments.
unsafe fn record(event: Event, entities: *mut u8, chunk: *mut u8) {
    let Some(layout) = CHUNKS.get() else {
        return;
    };
    if entities.addr() as u64 != crate::world::server_world() {
        return;
    }
    // SAFETY: the entity world holds its chunk manager.
    let manager: usize = unsafe { read(entities.addr(), layout.manager) };
    if manager == 0 {
        return;
    }
    // SAFETY: the manager's grid size.
    let width: u32 = unsafe { read(manager, layout.width) };
    // SAFETY: as above.
    let height: u32 = unsafe { read(manager, layout.height) };
    // SAFETY: the chunk's index in the grid.
    let index: u32 = unsafe { read(chunk.addr(), layout.index) };
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
    CHUNK_EVENTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((event, data));
}

/// Raises the chunk turns of the last step, on the game thread after it.
pub(crate) fn after_step(loader: &Loader) {
    if CHUNKS.get().is_none() {
        return;
    }
    let events = std::mem::take(&mut *CHUNK_EVENTS.lock().unwrap_or_else(PoisonError::into_inner));
    for (event, data) in &events {
        loader.emit(*event, None, (&raw const *data).cast());
    }
}

/// Drops the chunk turns of a hosted game that stopped.
pub(crate) fn forget() {
    CHUNK_EVENTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
}
