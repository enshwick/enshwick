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

mod anchor;
mod file;
mod game;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use enshwick::Entity;
use enshwick::abi::{Event, HookFlags, Status};
use rustc_hash::{FxHashMap, FxHashSet};

use self::anchor::{Anchor, Index, Pending};
use self::file::Saved;
use crate::chains::Next;
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[pinned::ecs::DESTROY_REQUEST.id()];

/// The longest kind id: a mod id, a colon and a name of up to 64 each.
const MAX_ID: usize = 129;
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
/// kind, else the one its anchor tells.
fn scope(kind: u32, anchor: Option<Anchor>) -> Option<Scope> {
    match (self::kind(kind), anchor) {
        (Some(spec), _) => Some(spec.scope),
        (None, Some(Anchor::Placed(_))) => Some(Scope::Entity),
        (None, Some(Anchor::Held { .. })) => Some(Scope::Item),
        (None, _) => None,
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
/// namespace `mod_id` (its first former id after a rename): `name` names the
/// mod's own kind, `mod:name` any mod's.
fn full_id(mod_id: &str, id: &str) -> Option<String> {
    let (namespace, name) = id.split_once(':').unwrap_or((mod_id, id));
    (enshwick::abi::is_valid_former_id(namespace) && is_part(name))
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
    let full = full_id(record.home(), id).ok_or(Status::BAD_ARGS)?;
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
        loader.address(pinned::ecs::DESTROY_REQUEST),
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
/// entities; after every step, drops the records of the entities destroyed
/// in it, and tells the clients.
pub(crate) fn after_step(loader: &Loader) {
    if FINDING.load(Ordering::Relaxed) {
        find_pending(loader);
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

/// The placed things whose templates the `pending` records name, by key;
/// `None` when any part of the walk does not read, since a thing missing
/// from the index would set its records aside for good.
fn index(loader: &Loader, pending: &[Pending]) -> Option<Index> {
    let mut index = Index::default();
    let wanted: FxHashSet<[u8; 16]> = pending
        .iter()
        .filter(|record| !record.aside)
        .filter_map(|record| match record.anchor {
            Anchor::Placed(placed) | Anchor::Held { root: placed, .. } => Some(placed.template()),
            Anchor::Session(_) => None,
        })
        .collect();
    if wanted.is_empty() {
        return Some(index);
    }
    let ty = game::transform_type(loader)?;
    let places = crate::entities::with_component(loader, ty).ok()?;
    let mut unread = 0usize;
    for place in places {
        let entity = place.entity.0;
        let Ok(template) = crate::world::template(loader, place.entity) else {
            unread += 1;
            continue;
        };
        if wanted.contains(&template) {
            let key = game::placed(loader, entity);
            unread += usize::from(key.is_none());
            if let Some(key) = key {
                index.add(key, entity);
            }
        }
    }
    if unread > 0 {
        crate::log!(
            WARN,
            "mod data: {unread} placed things did not read; the values wait as read for the next load"
        );
        return None;
    }
    Some(index)
}

/// What [`attach`] did, beside the values it attached.
#[derive(Debug, Default, PartialEq, Eq)]
struct Attached {
    /// The kinds and entities that took a value.
    found: Vec<(u32, u32)>,
    /// Records newly aside: on shared places, on empty ones, replaced.
    ambiguous: usize,
    missing: usize,
    replaced: usize,
    /// Records aside from before.
    earlier: usize,
}

/// Attaches `server`'s pending records to the entities `index` names, as
/// the first step after a load does. With no index, because the world did
/// not read, every record stays exactly as read and nothing is marked
/// changed: no record goes aside for a read that failed.
fn attach(
    server: &mut Server,
    index: Option<&Index>,
    held: impl Fn(u32, u8, u8) -> Option<(u32, u32)>,
) -> Option<Attached> {
    let index = index?;
    let taken: FxHashSet<(u32, u32)> = server.values.keys().copied().collect();
    let outcome = anchor::resolve(
        std::mem::take(&mut server.pending),
        index,
        held,
        |kind, entity| taken.contains(&(kind, entity)),
    );
    let mut attached = Attached {
        found: Vec::with_capacity(outcome.found.len()),
        ambiguous: outcome.ambiguous,
        missing: outcome.missing,
        replaced: outcome.replaced,
        earlier: outcome.earlier,
    };
    for (kind, entity, record) in outcome.found {
        attached.found.push((kind, entity));
        server.values.insert(
            (kind, entity),
            Record {
                item: record.check,
                anchor: Some(record.anchor),
                value: record.value,
            },
        );
    }
    server.pending = outcome.left;
    // Records newly aside are written so, whatever else changes.
    server.changed |=
        attached.ambiguous + attached.missing + attached.replaced + attached.earlier > 0;
    Some(attached)
}

/// Finds the entities of the records a load read, once, on the game thread
/// after the world's first step; records it cannot name for certain go
/// aside, and all wait as read when the world does not read.
#[cold]
fn find_pending(loader: &Loader) {
    if crate::world::server().is_err() {
        return;
    }
    FINDING.store(false, Ordering::Relaxed);
    let pending = {
        let server = SERVER.relock();
        let Some(server) = server.as_ref() else {
            return;
        };
        if server.pending.is_empty() {
            return;
        }
        server.pending.clone()
    };
    let started = std::time::Instant::now();
    let index = index(loader, &pending);
    let mut message = Vec::new();
    let attached = {
        let mut server = SERVER.relock();
        let Some(server) = server.as_mut() else {
            return;
        };
        let attached = attach(server, index.as_ref(), |root, link, slot| {
            game::in_slot(loader, root, link, slot)
        });
        for &(kind, entity) in attached.iter().flat_map(|a| &a.found) {
            let (Some(spec), Some(record)) = (
                self::kind(kind).filter(|k| k.sent),
                server.values.get(&(kind, entity)),
            ) else {
                continue;
            };
            encode(&mut message, &spec.id, entity, record.item, &record.value);
            if message.len() > MESSAGE_BYTES {
                crate::net::item_data(0, &std::mem::take(&mut message));
            }
        }
        KEPT.store(!server.values.is_empty(), Ordering::Relaxed);
        attached
    };
    if !message.is_empty() {
        crate::net::item_data(0, &message);
    }
    let Some(attached) = attached else {
        crate::log!(
            WARN,
            "mod data: the world's placed things did not read on this game build; {} values wait as read and no mod data attaches this session",
            pending.len()
        );
        return;
    };
    crate::log!(
        INFO,
        "mod data: {} values found their entities again after the load in {} ms; {} stay aside from before",
        attached.found.len(),
        started.elapsed().as_millis(),
        attached.earlier
    );
    let aside = attached.ambiguous + attached.missing + attached.replaced;
    if aside > 0 {
        crate::log!(
            WARN,
            "mod data: {aside} values set aside in the world's file, attached to no entity and never again: {} at a place more than one entity shares, {} at a place no entity holds, {} replaced before the world's first step",
            attached.ambiguous,
            attached.missing,
            attached.replaced
        );
    }
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
                .and_then(|loader| game::held(loader, pide))
                .and_then(|(anchor, _)| anchor),
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
    let server = SERVER.relock();
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

/// The file of world `key`'s values.
fn file(loader: &Loader, key: &str) -> PathBuf {
    loader
        .data_dir
        .join("enshwick")
        .join("worlds")
        .join(key)
        .join(FILE)
}

/// The records a world's file holds, each waiting for its entity, and the
/// kinds among them no mod opened, which number on from the opened ones;
/// `None` for a file the loader does not read, such as one of a newer
/// loader or a damaged one.
fn read_file(bytes: &[u8]) -> Option<(Vec<Pending>, Vec<String>)> {
    let opened = KINDS.sealed().map_or(0, <[Kind]>::len);
    let mut unopened: Vec<String> = Vec::new();
    let mut pending = Vec::new();
    for saved in file::read(bytes)? {
        let kind = if let Some(kind) = kind_of(saved.id) {
            kind
        } else {
            let at = unopened
                .iter()
                .position(|id| id == saved.id)
                .unwrap_or_else(|| {
                    unopened.push(saved.id.to_owned());
                    unopened.len() - 1
                });
            u32::try_from(opened + at).ok()?
        };
        pending.push(Pending {
            kind,
            anchor: saved.anchor,
            check: saved.check,
            value: saved.value.into(),
            aside: saved.aside,
        });
    }
    Some((pending, unopened))
}

/// The file's bytes: `server`'s values under their anchors, a value seen
/// with none under its entity id, which sets it aside at the next load;
/// then the records waiting or aside.
fn write_file(server: &Server) -> Vec<u8> {
    let mut sorted: Vec<_> = server.values.iter().collect();
    sorted.sort_by_key(|(key, _)| **key);
    let values = sorted.into_iter().filter_map(|(&(kind, entity), record)| {
        Some(Saved {
            id: server.kind_id(kind)?,
            anchor: record.anchor.unwrap_or(Anchor::Session(entity)),
            check: record.item,
            value: &record.value,
            aside: false,
        })
    });
    let pending = server.pending.iter().filter_map(|record| {
        Some(Saved {
            id: server.kind_id(record.kind)?,
            anchor: record.anchor,
            check: record.check,
            value: &record.value,
            aside: record.aside,
        })
    });
    file::write(values.chain(pending))
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
            let path = key.as_deref().map(|key| file(loader, key));
            let read = path.as_ref().and_then(|path| std::fs::read(path).ok());
            let (pending, unopened) = match read.as_deref().map(read_file) {
                Some(Some(records)) => records,
                Some(None) => {
                    // Keep a file this loader cannot read from being
                    // overwritten: the world runs without its values.
                    crate::log!(
                        WARN,
                        "mod data: {} does not read to its end, or is of a version this loader does not know; it stays as it is and this world keeps no mod data",
                        path.as_ref()
                            .map_or_else(String::new, |p| p.display().to_string())
                    );
                    *SERVER.relock() = None;
                    return;
                }
                None => Default::default(),
            };
            crate::log!(
                INFO,
                "mod data: {} values for world {}",
                pending.len(),
                key.as_deref().unwrap_or("?")
            );
            KEPT.store(false, Ordering::Relaxed);
            FINDING.store(!pending.is_empty(), Ordering::Relaxed);
            let _earlier_world = take_destroyed();
            *SERVER.relock() = Some(Server {
                key,
                values: Values::default(),
                pending,
                unopened,
                changed: false,
            });
        }
        Event::WORLD_SAVED | Event::SHUTDOWN_STARTED => save(loader, event == Event::WORLD_SAVED),
        _ => {}
    }
}

/// After a save of the game's, reads each value's anchor afresh from its
/// entity. A value whose entity holds another item or template now is
/// dropped. One whose entity the world no longer holds goes aside: every
/// saved entity exists from the load on (server 399), so it is gone, and a
/// later entity may take its place. Nothing changes where the world does
/// not read. The number of values with no anchor.
fn refresh(loader: &Loader, server: &mut Server) -> usize {
    if !game::readable(loader) {
        return 0;
    }
    // The placed things the world holds; `None` when the walk does not read,
    // and then no entity value counts as gone.
    let placed: Option<FxHashSet<u32>> = game::transform_type(loader)
        .and_then(|ty| crate::entities::with_component(loader, ty).ok())
        .map(|places| places.iter().map(|p| p.entity.0).collect());
    let before = server.values.len();
    let mut anchorless = 0;
    let mut gone = Vec::new();
    server.values.retain(|&(kind, entity), record| {
        // `Some(None)`: the world no longer holds the entity.
        let now = match scope(kind, record.anchor) {
            Some(Scope::Item) => {
                Some(game::held(loader, entity).map(|(anchor, item)| (item, anchor)))
            }
            Some(Scope::Entity) => match &placed {
                Some(placed) if !placed.contains(&entity) => Some(None),
                Some(_) => template_id(Entity(entity)).map(|template| {
                    Some((template, game::placed(loader, entity).map(Anchor::Placed)))
                }),
                None => None,
            },
            None => None,
        };
        match now {
            None => true,
            Some(None) => {
                gone.push(Pending {
                    kind,
                    anchor: record.anchor.unwrap_or(Anchor::Session(entity)),
                    check: record.item,
                    value: core::mem::take(&mut record.value),
                    aside: true,
                });
                false
            }
            Some(Some((held, _))) if held != record.item => false,
            Some(Some((_, anchor))) => {
                // A live entity in no place a load finds, such as an item in
                // a player's inventory, loses its anchor: the old one names a
                // slot another item may fill.
                if anchor != record.anchor {
                    record.anchor = anchor;
                    server.changed = true;
                }
                anchorless += usize::from(record.anchor.is_none());
                true
            }
        }
    });
    if !gone.is_empty() {
        crate::log!(
            WARN,
            "mod data: {} values set aside: the world no longer holds their entities",
            gone.len()
        );
    }
    server.pending.append(&mut gone);
    server.changed |= server.values.len() != before;
    anchorless
}

/// Writes the values when they changed, under their anchors as the world
/// holds them now.
fn save(loader: &Loader, after_game_save: bool) {
    let (key, bytes, anchorless) = {
        let mut server = SERVER.relock();
        let Some(server) = server.as_mut() else {
            return;
        };
        // Before the first step the records wait as read, and the values set
        // since carry the anchors they were set with. At shutdown the world
        // is being torn down: the anchors of its last save stand.
        let anchorless = if FINDING.load(Ordering::Relaxed) || !after_game_save {
            0
        } else {
            refresh(loader, server)
        };
        if !server.changed {
            return;
        }
        let Some(key) = server.key.clone() else {
            return;
        };
        server.changed = false;
        (key, write_file(server), anchorless)
    };
    if anchorless > 0 {
        crate::log!(
            WARN,
            "mod data: {anchorless} item values sit where a load cannot find them, such as a player's inventory; after a restart they stay aside unless the item is in a placed container when the world saves"
        );
    }
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
            full_id("you.lanterns", "oil").as_deref(),
            Some("you.lanterns:oil")
        );
        assert_eq!(
            full_id("you.lanterns", "other:fire_damage").as_deref(),
            Some("other:fire_damage")
        );
        assert_eq!(full_id("you.lanterns", "Charges"), None);
        assert_eq!(full_id("you.lanterns", "a:b:c"), None);
        assert_eq!(full_id("you.lanterns", ":x"), None);
    }

    #[test]
    fn a_destroyed_entity_loses_its_records_and_an_unloaded_one_keeps_them() {
        let record = |item| Record {
            item,
            anchor: None,
            value: Box::new([1]),
        };
        let mut values = Values::default();
        values.insert((0, 900), record(7));
        values.insert((1, 900), record(0));
        values.insert((0, 901), record(7));
        note_destroyed(Entity(900));
        note_destroyed(Entity(0));
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
            note_destroyed(Entity(entity));
        }
        let destroyed = take_destroyed();
        assert_eq!(destroyed.len(), DESTROYED_CAP);
        assert!(!destroyed.contains(&(u32::try_from(DESTROYED_CAP).unwrap() + 1)));
        // A request filled after its round was taken never reaches a later
        // one, where its slot is still being filled.
        note_destroyed(Entity(1));
        let late = reserve_destroyed();
        assert_eq!(take_destroyed().into_iter().collect::<Vec<_>>(), [1]);
        fill_destroyed(late, Entity(5));
        note_destroyed(Entity(2));
        let _filling = reserve_destroyed();
        assert_eq!(take_destroyed().into_iter().collect::<Vec<_>>(), [2]);
    }

    /// The kinds these tests share, sealed once per process: `a:charges`
    /// (0), an item kind clients get, and `a:owner` (1), an entity kind.
    fn kinds() {
        let _charges = KINDS.add(Kind {
            id: "a:charges".into(),
            scope: Scope::Item,
            sent: true,
        });
        let _owner = KINDS.add(Kind {
            id: "a:owner".into(),
            scope: Scope::Entity,
            sent: false,
        });
        let _sealed = KINDS.seal();
    }

    #[test]
    fn a_client_keeps_at_most_its_cap_of_a_server_s_records() {
        kinds();
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

    fn server(pending: Vec<Pending>, unopened: Vec<String>) -> Server {
        Server {
            key: Some("w".into()),
            values: Values::default(),
            pending,
            unopened,
            changed: false,
        }
    }

    fn saved<'a>(id: &'a str, anchor: Anchor, value: &'static [u8]) -> Saved<'a> {
        Saved {
            id,
            anchor,
            check: 9,
            value,
            aside: false,
        }
    }

    fn chest(x: u8) -> anchor::Placed {
        anchor::Placed::new([1; 16], [i64::from(x) << 32; 3])
    }

    /// Attaches `server`'s pending records against `index`, as the first
    /// step after a load does.
    fn first_step(server: &mut Server, index: &Index) {
        let _attached = attach(server, Some(index), |_, _, _| None);
    }

    #[test]
    fn a_world_that_does_not_read_leaves_every_record_as_read() {
        kinds();
        let bytes = file::write([
            saved("a:owner", Anchor::Placed(chest(1)), &[5]),
            saved("gone:owner", Anchor::Placed(chest(2)), &[6]),
        ]);
        let (pending, unopened) = read_file(&bytes).unwrap();
        let mut world = server(pending.clone(), unopened);
        assert_eq!(attach(&mut world, None, |_, _, _| None), None);
        assert_eq!(world.pending, pending, "nothing went aside");
        assert!(world.values.is_empty());
        assert!(!world.changed, "nothing to write");
        assert_eq!(write_file(&world), bytes, "and if written, the same bytes");
    }

    #[test]
    fn records_of_kinds_no_mod_opened_follow_their_entities_until_the_mod_returns() {
        kinds();
        let bytes = file::write([
            saved("gone:owner", Anchor::Placed(chest(1)), &[3]),
            saved("gone:owner", Anchor::Placed(chest(2)), &[4]),
            saved("gone:empty", Anchor::Session(901), &[]),
        ]);
        let (pending, unopened) = read_file(&bytes).unwrap();
        assert_eq!(unopened, ["gone:owner"]);
        let mut world = server(pending, unopened);
        // Chest 2 is gone; chest 1 stands, renumbered.
        let mut index = Index::default();
        index.add(chest(1), 1050);
        first_step(&mut world, &index);
        assert_eq!(world.values.len(), 1);
        assert_eq!(
            world.kind_id(world.values.keys().next().unwrap().0),
            Some("gone:owner")
        );
        let written = write_file(&world);
        let records = file::read(&written).unwrap();
        assert_eq!(
            records,
            [
                saved("gone:owner", Anchor::Placed(chest(1)), &[3]),
                Saved {
                    aside: true,
                    ..saved("gone:owner", Anchor::Placed(chest(2)), &[4])
                },
            ],
            "a new chest built where chest 2 stood never takes its value"
        );
    }

    #[test]
    fn a_value_set_before_the_first_step_is_found_after_the_next_load() {
        kinds();
        let bytes = file::write([saved("a:owner", Anchor::Placed(chest(1)), &[5])]);
        let (pending, unopened) = read_file(&bytes).unwrap();
        let mut world = server(pending, unopened);
        // A mod sets a value on the chest before the world's first step.
        world.values.insert(
            (1, 1027),
            Record {
                item: 9,
                anchor: Some(Anchor::Placed(chest(1))),
                value: Box::new([6]),
            },
        );
        let mut index = Index::default();
        index.add(chest(1), 1027);
        first_step(&mut world, &index);
        assert_eq!(&*world.values[&(1, 1027)].value, &[6]);
        // Restart: the chest is 1031 now.
        let (pending, unopened) = read_file(&write_file(&world)).unwrap();
        let mut world = server(pending, unopened);
        let mut index = Index::default();
        index.add(chest(1), 1031);
        first_step(&mut world, &index);
        assert_eq!(&*world.values[&(1, 1031)].value, &[6], "the live value");
        assert_eq!(world.pending.len(), 1, "the replaced one stays aside");
        assert!(world.pending[0].aside);
    }

    #[test]
    fn a_save_keeps_values_by_anchor_and_records_aside_as_they_are() {
        kinds();
        // A file of the version that kept entity ids: its record is aside
        // under the old id, never attached.
        let mut old = file::MAGIC.to_vec();
        old.extend_from_slice(&[1, 7]);
        old.extend_from_slice(b"a:owner");
        old.extend_from_slice(&1043u32.to_le_bytes());
        old.extend_from_slice(&9u32.to_le_bytes());
        old.extend_from_slice(&[1, 0, 5]);
        let (pending, unopened) = read_file(&old).unwrap();
        assert_eq!(pending[0].anchor, Anchor::Session(1043));
        assert!(pending[0].aside);
        let mut kept = server(pending, unopened);
        kept.values.insert(
            (1, 1027),
            Record {
                item: 9,
                anchor: Some(Anchor::Placed(chest(1))),
                value: Box::new([6]),
            },
        );
        kept.values.insert(
            (1, 1030),
            Record {
                item: 9,
                anchor: None,
                value: Box::new([8]),
            },
        );
        let written = write_file(&kept);
        let records = file::read(&written).unwrap();
        let anchors: Vec<(Anchor, &[u8], bool)> = records
            .iter()
            .map(|r| (r.anchor, r.value, r.aside))
            .collect();
        assert_eq!(
            anchors,
            [
                (Anchor::Placed(chest(1)), &[6u8][..], false),
                (Anchor::Session(1030), &[8][..], false),
                (Anchor::Session(1043), &[5][..], true),
            ]
        );
        assert_eq!(read_file(b"EWID\x09"), None, "a newer file is not read");
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
