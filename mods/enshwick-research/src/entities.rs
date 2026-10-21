//! R11, entity ids across the wire: which entities stand near each player,
//! with their ids, templates and positions, seen once from the server and
//! once from the client, so the two logs can be joined by template and
//! position.
//!
//! No reflected component names a server id on either build, while the
//! replicated components carry plain entity ids (`DespawnLink`,
//! `OwnerRelationship`, `NetworkComfort`, `AggroTargetNetworkData`). Each
//! side therefore logs, per entity, the id, its slot in the entity map and
//! its entity record, and every id those components hold with what that id
//! names on the same side.
//!
//! The server side runs on the server's game thread every five seconds
//! through the world reads. The client side runs inside the client-only
//! `client_cursor` system, whose query context leads to the client's entity
//! world, and calls the engine's own walk and lookups there; every pointer
//! it follows is read through reads that cannot fault first.

use std::ops::ControlFlow;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use enshwick_loader_api::ecs::{Entity, metres};
use enshwick_loader_api::world::{ComponentType, Placed, World};
use enshwick_loader_api::{Load, Next, Off};

use crate::memory;

/// How often each side logs.
const PERIOD: Duration = Duration::from_secs(5);
/// How far from the player an entity is logged, in metres.
const RADIUS: f64 = 20.0;
/// The most entities logged per player and sweep.
const MAX_LOGGED: usize = 40;

/// The replicated components that hold an entity id, with its offset.
const REFERENCES: [(&str, &str, usize); 4] = [
    ("keen::ecs::DespawnLink", "DespawnLink.parentEntityId", 0),
    (
        "keen::ecs::OwnerRelationship",
        "OwnerRelationship.ownerEntityId",
        0,
    ),
    (
        "keen::ecs::NetworkComfort",
        "NetworkComfort.hearthEntityId",
        0,
    ),
    (
        "keen::ecs::AggroTargetNetworkData",
        "AggroTargetNetworkData.entityId",
        4,
    ),
];

/// The component only the local player's character has on a client.
const LOCAL_PLAYER: &str = "keen::ecs::ClientPlayerInput";

/// The engine's layout on client build 1269579, as the loader's build
/// table names it.
mod layout {
    /// `ecs.context_entity_world`: the query descriptor's entity world.
    pub(super) const CONTEXT_WORLD: usize = 0x30;
    /// `ecs.registry_type_map` and its fallback, in the registry.
    pub(super) const REGISTRY_MAPS: [usize; 2] = [0x30, 0x88];
    /// `ecs.entity_map`: the id-to-slot map in the entity world.
    pub(super) const ENTITY_MAP: usize = 0x148;
    /// `ecs.entity_map_capacity`: what a missing id's slot reads as.
    pub(super) const ENTITY_CAPACITY: usize = 0x158;
    /// `ecs.entity_records`: one record pointer per slot.
    pub(super) const ENTITY_RECORDS: usize = 0x188;
    /// `ecs.record_template`: the record's pointer to its template GUID.
    pub(super) const RECORD_TEMPLATE: usize = 0x28;
    /// `ecs.entities_entry_size` and `ecs.entities_entry_transform`.
    pub(super) const ENTRY_SIZE: usize = 0x40;
    pub(super) const ENTRY_TRANSFORM: usize = 8;
    /// Bytes of an entity record shown.
    pub(super) const RECORD_SHOWN: usize = 0x40;
}

/// The engine's growable array the walk fills; with no grow function a
/// full array ends the walk with 0.
#[repr(C)]
struct Found {
    entries: *mut u8,
    count: u64,
    capacity: u64,
    grow: usize,
}

type EntitiesWith = unsafe extern "system" fn(out: *mut Found, world: *mut u8, ty: u32) -> u8;
type EntitySlot = unsafe extern "system" fn(map: *const u8, entity: *const u32) -> u64;
type TypeIndex = unsafe extern "system" fn(map: *const u8, hash: *const u32) -> *const u16;
type ReadComponent = unsafe extern "system" fn(
    out: *mut [usize; 2],
    world: *const u8,
    index: *const u16,
    entity: u32,
);
type System = unsafe extern "system" fn(ctx: *mut u64);

/// Entries the first walk makes room for, and the most it grows to.
const FIRST_CAPACITY: usize = 4096;
const MAX_CAPACITY: usize = 1 << 19;

/// The engine functions the client side calls, each checked against the
/// build its address was found on.
struct Engine {
    entities_with: EntitiesWith,
    entity_slot: EntitySlot,
    type_index: TypeIndex,
    read_component: ReadComponent,
    transform: u32,
    local_player: u32,
    references: Vec<(&'static str, u32, usize)>,
    start: Instant,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
/// Milliseconds after start of the last client sweep, plus one; 0 before.
static LAST_SWEEP: AtomicU64 = AtomicU64::new(0);
static FIRST_CALL: AtomicBool = AtomicBool::new(true);
static SERVER_EMPTY_SWEPT: AtomicBool = AtomicBool::new(false);

enshwick_loader_api::detour! {
    static CLIENT_CURSOR: unsafe extern "system" fn(ctx: *mut u64) = client_cursor;
}

/// One function the client side uses.
struct Site {
    symbol: &'static str,
    /// Its first bytes on client build 1269579.
    expected: [u8; 8],
}

const CURSOR_SITE: Site = Site {
    symbol: "research.client_cursor",
    expected: 0x5441_5508_244c_8948_u64.to_le_bytes(),
};
const CALLED: [Site; 4] = [
    Site {
        symbol: "research.entities_with",
        expected: 0x4855_4154_4157_5640_u64.to_le_bytes(),
    },
    Site {
        symbol: "research.entity_slot",
        expected: 0x7c89_4808_245c_8948_u64.to_le_bytes(),
    },
    Site {
        symbol: "research.type_index",
        expected: 0x7c89_4808_245c_8948_u64.to_le_bytes(),
    },
    Site {
        symbol: "research.component_read_raw",
        expected: 0x7489_4808_245c_8948_u64.to_le_bytes(),
    },
];

/// The address of `site` when its first bytes are those of build 1269579.
fn checked(load: &Load<'_>, code: &[u8], site: &Site) -> Result<usize, Off> {
    let address = usize::try_from(load.symbol(site.symbol)?)
        .map_err(|_| Off::new(format!("{}: address out of range", site.symbol)))?;
    let found = address
        .checked_sub(code.as_ptr().addr())
        .and_then(|offset| code.get(offset..offset + site.expected.len()));
    if found == Some(&site.expected[..]) {
        Ok(address)
    } else {
        Err(Off::new(format!(
            "{}: its bytes at {address:#x} are {found:02x?}, not those of build 1269579",
            site.symbol
        )))
    }
}

/// Links the client side: the `client_cursor` system and the engine
/// functions it calls.
///
/// # Errors
///
/// [`Off`] when a site's bytes differ from build 1269579, a type is not
/// reflected or the hook is refused.
pub(crate) fn link_client(load: &mut Load<'_>, world: &World) -> Result<(), Off> {
    let code = load.code()?;
    let cursor = checked(load, code, &CURSOR_SITE)?;
    let [entities_with, entity_slot, type_index, read_component] =
        CALLED.each_ref().map(|site| checked(load, code, site));
    let (entities_with, entity_slot, type_index, read_component) =
        (entities_with?, entity_slot?, type_index?, read_component?);
    let references = REFERENCES
        .iter()
        .filter_map(|&(ty, name, offset)| {
            world
                .component_type(load, ty)
                .ok()
                .map(|t| (name, t.hash(), offset))
        })
        .collect();
    let engine = Engine {
        // SAFETY: each address holds the engine function with this
        // signature on client build 1269579, its first bytes checked above.
        entities_with: unsafe { core::mem::transmute::<usize, EntitiesWith>(entities_with) },
        // SAFETY: as above.
        entity_slot: unsafe { core::mem::transmute::<usize, EntitySlot>(entity_slot) },
        // SAFETY: as above.
        type_index: unsafe { core::mem::transmute::<usize, TypeIndex>(type_index) },
        // SAFETY: as above.
        read_component: unsafe { core::mem::transmute::<usize, ReadComponent>(read_component) },
        transform: world.transform_type().hash(),
        local_player: world.component_type(load, LOCAL_PLAYER)?.hash(),
        references,
        start: Instant::now(),
    };
    let _first = ENGINE.set(engine);
    load.hook(&CLIENT_CURSOR, cursor as u64)
}

/// Links the server side: a sweep every five seconds on the server's game
/// thread, a dedicated server's or the one a client hosts.
///
/// # Errors
///
/// [`Off`] when the scheduler is not offered.
pub(crate) fn link_server(load: &mut Load<'_>, world: World) -> Result<(), Off> {
    let references: Vec<(&'static str, ComponentType, usize)> = REFERENCES
        .iter()
        .filter_map(|&(ty, name, offset)| {
            world
                .component_type(load, ty)
                .ok()
                .map(|t| (name, t, offset))
        })
        .collect();
    let scheduler = load.scheduler()?;
    scheduler.every(PERIOD, move || {
        server_sweep(&world, &references);
        ControlFlow::Continue(())
    });
    Ok(())
}

fn server_sweep(world: &World, references: &[(&'static str, ComponentType, usize)]) {
    let players = world.players();
    let all = world.entities(world.transform_type());
    if players.is_empty() {
        // Once, so a server without players shows the sweep works.
        if let Some(first) = all.first()
            && !SERVER_EMPTY_SWEPT.swap(true, Ordering::Relaxed)
        {
            let who = format!("no players; around entity {}", id(first.entity.0));
            server_around(world, references, &all, &who, first.position);
        }
        return;
    }
    for player in players {
        if let Some(at) = world.position(player.entity) {
            let who = format!("player {:?} entity {}", player.name, id(player.entity.0));
            server_around(world, references, &all, &who, at);
        }
    }
}

/// Logs the entities of `all` within the radius of `at`, nearest first.
fn server_around(
    world: &World,
    references: &[(&'static str, ComponentType, usize)],
    all: &[Placed],
    who: &str,
    at: [f64; 3],
) {
    let mut near: Vec<_> = all
        .iter()
        .map(|placed| (placed.distance(at), placed))
        .filter(|&(distance, _)| distance <= RADIUS)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    enshwick_loader_api::info!(
        "R11 server sweep: {who} at {}; {} entities with a transform, {} within {RADIUS} m (up to {MAX_LOGGED} shown)",
        position(at),
        all.len(),
        near.len()
    );
    for (distance, placed) in near.into_iter().take(MAX_LOGGED) {
        let entity = placed.entity;
        let refs: Vec<String> = references
            .iter()
            .filter_map(|&(name, ty, offset)| {
                let data = world.component(entity, ty)?;
                let target = u32_at(data.addr() + offset)?;
                let names = world
                    .template(Entity(target))
                    .map_or_else(|| "not here".to_owned(), |t| guid(&t));
                Some(format!("{name}={} ({names})", id(target)))
            })
            .collect();
        enshwick_loader_api::info!(
            "R11 server entity {} template {} at {} d={distance:.1}{}",
            id(entity.0),
            world
                .template(entity)
                .map_or_else(|| "?".to_owned(), |t| guid(&t)),
            position(placed.position),
            listed(&refs)
        );
    }
}

fn client_cursor(next: Next<System>, ctx: *mut u64) {
    // SAFETY: forwarded unchanged to the rest of the chain.
    unsafe { next.get()(ctx) };
    let Some(engine) = ENGINE.get() else {
        return;
    };
    let now = u64::try_from(engine.start.elapsed().as_millis()).unwrap_or(u64::MAX) + 1;
    let last = LAST_SWEEP.load(Ordering::Relaxed);
    let period = u64::try_from(PERIOD.as_millis()).unwrap_or(u64::MAX);
    if last != 0 && now.saturating_sub(last) < period {
        return;
    }
    if LAST_SWEEP
        .compare_exchange(last, now, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let Some(world) = client_world(ctx) else {
        enshwick_loader_api::warn!("R11 client: the cursor system's context leads to no world");
        return;
    };
    if FIRST_CALL.swap(false, Ordering::Relaxed) {
        enshwick_loader_api::info!("R11 client world at {world:#x}, from the cursor system");
    }
    client_sweep(engine, world);
}

/// The client's entity world, from the running system's query context.
fn client_world(ctx: *mut u64) -> Option<usize> {
    let descriptor = memory::read_u64(ctx.addr()).and_then(|d| usize::try_from(d).ok())?;
    let world = memory::read_u64(descriptor.checked_add(layout::CONTEXT_WORLD)?)
        .and_then(|w| usize::try_from(w).ok())?;
    let registry = memory::read_u64(world).and_then(|r| usize::try_from(r).ok())?;
    let capacity = memory::read_u64(world + layout::ENTITY_CAPACITY)?;
    (memory::plausible(registry) && capacity > 0 && capacity < 1 << 32).then_some(world)
}

/// Every entity of the client world with a component of type `hash` and a
/// transform, with its position in metres.
fn walk(engine: &Engine, world: usize, hash: u32) -> Vec<(u32, [f64; 3])> {
    let mut capacity = FIRST_CAPACITY;
    loop {
        let mut entries = vec![0u8; capacity * layout::ENTRY_SIZE];
        let mut found = Found {
            entries: entries.as_mut_ptr(),
            count: 0,
            capacity: capacity as u64,
            grow: 0,
        };
        // SAFETY: the client's entity world, inside one of its systems,
        // where entities are added and removed only after the step; an
        // array with room for `capacity` entries.
        let whole = unsafe {
            (engine.entities_with)(
                &raw mut found,
                core::ptr::with_exposed_provenance_mut(world),
                hash,
            )
        };
        if whole != 0 || capacity >= MAX_CAPACITY {
            let count = usize::try_from(found.count).unwrap_or(0).min(capacity);
            return entries
                .as_chunks::<{ layout::ENTRY_SIZE }>()
                .0
                .iter()
                .take(count)
                .map(|bytes| entry(bytes))
                .collect();
        }
        capacity *= 4;
    }
}

/// One entry of the walk: the entity, then its transform, which starts with
/// the position.
fn entry(bytes: &[u8]) -> (u32, [f64; 3]) {
    let word = |at: usize| {
        bytes
            .get(at..at + 8)
            .and_then(|b| b.try_into().ok())
            .map_or(0, i64::from_le_bytes)
    };
    let at = layout::ENTRY_TRANSFORM;
    let entity = bytes
        .get(..4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_le_bytes);
    (entity, metres([word(at), word(at + 8), word(at + 16)]))
}

/// An entity's slot in the client's entity map and its record, when the
/// map holds it.
fn record(engine: &Engine, world: usize, entity: u32) -> Option<(u64, usize)> {
    // SAFETY: the client world's id-to-slot map, a lookup that changes
    // nothing.
    let slot = unsafe {
        (engine.entity_slot)(
            core::ptr::with_exposed_provenance(world + layout::ENTITY_MAP),
            &raw const entity,
        )
    };
    let capacity = u32_at(world + layout::ENTITY_CAPACITY)?;
    if slot >= u64::from(capacity) {
        return None;
    }
    let records = memory::read_u64(world + layout::ENTITY_RECORDS)?;
    let slot_offset = usize::try_from(slot)
        .ok()?
        .checked_mul(size_of::<usize>())?;
    let record = memory::read_u64(usize::try_from(records).ok()?.checked_add(slot_offset)?)?;
    let record = usize::try_from(record).ok().filter(|&r| r != 0)?;
    Some((slot, record))
}

/// The template GUID a record points at.
fn record_template(record: usize) -> Option<[u8; 16]> {
    let pointer = memory::read_u64(record + layout::RECORD_TEMPLATE)?;
    let mut out = [0u8; 16];
    (memory::read(usize::try_from(pointer).ok()?, &mut out) == out.len()).then_some(out)
}

/// The client entity's component of type `hash`, when it has one.
fn component(engine: &Engine, world: usize, entity: u32, hash: u32) -> Option<usize> {
    let registry = usize::try_from(memory::read_u64(world)?).ok()?;
    let index = layout::REGISTRY_MAPS.iter().find_map(|&map| {
        // SAFETY: the registry's type maps; a lookup that changes nothing.
        let index = unsafe {
            (engine.type_index)(
                core::ptr::with_exposed_provenance(registry + map),
                &raw const hash,
            )
        };
        (!index.is_null()).then_some(index)
    })?;
    let mut out = [0usize; 2];
    // SAFETY: a read with no side effects, inside a running system of this
    // world.
    unsafe {
        (engine.read_component)(
            &raw mut out,
            core::ptr::with_exposed_provenance(world),
            index,
            entity,
        );
    };
    (out[0] != 0).then_some(out[0])
}

fn client_sweep(engine: &Engine, world: usize) {
    let Some(&(player, at)) = walk(engine, world, engine.local_player).first() else {
        enshwick_loader_api::info!("R11 client sweep: no local player in the client world yet");
        return;
    };
    let all = walk(engine, world, engine.transform);
    let mut near: Vec<_> = all
        .iter()
        .map(|&(entity, place)| (distance(place, at), entity, place))
        .filter(|&(d, ..)| d <= RADIUS)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    enshwick_loader_api::info!(
        "R11 client sweep: local player entity {} at {}; {} entities with a transform, {} within {RADIUS} m (up to {MAX_LOGGED} shown)",
        id(player),
        position(at),
        all.len(),
        near.len()
    );
    for (d, entity, place) in near.into_iter().take(MAX_LOGGED) {
        let found = record(engine, world, entity);
        let template = found
            .and_then(|(_, r)| record_template(r))
            .map_or_else(|| "?".to_owned(), |t| guid(&t));
        let slot = found.map_or_else(|| "none".to_owned(), |(s, _)| s.to_string());
        let refs: Vec<String> = engine
            .references
            .iter()
            .filter_map(|&(name, hash, offset)| {
                let data = component(engine, world, entity, hash)?;
                let target = u32_at(data + offset)?;
                let names = record(engine, world, target)
                    .and_then(|(_, r)| record_template(r))
                    .map_or_else(|| "not here".to_owned(), |t| guid(&t));
                Some(format!("{name}={} ({names})", id(target)))
            })
            .collect();
        enshwick_loader_api::info!(
            "R11 client entity {} slot {slot} template {template} at {} d={d:.1}{}",
            id(entity),
            position(place),
            listed(&refs)
        );
        if let Some((_, address)) = found {
            let mut bytes = [0u8; layout::RECORD_SHOWN];
            let read = memory::read(address, &mut bytes);
            for line in memory::words("record", &bytes[..read]) {
                enshwick_loader_api::info!("{line}");
            }
        }
    }
}

fn u32_at(address: usize) -> Option<u32> {
    let mut bytes = [0u8; 4];
    (memory::read(address, &mut bytes) == bytes.len()).then(|| u32::from_le_bytes(bytes))
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f64>()
        .sqrt()
}

/// An entity id in decimal and hex, the hex showing any high bits.
fn id(entity: u32) -> String {
    format!("{entity} ({entity:#x})")
}

fn position(at: [f64; 3]) -> String {
    format!("({:.2}, {:.2}, {:.2})", at[0], at[1], at[2])
}

fn guid(bytes: &[u8; 16]) -> String {
    use core::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(32), |mut out, b| {
        let _infallible = write!(out, "{b:02x}");
        out
    })
}

fn listed(refs: &[String]) -> String {
    if refs.is_empty() {
        String::new()
    } else {
        format!(" refs: {}", refs.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_reads_its_entity_and_position() {
        let mut bytes = [0u8; layout::ENTRY_SIZE];
        bytes[..4].copy_from_slice(&0x1234u32.to_le_bytes());
        bytes[8..16].copy_from_slice(&(3i64 << 32).to_le_bytes());
        bytes[16..24].copy_from_slice(&(-(1i64 << 31)).to_le_bytes());
        bytes[24..32].copy_from_slice(&(10i64 << 32).to_le_bytes());
        assert_eq!(entry(&bytes), (0x1234, [3.0, -0.5, 10.0]));
    }

    #[test]
    fn ids_and_guids_read_plainly() {
        assert_eq!(id(0x8000_0001), "2147483649 (0x80000001)");
        let mut g = [0u8; 16];
        g[0] = 0xab;
        g[15] = 0x01;
        assert_eq!(guid(&g), "ab000000000000000000000000000001");
        assert_eq!(listed(&[]), "");
        assert_eq!(position([1.0, 2.5, -3.0]), "(1.00, 2.50, -3.00)");
    }

    #[test]
    fn distance_is_euclidean() {
        assert!((distance([0.0, 0.0, 0.0], [3.0, 4.0, 0.0]) - 5.0).abs() < 1e-9);
    }
}
