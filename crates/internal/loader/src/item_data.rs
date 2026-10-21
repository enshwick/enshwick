//! Data mods attach to items and other entities: kinds named `<mod>:<name>`,
//! a value per entity and kind, kept per world and, for kinds that say so,
//! sent to the clients that run Enshwick.
//!
//! A record keeps, beside its value, what its entity was: for an item kind
//! the item id its entity, the PIDE, held; for an entity kind the game id of
//! the entity's template. The game saves entities under their ids and
//! creates them with those ids on load, so the id names the entity across
//! saves; a record reads as missing once its entity is something else, and
//! the save drops the records of entities that hold something else now.
//! Entities the game destroys lose their records after the step that asked
//! for it: the loader follows the engine's destroy requests, which only
//! gameplay makes; an entity in an area that unloads keeps its record.
//!
//! The server keeps the values and writes them, when the world saves, to
//! `mod-data/enshwick/worlds/<key>/attachments.bin`, never into the game's
//! save. Every change of a sent kind goes to the clients that said hello,
//! and a client that says hello gets every value of those kinds; a client
//! keeps what it got and answers reads from it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{Event, HookFlags, Status};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::chains::Next;
use crate::host::Loader;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &["ecs.destroy_request"];

/// The longest kind id: a mod id, a colon and a name of up to 64 each.
const MAX_ID: usize = 129;
/// The largest value, so a record fits one message frame.
pub(crate) const MAX_VALUE: usize = 512;
/// The file of a world's values.
const FILE: &str = "attachments.bin";
/// The file's magic and version.
const MAGIC: &[u8; 4] = b"EWID";
const VERSION: u8 = 1;
/// Bytes of records in one message to a client.
const MESSAGE_BYTES: usize = 680;
/// The most records a client keeps from one server: what a server sends
/// past it is dropped, so a server cannot fill the client's memory.
const MAX_CLIENT_RECORDS: usize = 1 << 16;
/// Destroy requests noted in one step at most; past it an entity keeps its
/// record until its id holds something else.
const DESTROYED_CAP: usize = 1024;

/// One entity's value of one kind, and what the entity was: the item id an
/// item entity held, or an entity's template id.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Record {
    item: u32,
    value: Box<[u8]>,
}

/// What a kind's values belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// Items with an entity of their own, checked by the item they hold.
    Item,
    /// Any entity, checked by its template.
    Entity,
}

/// A kind: its full id, what its values belong to and whether clients get
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Kind {
    id: String,
    scope: Scope,
    sent: bool,
}

/// Values by kind index and item entity.
type Values = FxHashMap<(u32, u32), Record>;

/// The server's values and the world they belong to.
#[derive(Debug, Default)]
struct Server {
    key: Option<String>,
    values: Values,
    /// The file's records of kinds no mod opened this session, as read,
    /// written back unchanged for when their mods return.
    unknown: Vec<u8>,
    changed: bool,
}

/// Kinds while mods initialise.
static PENDING: Mutex<Vec<Kind>> = Mutex::new(Vec::new());
/// Kinds once sealed; the index is the kind.
static KINDS: OnceLock<Box<[Kind]>> = OnceLock::new();
static SERVER: Mutex<Option<Server>> = Mutex::new(None);
/// What a client got from its server.
static CLIENT: Mutex<Option<Values>> = Mutex::new(None);
/// Whether the client said once that it dropped records over
/// [`MAX_CLIENT_RECORDS`] from this server.
static CLIENT_FULL: AtomicBool = AtomicBool::new(false);
/// Whether the server keeps any record, so destroy requests are noted.
static KEPT: AtomicBool = AtomicBool::new(false);
/// Entities the game asked to destroy since the last step, 0 for an empty
/// slot; written from any thread without a lock.
static DESTROYED: [AtomicU32; DESTROYED_CAP] = [const { AtomicU32::new(0) }; DESTROYED_CAP];
static DESTROYED_COUNT: AtomicUsize = AtomicUsize::new(0);

/// `ecs.destroy_request(command_buffer, requester, entity)`: queues the
/// entity's destroy for the playback after the next step; nonzero when
/// queued.
type DestroyFn = unsafe extern "system" fn(buffer: *mut u8, requester: u32, entity: u32) -> u8;
// SAFETY: the slot joins the chain of `ecs.destroy_request`, whose type
// `DestroyFn` names.
static DESTROY_NEXT: Next<DestroyFn> = unsafe { Next::new() };
/// `ecs::ItemState`'s type hash and the offset of its `itemId`.
static ITEM_STATE: OnceLock<Option<(u32, usize)>> = OnceLock::new();

/// Whether `part` may be one side of a kind id: 1 to 64 of `[a-z0-9_-]`.
fn is_part(part: &str) -> bool {
    (1..=64).contains(&part.len())
        && part
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The full id of `id` asked for by mod `mod_id`: `name` names the mod's
/// own kind, `mod:name` any mod's.
fn full_id(mod_id: &str, id: &str) -> Option<String> {
    let (namespace, name) = id.split_once(':').unwrap_or((mod_id, id));
    (is_part(namespace) && is_part(name)).then(|| format!("{namespace}:{name}"))
}

/// Opens item kind `id` for mod `owner` and returns its index; the same id
/// gives the same index to every mod. During init only.
pub(crate) fn open(loader: &Loader, owner: usize, id: &str) -> Result<u32, Status> {
    open_kind(loader, owner, id, Scope::Item, true)
}

/// Opens entity kind `id` for mod `owner`, sent to clients when `sent`
/// (once any mod asks for it), and returns its index. During init only.
pub(crate) fn open_entity(
    loader: &Loader,
    owner: usize,
    id: &str,
    sent: bool,
) -> Result<u32, Status> {
    open_kind(loader, owner, id, Scope::Entity, sent)
}

fn open_kind(
    loader: &Loader,
    owner: usize,
    id: &str,
    scope: Scope,
    sent: bool,
) -> Result<u32, Status> {
    if KINDS.get().is_some() {
        return Err(Status::TOO_LATE);
    }
    let record = loader.mods.get(owner).ok_or(Status::BAD_ARGS)?;
    let full = full_id(&record.id, id).ok_or(Status::BAD_ARGS)?;
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    let index = if let Some(index) = pending.iter().position(|k| k.id == full) {
        let kind = &mut pending[index];
        if kind.scope != scope {
            crate::log::write(
                enshwick_loader_api::abi::Level::ERROR,
                &record.id,
                &format!("data kind {full} is opened for items and for entities"),
            );
            return Err(Status::REFUSED);
        }
        kind.sent |= sent;
        index
    } else {
        pending.push(Kind {
            id: full,
            scope,
            sent,
        });
        pending.len() - 1
    };
    u32::try_from(index).map_err(|_| Status::REFUSED)
}

/// Opens the loader's own item kind `id` for a platform feature, before the
/// kinds are sealed.
pub(crate) fn open_platform(id: &str) {
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    if !pending.iter().any(|k| k.id == id) {
        pending.push(Kind {
            id: id.to_owned(),
            scope: Scope::Item,
            sent: true,
        });
    }
}

/// The value of the loader's own kind `id` on the item `pide` holding
/// `item`.
pub(crate) fn get_platform_kind(id: &str, pide: u32, item: u32) -> Option<Box<[u8]>> {
    get(kind_of(id)?, pide, item).ok()
}

/// Sets the item stats of the item `pide` holding `item`, as a mod's set
/// would.
pub(crate) fn set_platform(pide: u32, item: u32, value: &[u8]) -> Result<(), Status> {
    let kind = kind_of(enshwick_loader_api::stats::ITEM_KIND).ok_or(Status::MISSING)?;
    set(kind, pide, item, value)
}

/// The item stats of the item `pide` holding `item`.
pub(crate) fn get_platform(pide: u32, item: u32) -> Option<Box<[u8]>> {
    get_platform_kind(enshwick_loader_api::stats::ITEM_KIND, pide, item)
}

/// Seals the kinds; the store runs only when a mod opened one.
pub(crate) fn seal() {
    let kinds = std::mem::take(&mut *PENDING.lock().unwrap_or_else(PoisonError::into_inner));
    if !kinds.is_empty() {
        let names: Vec<String> = kinds
            .iter()
            .map(|k| match (k.scope, k.sent) {
                (Scope::Item, _) => format!("{} (items)", k.id),
                (Scope::Entity, true) => format!("{} (entities, sent)", k.id),
                (Scope::Entity, false) => format!("{} (entities)", k.id),
            })
            .collect();
        crate::log!(INFO, "mod data: kinds {}", names.join(", "));
    }
    let _first = KINDS.set(kinds.into_boxed_slice());
}

/// Follows the game's destroy requests, when a mod keeps item data.
pub(crate) fn link(loader: &Loader) {
    if !wanted() {
        return;
    }
    loader.link_feature(
        "mod data of destroyed entities",
        loader.address("ecs.destroy_request"),
        &DESTROY_NEXT,
        destroy,
        HookFlags::NONE,
    );
}

unsafe extern "system" fn destroy(buffer: *mut u8, requester: u32, entity: u32) -> u8 {
    // SAFETY: forwarded unchanged.
    let queued = unsafe { DESTROY_NEXT.get()(buffer, requester, entity) };
    if queued != 0 && entity != 0 && KEPT.load(Ordering::Relaxed) {
        note_destroyed(entity);
    }
    queued
}

/// Notes that the game asked to destroy `entity`.
fn note_destroyed(entity: u32) {
    let at = DESTROYED_COUNT.fetch_add(1, Ordering::AcqRel);
    if let Some(slot) = DESTROYED.get(at) {
        slot.store(entity, Ordering::Release);
    }
}

/// The entities noted since the last call.
fn take_destroyed() -> FxHashSet<u32> {
    let count = DESTROYED_COUNT.swap(0, Ordering::AcqRel).min(DESTROYED_CAP);
    DESTROYED[..count]
        .iter()
        .map(|slot| slot.swap(0, Ordering::AcqRel))
        .filter(|&entity| entity != 0)
        .collect()
}

/// Drops the records of the `destroyed` entities from `values`; the keys
/// dropped.
fn prune_destroyed(values: &mut Values, destroyed: &FxHashSet<u32>) -> Vec<(u32, u32)> {
    let gone: Vec<(u32, u32)> = values
        .keys()
        .filter(|(_, pide)| destroyed.contains(pide))
        .copied()
        .collect();
    for key in &gone {
        values.remove(key);
    }
    gone
}

/// After the server's step: drops the records of the entities destroyed in
/// it, and tells the clients.
pub(crate) fn after_step() {
    if !KEPT.load(Ordering::Relaxed) || DESTROYED_COUNT.load(Ordering::Acquire) == 0 {
        return;
    }
    let destroyed = take_destroyed();
    let gone = {
        let mut server = SERVER.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(server) = server.as_mut() else {
            return;
        };
        let gone = prune_destroyed(&mut server.values, &destroyed);
        server.changed |= !gone.is_empty();
        gone
    };
    let mut message = Vec::new();
    for (kind, pide) in gone {
        if let Some(spec) = self::kind(kind).filter(|k| k.sent) {
            encode(&mut message, &spec.id, pide, 0, &[]);
            if message.len() > MESSAGE_BYTES {
                crate::net::item_data(0, &std::mem::take(&mut message));
            }
        }
    }
    if !message.is_empty() {
        crate::net::item_data(0, &message);
    }
}

/// Whether any mod keeps item data, which needs the world events and the
/// clients' hello.
pub(crate) fn wanted() -> bool {
    KINDS.get().is_some_and(|k| !k.is_empty())
}

fn kind(kind: u32) -> Option<&'static Kind> {
    KINDS.get()?.get(usize::try_from(kind).ok()?)
}

fn kind_id(kind: u32) -> Option<&'static str> {
    self::kind(kind).map(|k| k.id.as_str())
}

fn kind_of(id: &str) -> Option<u32> {
    let index = KINDS.get()?.iter().position(|k| k.id == id)?;
    u32::try_from(index).ok()
}

/// The game id of `entity`'s template, on the server's game thread; `None`
/// for an entity that is gone or off that thread.
fn template_id(entity: u32) -> Option<u32> {
    let loader = crate::host::loader()?;
    let template = crate::world::template(loader, entity).ok()?;
    Some(enshwick_loader_api::content::Guid(template).id())
}

/// Whether this process runs the world the values belong to: the dedicated
/// server, or a client while it hosts.
fn serves() -> bool {
    crate::world::reader().is_ok()
}

/// The value of kind `kind` on the item `pide` holding `item`, or for an
/// entity kind on the entity `pide` (`item` 0). On the server's game thread
/// an entity whose template changed loses its value.
pub(crate) fn get(kind: u32, pide: u32, item: u32) -> Result<Box<[u8]>, Status> {
    let scope = self::kind(kind).ok_or(Status::BAD_ARGS)?.scope;
    if serves() {
        let mut server = SERVER.lock().unwrap_or_else(PoisonError::into_inner);
        let server = server.as_mut().ok_or(Status::MISSING)?;
        let record = server.values.get(&(kind, pide)).ok_or(Status::MISSING)?;
        let wanted = match scope {
            Scope::Item => item,
            // An entity in an area that is not loaded is not in the world's
            // map either: its value stays, unchecked.
            Scope::Entity => match template_id(pide) {
                Some(now) => now,
                None => return Ok(record.value.clone()),
            },
        };
        if record.item == wanted {
            return Ok(record.value.clone());
        }
        if scope == Scope::Entity {
            server.values.remove(&(kind, pide));
            server.changed = true;
        }
        return Err(Status::MISSING);
    }
    let client = CLIENT.lock().unwrap_or_else(PoisonError::into_inner);
    client
        .as_ref()
        .and_then(|v| v.get(&(kind, pide)).cloned())
        .filter(|r| scope == Scope::Entity || r.item == item)
        .map(|r| r.value)
        .ok_or(Status::MISSING)
}

/// Sets, or for an empty value removes, kind `kind`'s value on the item
/// `pide` holding `item`, and sends the change to the clients. On the
/// server only, once its world has loaded.
pub(crate) fn set(kind: u32, pide: u32, item: u32, value: &[u8]) -> Result<(), Status> {
    let spec = self::kind(kind).ok_or(Status::BAD_ARGS)?;
    let id = spec.id.as_str();
    if pide == 0 || value.len() > MAX_VALUE {
        return Err(Status::BAD_ARGS);
    }
    if !serves() {
        return Err(Status::REFUSED);
    }
    let item = match spec.scope {
        Scope::Item if item == 0 => return Err(Status::BAD_ARGS),
        Scope::Item => item,
        Scope::Entity => {
            crate::world::server().map_err(|_| Status::WRONG_THREAD)?;
            match template_id(pide) {
                Some(template) => template,
                None if value.is_empty() => 0,
                None => return Err(Status::BAD_ARGS),
            }
        }
    };
    {
        let mut server = SERVER.lock().unwrap_or_else(PoisonError::into_inner);
        let server = server.as_mut().ok_or(Status::MISSING)?;
        if value.is_empty() {
            server.values.remove(&(kind, pide));
        } else {
            let record = Record {
                item,
                value: value.into(),
            };
            server.values.insert((kind, pide), record);
            KEPT.store(true, Ordering::Relaxed);
        }
        server.changed = true;
    }
    if id == enshwick_loader_api::stats::ITEM_KIND {
        crate::stats::items_changed();
    }
    if !spec.sent {
        return Ok(());
    }
    let mut message = Vec::new();
    encode(&mut message, id, pide, item, value);
    crate::net::item_data(0, &message);
    Ok(())
}

/// Appends one record: the kind id, the entity, the item and the value.
fn encode(out: &mut Vec<u8>, id: &str, pide: u32, item: u32, value: &[u8]) {
    out.push(u8::try_from(id.len()).unwrap_or(u8::MAX));
    out.extend_from_slice(id.as_bytes());
    out.extend_from_slice(&pide.to_le_bytes());
    out.extend_from_slice(&item.to_le_bytes());
    out.extend_from_slice(&u16::try_from(value.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(value);
}

/// The records in `bytes`, each `(id, pide, item, value)`; stops at the
/// first that does not read.
fn decode(mut bytes: &[u8]) -> Vec<(&str, u32, u32, &[u8])> {
    let mut records = Vec::new();
    while let Some((&len, rest)) = bytes.split_first() {
        let len = usize::from(len);
        if len > MAX_ID || rest.len() < len + 10 {
            break;
        }
        let (id, rest) = rest.split_at(len);
        let Ok(id) = core::str::from_utf8(id) else {
            break;
        };
        let word =
            |at: usize| u32::from_le_bytes([rest[at], rest[at + 1], rest[at + 2], rest[at + 3]]);
        let (pide, item) = (word(0), word(4));
        let size = usize::from(u16::from_le_bytes([rest[8], rest[9]]));
        let Some(value) = rest.get(10..10 + size) else {
            break;
        };
        records.push((id, pide, item, value));
        bytes = &rest[10 + size..];
    }
    records
}

/// The messages that give a client every value, each at most
/// [`MESSAGE_BYTES`] long.
fn snapshot() -> Vec<Vec<u8>> {
    let server = SERVER.lock().unwrap_or_else(PoisonError::into_inner);
    let mut messages = Vec::new();
    let mut message = Vec::new();
    for (&(kind, pide), record) in server.iter().flat_map(|s| &s.values) {
        let Some(id) = self::kind(kind).filter(|k| k.sent).map(|k| k.id.as_str()) else {
            continue;
        };
        let mut one = Vec::new();
        encode(&mut one, id, pide, record.item, &record.value);
        if message.len() + one.len() > MESSAGE_BYTES && !message.is_empty() {
            messages.push(std::mem::take(&mut message));
        }
        message.extend_from_slice(&one);
    }
    if !message.is_empty() {
        messages.push(message);
    }
    messages
}

/// The messages a client that just said hello gets.
pub(crate) fn hello() -> Vec<Vec<u8>> {
    if wanted() && serves() {
        snapshot()
    } else {
        Vec::new()
    }
}

/// Takes the records the server sent, on a client.
pub(crate) fn received(bytes: &[u8]) {
    if !wanted() {
        return;
    }
    let mut client = CLIENT.lock().unwrap_or_else(PoisonError::into_inner);
    let values = client.get_or_insert_with(Values::default);
    if !keep(values, bytes) && !CLIENT_FULL.swap(true, Ordering::Relaxed) {
        crate::log!(
            WARN,
            "mod data: the server sent more than {MAX_CLIENT_RECORDS} records; the rest are dropped"
        );
    }
}

/// Keeps the records in `bytes` in `values`, at most
/// [`MAX_CLIENT_RECORDS`]; false when one was dropped for that.
fn keep(values: &mut Values, bytes: &[u8]) -> bool {
    let mut kept_all = true;
    for (id, pide, item, value) in decode(bytes) {
        let Some(kind) = kind_of(id) else {
            continue;
        };
        if value.is_empty() {
            values.remove(&(kind, pide));
            continue;
        }
        if values.len() >= MAX_CLIENT_RECORDS && !values.contains_key(&(kind, pide)) {
            kept_all = false;
            continue;
        }
        values.insert(
            (kind, pide),
            Record {
                item,
                value: value.into(),
            },
        );
    }
    kept_all
}

/// Forgets what the client got, when it changes servers.
pub(crate) fn forget_client() {
    *CLIENT.lock().unwrap_or_else(PoisonError::into_inner) = None;
    CLIENT_FULL.store(false, Ordering::Relaxed);
}

/// The file of world `key`'s values.
fn file(loader: &Loader, key: &str) -> PathBuf {
    loader
        .data_dir
        .join("enshwick")
        .join("worlds")
        .join(key)
        .join(FILE)
}

/// The values a world's file holds, and its records of kinds no mod
/// opened; nothing for a file that does not read.
fn read_file(bytes: &[u8]) -> (Values, Vec<u8>) {
    let mut values = Values::default();
    let mut unknown = Vec::new();
    let Some(records) = bytes
        .strip_prefix(MAGIC.as_slice())
        .and_then(|rest| rest.strip_prefix(&[VERSION]))
    else {
        return (values, unknown);
    };
    for (id, pide, item, value) in decode(records) {
        if value.is_empty() {
            continue;
        }
        let Some(kind) = kind_of(id) else {
            encode(&mut unknown, id, pide, item, value);
            continue;
        };
        values.insert(
            (kind, pide),
            Record {
                item,
                value: value.into(),
            },
        );
    }
    (values, unknown)
}

fn write_file(values: &Values, unknown: &[u8]) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.push(VERSION);
    let mut sorted: Vec<_> = values.iter().collect();
    sorted.sort_by_key(|(key, _)| **key);
    for (&(kind, pide), record) in sorted {
        if let Some(id) = kind_id(kind) {
            encode(&mut bytes, id, pide, record.item, &record.value);
        }
    }
    bytes.extend_from_slice(unknown);
    bytes
}

/// The item entity `pide` now holds, from its `ItemState`; `None` for an
/// entity that is gone.
fn held(loader: &Loader, pide: u32) -> Option<u32> {
    let (ty, offset) = (*ITEM_STATE.get_or_init(|| {
        let info = loader.reflection.type_info("keen::ecs::ItemState").ok()?;
        let ty = crate::world::type_hash(info.va())?;
        let offset = loader
            .reflection
            .field_offset("keen::ecs::ItemState", "itemId")
            .ok()?;
        Some((ty, usize::try_from(offset).ok()?))
    }))?;
    let data = crate::world::component(loader, pide, ty).ok()?;
    // SAFETY: a live ItemState holds its item id at the reflected offset,
    // read on the game thread.
    Some(unsafe {
        data.cast::<u8>()
            .wrapping_add(offset)
            .cast::<u32>()
            .read_unaligned()
    })
}

/// Follows the server's world: reads the world's values once it has
/// loaded, writes them when it saves and when it shuts down.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    if !wanted() || !serves() {
        return;
    }
    match event {
        Event::WORLD_LOADED => {
            let key = crate::saves::world_key(loader).map(str::to_owned);
            let (values, unknown) = key
                .as_deref()
                .and_then(|key| std::fs::read(file(loader, key)).ok())
                .map(|bytes| read_file(&bytes))
                .unwrap_or_default();
            crate::log!(
                INFO,
                "mod data: {} values for world {}",
                values.len(),
                key.as_deref().unwrap_or("?")
            );
            KEPT.store(!values.is_empty(), Ordering::Relaxed);
            *SERVER.lock().unwrap_or_else(PoisonError::into_inner) = Some(Server {
                key,
                values,
                unknown,
                changed: false,
            });
        }
        Event::WORLD_SAVED | Event::SHUTDOWN => save(loader, event == Event::WORLD_SAVED),
        _ => {}
    }
}

/// Writes the values when they changed; after a save of the game's, first
/// drops the records whose entity now holds another item or template.
fn save(loader: &Loader, prune: bool) {
    let (key, bytes) = {
        let mut server = SERVER.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(server) = server.as_mut() else {
            return;
        };
        if prune {
            let before = server.values.len();
            server.values.retain(|&(kind, pide), record| {
                let now = match self::kind(kind).map(|k| k.scope) {
                    Some(Scope::Item) => held(loader, pide),
                    Some(Scope::Entity) => template_id(pide),
                    None => return true,
                };
                // An entity the world does not hold may lie in an area that
                // is not loaded, so only one that holds something else loses
                // its record.
                now.is_none_or(|now| now == record.item)
            });
            server.changed |= server.values.len() != before;
        }
        if !server.changed {
            return;
        }
        let Some(key) = server.key.clone() else {
            return;
        };
        server.changed = false;
        (key, write_file(&server.values, &server.unknown))
    };
    let path = file(loader, &key);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            let fresh = path.with_extension("tmp");
            std::fs::write(&fresh, &bytes)?;
            std::fs::rename(&fresh, &path)
        });
    if let Err(error) = written {
        crate::log!(WARN, "mod data: {} not written: {error}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_take_the_mod_s_namespace_unless_they_name_one() {
        assert_eq!(
            full_id("survivors", "charges").as_deref(),
            Some("survivors:charges")
        );
        assert_eq!(
            full_id("probe", "other:fire_damage").as_deref(),
            Some("other:fire_damage")
        );
        assert_eq!(full_id("probe", "Charges"), None);
        assert_eq!(full_id("probe", "a:b:c"), None);
        assert_eq!(full_id("probe", ":x"), None);
    }

    #[test]
    fn a_destroyed_entity_loses_its_records_and_an_unloaded_one_keeps_them() {
        let record = |item| Record {
            item,
            value: Box::new([1]),
        };
        let mut values = Values::default();
        values.insert((0, 900), record(7));
        values.insert((1, 900), record(0));
        values.insert((0, 901), record(7));
        note_destroyed(900);
        note_destroyed(0);
        let destroyed = take_destroyed();
        assert!(take_destroyed().is_empty(), "each request is taken once");
        let mut gone = prune_destroyed(&mut values, &destroyed);
        gone.sort_unstable();
        assert_eq!(gone, [(0, 900), (1, 900)]);
        // 901 is merely not loaded: no destroy request names it.
        assert_eq!(values.len(), 1);
        assert!(values.contains_key(&(0, 901)));
        // Past the cap in one step, requests are left for the save.
        for entity in 1..=u32::try_from(DESTROYED_CAP).unwrap() + 10 {
            note_destroyed(entity);
        }
        let destroyed = take_destroyed();
        assert_eq!(destroyed.len(), DESTROYED_CAP);
        assert!(!destroyed.contains(&(u32::try_from(DESTROYED_CAP).unwrap() + 1)));
    }

    #[test]
    fn a_client_keeps_at_most_its_cap_of_a_server_s_records() {
        let _sealed = KINDS.set(
            vec![Kind {
                id: "a:charges".into(),
                scope: Scope::Item,
                sent: true,
            }]
            .into_boxed_slice(),
        );
        let mut values = Values::default();
        let mut bytes = Vec::new();
        for pide in 1..=u32::try_from(MAX_CLIENT_RECORDS).unwrap() {
            encode(&mut bytes, "a:charges", pide, 7, &[1]);
        }
        assert!(keep(&mut values, &bytes));
        assert_eq!(values.len(), MAX_CLIENT_RECORDS);
        let mut more = Vec::new();
        encode(&mut more, "a:charges", 1, 7, &[2]);
        encode(&mut more, "a:charges", u32::MAX, 7, &[1]);
        assert!(
            !keep(&mut values, &more),
            "a new record past the cap is dropped"
        );
        assert_eq!(values.len(), MAX_CLIENT_RECORDS);
        assert_eq!(&*values[&(0, 1)].value, &[2], "a known one still changes");
        let mut removal = Vec::new();
        encode(&mut removal, "a:charges", 1, 0, &[]);
        assert!(keep(&mut values, &removal));
        assert_eq!(values.len(), MAX_CLIENT_RECORDS - 1);
    }

    #[test]
    fn records_of_kinds_no_mod_opened_survive_a_save() {
        let mut bytes = MAGIC.to_vec();
        bytes.push(VERSION);
        encode(&mut bytes, "gone:charges", 900, 7, &[3]);
        encode(&mut bytes, "gone:empty", 901, 7, &[]);
        let (values, unknown) = read_file(&bytes);
        assert!(values.is_empty());
        let written = write_file(&values, &unknown);
        let records = decode(&written[MAGIC.len() + 1..]);
        assert_eq!(records, [("gone:charges", 900, 7, &[3u8][..])]);
    }

    #[test]
    fn records_read_back_as_written_and_a_cut_one_ends_the_list() {
        let mut bytes = Vec::new();
        encode(&mut bytes, "a:charges", 900, 7, &[3]);
        encode(&mut bytes, "b:name", 901, 8, b"Oswald");
        encode(&mut bytes, "a:charges", 902, 7, &[]);
        let records = decode(&bytes);
        assert_eq!(
            records,
            [
                ("a:charges", 900, 7, &[3u8][..]),
                ("b:name", 901, 8, &b"Oswald"[..]),
                ("a:charges", 902, 7, &[][..]),
            ]
        );
        assert_eq!(decode(&bytes[..bytes.len() - 3]).len(), 2);
        assert!(decode(&[200, 1, 2]).is_empty());
    }
}
