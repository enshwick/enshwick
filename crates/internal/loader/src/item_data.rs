//! Data mods attach to items and other entities: kinds named `<mod>:<name>`,
//! a value per entity and kind, kept per world and, for kinds that say so,
//! sent to the clients that run Enshwick.
//!
//! A record keeps, beside its value, what its entity was: for an item kind
//! the item id its entity, the PIDE, held; for an entity kind the game id of
//! the entity's template. Within a session the entity id names the entity;
//! a record reads as missing once its entity is something else, and the
//! save drops the records of entities that hold something else now.
//! Entities the game destroys lose their records after the step that asked
//! for it: the loader follows the engine's destroy requests, which only
//! gameplay makes. One gone without a request it saw goes aside when the
//! world saves.
//!
//! A load numbers every entity anew, so the file keeps each record under an
//! [`Anchor`] instead of its id: a placed thing's template and transform, an
//! item's place in a placed thing's inventory. After the first step of a
//! loaded world the records find their entities again. This rests on a
//! load creating every entity its save holds before that step (server
//! 399). A record whose place names no entity,
//! or more than one, goes aside: it stays in the file, flagged so it never
//! attaches again, since a later entity may take its place.
//!
//! The server keeps the values and writes them, when the world saves, to
//! `mod-data/enshwick/worlds/<key>/attachments.bin`, never into the game's
//! save. Every change of a sent kind goes to the clients that said hello,
//! and a client that says hello gets every value of those kinds; a client
//! keeps what it got and answers reads from it.

mod attach;
mod carried;
mod game;
mod world_file;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use enshwick::Entity;
use enshwick::abi::{HookFlags, Status};
use rustc_hash::{FxHashMap, FxHashSet};

use self::attach::find_pending;
use self::carried::{JOIN_LOOK_EVERY, Joining, find_carried};
pub(crate) use self::carried::{player_joined, player_left};
pub(crate) use self::world_file::world_event;
use crate::chains::Next;
use crate::host::{Loader, Target};
use crate::item_records::anchor::{self, Anchor, Pending};
use crate::item_records::{decode, encode};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[pinned::ecs::DESTROY_REQUEST.id()];

/// The largest value, so a record fits one message frame.
pub(crate) const MAX_VALUE: usize = 512;
/// The file of a world's values.
const FILE: &str = "attachments.bin";
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
    /// What finds the entity after a load, as last read; `None` on a client
    /// and for an item not yet seen in an inventory.
    anchor: Option<Anchor>,
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
    /// The file's records that have not found their entity, written back
    /// unchanged: before the first step, and those aside after it.
    pending: Vec<Pending>,
    /// The kinds of the file no mod opened this session, numbered after the
    /// opened ones. Their records find their entities and follow them like
    /// any other, so they are right when their mod returns; no mod reads
    /// them meanwhile.
    unopened: Vec<String>,
    /// The joined characters whose carried records wait; at most one entry
    /// per player.
    joining: Vec<Joining>,
    changed: bool,
}

impl Server {
    /// The id of kind `kind`, opened or not.
    fn kind_id(&self, kind: u32) -> Option<&str> {
        let kind = usize::try_from(kind).ok()?;
        let opened = KINDS.sealed().unwrap_or_default();
        match opened.get(kind) {
            Some(spec) => Some(&spec.id),
            None => self.unopened.get(kind - opened.len()).map(String::as_str),
        }
    }
}

/// The scope of kind `kind`'s record at `anchor`: its own for an opened
/// kind, else the one its anchor tells. A live value of a kind no mod
/// opened has no anchor only once its item left every placed inventory: an
/// entity value always keeps its place.
fn scope(kind: u32, anchor: Option<Anchor>) -> Option<Scope> {
    match (self::kind(kind), anchor) {
        (Some(spec), _) => Some(spec.scope),
        (None, Some(Anchor::Placed(_))) => Some(Scope::Entity),
        (None, Some(Anchor::Held { .. } | Anchor::Carried { .. }) | None) => Some(Scope::Item),
        (None, Some(Anchor::Session(_))) => None,
    }
}

/// Kinds while mods initialise.
/// Kinds once sealed; the index is the kind.
static KINDS: Registry<Kind> = Registry::new();
static SERVER: Mutex<Option<Server>> = Mutex::new(None);
/// What a client got from its server.
static CLIENT: Mutex<Option<Values>> = Mutex::new(None);
/// Whether the client said once that it dropped records over
/// [`MAX_CLIENT_RECORDS`] from this server.
static CLIENT_FULL: AtomicBool = AtomicBool::new(false);
/// Whether the server keeps any record, so destroy requests are noted.
static KEPT: AtomicBool = AtomicBool::new(false);
/// Whether a loaded world's records wait for its first step to find their
/// entities.
static FINDING: AtomicBool = AtomicBool::new(false);
/// Whether the world's placed things did not read at the first step: its
/// records wait unattached, so a value set now would claim a place one of
/// them holds. The store refuses values until the next load.
static UNREAD: AtomicBool = AtomicBool::new(false);
/// Entities the game asked to destroy since the last step, each in the low
/// word under its round in the high one, 0 for an empty slot; written from
/// any thread without a lock. A slot from an earlier round is stale.
static DESTROYED: [AtomicU64; DESTROYED_CAP] = [const { AtomicU64::new(0) }; DESTROYED_CAP];
/// The round in the high word and the slots taken in it in the low one.
static DESTROYED_COUNT: AtomicU64 = AtomicU64::new(1 << 32);

/// `ecs.destroy_request(command_buffer, requester, entity)`: queues the
/// entity's destroy for the playback after the next step; nonzero when
/// queued.
type DestroyFn = unsafe extern "system" fn(buffer: *mut u8, requester: u32, entity: u32) -> u8;
// SAFETY: `DestroyFn` is the exact signature of the game function at the
// table's `ecs.destroy_request`, and the slot is used only as the `next` of
// the `destroy` link on that function.
static DESTROY_NEXT: Next<DestroyFn> = unsafe { Next::new() };

/// Whether `part` may be the name side of a kind id: 1 to 64 of `[a-z0-9_-]`.
fn is_part(part: &str) -> bool {
    (1..=64).contains(&part.len())
        && part
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The full id of `id` asked for by a mod whose bare names take the
/// namespace `mod_id`: `name` names the mod's own kind, `mod:name` any
/// mod's.
fn full_id(mod_id: &str, id: &str) -> Option<String> {
    let (namespace, name) = id.split_once(':').unwrap_or((mod_id, id));
    (enshwick::abi::is_valid_old_form_id(namespace) && is_part(name))
        .then(|| format!("{namespace}:{name}"))
}

/// Opens item kind `id` for mod `owner` and returns its index; the same id
/// gives the same index to every mod. During init only.
pub(crate) fn open(loader: &Loader, owner: ModIndex, id: &str) -> Result<u32, Status> {
    open_kind(loader, owner, id, Scope::Item, true)
}

/// Opens entity kind `id` for mod `owner`, sent to clients when `sent`
/// (once any mod asks for it), and returns its index. During init only.
pub(crate) fn open_entity(
    loader: &Loader,
    owner: ModIndex,
    id: &str,
    sent: bool,
) -> Result<u32, Status> {
    open_kind(loader, owner, id, Scope::Entity, sent)
}

fn open_kind(
    loader: &Loader,
    owner: ModIndex,
    id: &str,
    scope: Scope,
    sent: bool,
) -> Result<u32, Status> {
    let record = loader.mods.get(owner.get()).ok_or(Status::BAD_ARGS)?;
    let full = full_id(&record.id, id).ok_or(Status::BAD_ARGS)?;
    let index = KINDS.edit(|pending| {
        if let Some(index) = pending.iter().position(|k| k.id == full) {
            let kind = &mut pending[index];
            if kind.scope != scope {
                crate::log::write(
                    enshwick::abi::Level::ERROR,
                    &record.id,
                    &format!("data kind {full} is opened for items and for entities"),
                );
                return Err(Status::REFUSED);
            }
            kind.sent |= sent;
            Ok(index)
        } else {
            pending.push(Kind {
                id: full,
                scope,
                sent,
            });
            Ok(pending.len() - 1)
        }
    })??;
    u32::try_from(index).map_err(|_| Status::REFUSED)
}

/// Opens the loader's own item kind `id` for a platform feature, before the
/// kinds are sealed.
pub(crate) fn open_platform(id: &str) {
    let _open = KINDS.edit(|pending| {
        if !pending.iter().any(|k| k.id == id) {
            pending.push(Kind {
                id: id.to_owned(),
                scope: Scope::Item,
                sent: true,
            });
        }
    });
}

/// The value of the loader's own kind `id` on the item `pide` holding
/// `item`.
pub(crate) fn get_platform_kind(id: &str, pide: u32, item: u32) -> Option<Box<[u8]>> {
    get(kind_of(id)?, pide, item).ok()
}

/// Sets the item stats of the item `pide` holding `item`, as a mod's set
/// would.
pub(crate) fn set_platform(pide: u32, item: u32, value: &[u8]) -> Result<(), Status> {
    let kind = kind_of(enshwick::stats::ITEM_KIND).ok_or(Status::MISSING)?;
    set(kind, pide, item, value)
}

/// The item stats of the item `pide` holding `item`.
pub(crate) fn get_platform(pide: u32, item: u32) -> Option<Box<[u8]>> {
    get_platform_kind(enshwick::stats::ITEM_KIND, pide, item)
}

/// Seals the kinds; the store runs only when a mod opened one.
pub(crate) fn seal() {
    let kinds = KINDS.seal();
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
}

/// Follows the game's destroy requests, when a mod keeps item data.
pub(crate) fn link(loader: &Loader) {
    if !wanted() {
        return;
    }
    loader.link_feature(
        "mod data of destroyed entities",
        Target::Pinned(pinned::ecs::DESTROY_REQUEST),
        &DESTROY_NEXT,
        destroy,
        HookFlags::NONE,
    );
}

unsafe extern "system" fn destroy(buffer: *mut u8, requester: u32, entity: u32) -> u8 {
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the destroy request's signature.
    let queued = unsafe { DESTROY_NEXT.get()(buffer, requester, entity) };
    if queued != 0 && entity != 0 && KEPT.load(Ordering::Relaxed) {
        note_destroyed(Entity(entity));
    }
    queued
}

/// Notes that the game asked to destroy `entity`.
fn note_destroyed(entity: Entity) {
    fill_destroyed(reserve_destroyed(), entity);
}

/// A slot for a destroy request: its index and its round.
fn reserve_destroyed() -> (usize, u64) {
    let count = DESTROYED_COUNT.fetch_add(1, Ordering::AcqRel);
    ((count & 0xffff_ffff) as usize, count >> 32)
}

fn fill_destroyed((at, round): (usize, u64), entity: Entity) {
    if let Some(slot) = DESTROYED.get(at) {
        slot.store(round << 32 | u64::from(entity.0), Ordering::Release);
    }
}

/// Whether a destroy request waits to be taken.
fn any_destroyed() -> bool {
    DESTROYED_COUNT.load(Ordering::Acquire) & 0xffff_ffff != 0
}

/// The entities noted since the last call. A request filled after the call
/// that counted it is dropped, never taken in a later round, as its entity
/// id may be another's by then.
fn take_destroyed() -> FxHashSet<u32> {
    let count = DESTROYED_COUNT
        .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            Some(((count >> 32) + 1) << 32)
        })
        .unwrap_or_else(|count| count);
    let (taken, round) = (
        ((count & 0xffff_ffff) as usize).min(DESTROYED_CAP),
        count >> 32,
    );
    DESTROYED[..taken]
        .iter()
        .map(|slot| slot.swap(0, Ordering::AcqRel))
        .filter(|&noted| noted >> 32 == round)
        .map(|noted| (noted & 0xffff_ffff) as u32)
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

/// After the server's step: after a load's first, finds the file's records'
/// entities; every [`JOIN_LOOK_EVERY`] steps, looks for joined characters'
/// carried items; after every step, drops the records of the entities
/// destroyed in it, and tells the clients.
pub(crate) fn after_step(loader: &Loader) {
    if FINDING.load(Ordering::Relaxed) {
        find_pending(loader);
    }
    if crate::events::steps().is_multiple_of(JOIN_LOOK_EVERY) {
        find_carried(loader);
    }
    if !KEPT.load(Ordering::Relaxed) || !any_destroyed() {
        return;
    }
    let destroyed = take_destroyed();
    let gone = {
        let mut server = SERVER.relock();
        let Some(server) = server.as_mut() else {
            return;
        };
        let gone = prune_destroyed(&mut server.values, &destroyed);
        server.changed |= !gone.is_empty();
        gone
    };
    let mut messages = Vec::new();
    for (kind, pide) in gone {
        if let Some(spec) = self::kind(kind).filter(|k| k.sent) {
            push_encoded(&mut messages, &spec.id, pide, 0, &[]);
        }
    }
    send(&messages);
}

/// Appends a record's encoding to the last of `messages`, moving it to a
/// new message when it would grow the last past [`MESSAGE_BYTES`].
fn push_encoded(messages: &mut Vec<Vec<u8>>, id: &str, pide: u32, item: u32, value: &[u8]) {
    if messages.is_empty() {
        messages.push(Vec::new());
    }
    let last = messages.len() - 1;
    let start = messages[last].len();
    encode(&mut messages[last], id, pide, item, value);
    if start > 0 && messages[last].len() > MESSAGE_BYTES {
        let record = messages[last].split_off(start);
        messages.push(record);
    }
}

/// The messages that tell clients the values of `keys` in `values`, for the
/// kinds clients get.
fn messages_for(values: &Values, keys: &[(u32, u32)]) -> Vec<Vec<u8>> {
    let mut messages = Vec::new();
    for &(kind, pide) in keys {
        if let (Some(spec), Some(record)) = (
            self::kind(kind).filter(|k| k.sent),
            values.get(&(kind, pide)),
        ) {
            push_encoded(&mut messages, &spec.id, pide, record.item, &record.value);
        }
    }
    messages
}

/// Sends `messages` to every client.
fn send(messages: &[Vec<u8>]) {
    for message in messages {
        crate::net::item_data(0, message);
    }
}

/// Puts each `found` record into `values` under its kind and entity, as
/// attached from now on; the keys.
fn insert_found(values: &mut Values, found: Vec<(u32, u32, Pending)>) -> Vec<(u32, u32)> {
    found
        .into_iter()
        .map(|(kind, entity, record)| {
            values.insert(
                (kind, entity),
                Record {
                    item: record.check,
                    anchor: Some(record.anchor),
                    value: record.value,
                },
            );
            (kind, entity)
        })
        .collect()
}

/// Whether any mod keeps item data, which needs the world events and the
/// clients' hello.
pub(crate) fn wanted() -> bool {
    KINDS.sealed().is_some_and(|k| !k.is_empty())
}

fn kind(kind: u32) -> Option<&'static Kind> {
    KINDS.sealed()?.get(usize::try_from(kind).ok()?)
}

fn kind_of(id: &str) -> Option<u32> {
    let index = KINDS.sealed()?.iter().position(|k| k.id == id)?;
    u32::try_from(index).ok()
}

/// The game id of `entity`'s template, on the server's game thread; `None`
/// for an entity that is gone or off that thread.
fn template_id(entity: Entity) -> Option<u32> {
    let loader = crate::host::loader()?;
    let template = crate::world::template(loader, entity).ok()?;
    Some(enshwick::content::Guid(template).fnv1a())
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
        let mut server = SERVER.relock();
        let server = server.as_mut().ok_or(Status::MISSING)?;
        let record = server.values.get(&(kind, pide)).ok_or(Status::MISSING)?;
        let wanted = match scope {
            Scope::Item => item,
            // An entity in an area that is not loaded is not in the world's
            // map either: its value stays, unchecked.
            Scope::Entity => match template_id(Entity(pide)) {
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
    let client = CLIENT.relock();
    client
        .as_ref()
        .and_then(|v| v.get(&(kind, pide)).cloned())
        .filter(|r| scope == Scope::Entity || r.item == item)
        .map(|r| r.value)
        .ok_or(Status::MISSING)
}

/// Sets, or for an empty value removes, kind `kind`'s value on the item
/// `pide` holding `item`, and sends the change to the clients. On the
/// server only, once its world has loaded. An entity kind's value needs an
/// entity the next load finds again: [`Status::NOT_KEPT`] otherwise.
pub(crate) fn set(kind: u32, pide: u32, item: u32, value: &[u8]) -> Result<(), Status> {
    let spec = self::kind(kind).ok_or(Status::BAD_ARGS)?;
    let id = spec.id.as_str();
    if pide == 0 || value.len() > MAX_VALUE {
        return Err(Status::BAD_ARGS);
    }
    if !serves() {
        return Err(Status::REFUSED);
    }
    if !value.is_empty() && UNREAD.load(Ordering::Relaxed) {
        return Err(Status::NOT_KEPT);
    }
    let loader = crate::host::loader();
    let (item, anchor) = match spec.scope {
        Scope::Item if item == 0 => return Err(Status::BAD_ARGS),
        // No layout, no anchor: the value would be aside at the next load.
        Scope::Item if !value.is_empty() && loader.is_some_and(|l| !game::readable(l)) => {
            return Err(Status::NOT_KEPT);
        }
        Scope::Item => (
            item,
            loader
                .and_then(|loader| {
                    game::held(loader, pide, |root| {
                        crate::world::character_of(loader, root)
                    })
                })
                .and_then(|held| held.anchor),
        ),
        Scope::Entity => {
            crate::world::server().map_err(|_| Status::WRONG_THREAD)?;
            if value.is_empty() {
                (0, None)
            } else {
                let template = template_id(Entity(pide)).ok_or(Status::BAD_ARGS)?;
                let placed = loader
                    .and_then(|loader| game::placed(loader, pide))
                    .ok_or(Status::NOT_KEPT)?;
                (template, Some(Anchor::Placed(placed)))
            }
        }
    };
    {
        let mut server = SERVER.relock();
        let server = server.as_mut().ok_or(Status::MISSING)?;
        if value.is_empty() {
            server.values.remove(&(kind, pide));
        } else {
            let record = Record {
                item,
                anchor,
                value: value.into(),
            };
            server.values.insert((kind, pide), record);
            KEPT.store(true, Ordering::Relaxed);
        }
        server.changed = true;
    }
    if id == enshwick::stats::ITEM_KIND {
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

/// The messages that give a client every value, each at most
/// [`MESSAGE_BYTES`] long.
fn snapshot() -> Vec<Vec<u8>> {
    let server = SERVER.relock();
    let mut messages = Vec::new();
    for (&(kind, pide), record) in server.iter().flat_map(|s| &s.values) {
        if let Some(spec) = self::kind(kind).filter(|k| k.sent) {
            push_encoded(&mut messages, &spec.id, pide, record.item, &record.value);
        }
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
    let mut client = CLIENT.relock();
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
                anchor: None,
                value: value.into(),
            },
        );
    }
    kept_all
}

/// Forgets what the client got, when it changes servers.
pub(crate) fn forget_client() {
    *CLIENT.relock() = None;
    CLIENT_FULL.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests;
