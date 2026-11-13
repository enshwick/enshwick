//! Game events for mods: every ECS event the server's systems write goes
//! into an event stream, and the engine moves the streams into its event
//! queues at the start of each step, on the game thread, before any system
//! runs. The loader links that move, copies the events a mod subscribed to
//! and hands them over right after it, with the world still quiescent and
//! the entities they name still in it.
//!
//! The same pass lets the craft filters screen the craft requests in the
//! streams before the engine moves them (see the vetoes module).
//!
//! A client runs the same move for its own world as well as for a hosted
//! game's server. Explicit client subscriptions follow its own world in a
//! separate observation batch, without running server filters or actions.

use core::cell::RefCell;
use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_core::abi::{GameEventData, GameEventFn, Header, HookFlags, Side, Status};
use rustc_hash::FxHashSet;

use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::pinned::Constant;
use enshwick_base::Relock;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::EVENT_MERGE.id(),
    pinned::ecs::EVENT_RECORD_ALIGN.id(),
    pinned::ecs::EVENT_RECORD_HEADER.id(),
    pinned::ecs::EVENT_RECORD_SIZE.id(),
    pinned::ecs::EVENT_STREAM_COUNT.id(),
    pinned::ecs::EVENT_STREAM_SIZE.id(),
    pinned::ecs::EVENT_STREAM_USED.id(),
    pinned::ecs::EVENT_STREAMS.id(),
];

/// Client observations never share a batch or callbacks with server events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Server,
    Client,
}

impl Scope {
    fn matches(self, delivered: Self, wanted: u32, actual: u32) -> bool {
        self == delivered && (wanted == actual || self == Self::Server && wanted == 0)
    }
}

fn scope_for(side: Side, server: bool, own_client: bool) -> Option<Scope> {
    if server {
        Some(Scope::Server)
    } else if side == Side::CLIENT && own_client {
        Some(Scope::Client)
    } else {
        None
    }
}

const CLIENT_EVENTS: usize = 256;
const CLIENT_BYTES: usize = 64 * 1024;
const CLIENT_SUBSCRIPTIONS: usize = 32;
/// Bound traversal even when no subscribed event occurs in a client merge.
const CLIENT_RECORDS: usize = 65_536;
const CLIENT_STREAMS: usize = 256;
static CLIENT_DROPPED: AtomicBool = AtomicBool::new(false);

fn client_registration(side: Side, type_hash: u32, count: usize) -> Status {
    if side != Side::CLIENT {
        Status::UNAVAILABLE
    } else if type_hash == 0 || count >= CLIENT_SUBSCRIPTIONS {
        Status::BAD_ARGS
    } else {
        Status::OK
    }
}

fn client_drop() {
    if !CLIENT_DROPPED.swap(true, Ordering::Relaxed) {
        crate::log!(
            WARN,
            "client game events: copy or scan cap reached; observations dropped (256 events, 64 KiB, 65536 records, 256 streams per merge)"
        );
    }
}

/// A mod's handler for one event type, or for all with hash 0.
#[derive(Debug, Clone, Copy)]
struct Subscriber {
    scope: Scope,
    owner: ModIndex,
    type_hash: u32,
    callback: GameEventFn,
    user: usize,
}

/// The subscriptions once every mod is initialised.
#[derive(Debug)]
struct Sealed {
    subscribers: Box<[Subscriber]>,
    wanted: FxHashSet<u32>,
    client_wanted: FxHashSet<u32>,
    every: bool,
    /// The equipment event, when stats follow it.
    equipment: Option<u32>,
}

impl Sealed {
    fn wants(&self, scope: Scope, type_hash: u32) -> bool {
        match scope {
            Scope::Server => {
                self.every || self.wanted.contains(&type_hash) || self.equipment == Some(type_hash)
            }
            Scope::Client => self.client_wanted.contains(&type_hash),
        }
    }
}

/// Where the engine keeps its event streams, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    streams: usize,
    stream_count: usize,
    stream_size: usize,
    stream_used: usize,
    record_size: usize,
    record_header: usize,
    record_align: usize,
}

/// One copied event: its type, size and where its bytes start in the buffer.
#[derive(Debug, Clone, Copy)]
struct Copied {
    type_hash: u32,
    size: u32,
    start: usize,
}

/// Events copied during one move, on the game thread only.
#[derive(Debug, Default)]
struct Batch {
    events: Vec<Copied>,
    bytes: Vec<u8>,
}

impl Batch {
    /// Appends `other`'s events after this batch's own.
    fn append(&mut self, other: &Self) {
        let offset = self.bytes.len();
        self.events.extend(other.events.iter().map(|e| Copied {
            start: e.start + offset,
            ..*e
        }));
        self.bytes.extend_from_slice(&other.bytes);
    }
}

/// Takes this thread's batch out of its cell, so the mod callbacks that run
/// while it is filled or handed out may reach the loader again.
fn take_batch(scope: Scope) -> Batch {
    match scope {
        Scope::Server => BATCH.take(),
        Scope::Client => CLIENT_BATCH.take(),
    }
}

/// Puts `batch` back into this thread's cell, after the events a callback
/// copied meanwhile, keeping its buffers when there were none.
fn give_back(scope: Scope, mut batch: Batch) {
    let put = |held: &mut Batch| {
        if held.events.is_empty() && held.bytes.is_empty() {
            core::mem::swap(held, &mut batch);
        } else {
            let newer = core::mem::take(held);
            if scope == Scope::Client
                && (batch.events.len() + newer.events.len() > CLIENT_EVENTS
                    || batch.bytes.len() + newer.bytes.len() > CLIENT_BYTES)
            {
                client_drop();
            } else {
                batch.append(&newer);
            }
            *held = batch;
        }
    };
    match scope {
        Scope::Server => BATCH.with_borrow_mut(put),
        Scope::Client => CLIENT_BATCH.with_borrow_mut(put),
    }
}

static PENDING: Mutex<Vec<Subscriber>> = Mutex::new(Vec::new());
static SEALED: OnceLock<Sealed> = OnceLock::new();
/// The loader and the build's stream layout, when available.
static LOADED: OnceLock<(&'static Loader, Option<Layout>)> = OnceLock::new();
/// The event move links independently from each consumer's delivery fuse.
static EVENTS_FUSE: Fuse = Fuse::new("game events");
/// Copies and delivers server events to subscribed mods.
static DELIVERY_FUSE: Fuse = Fuse::new("game event delivery");
static CLIENT_FUSE: Fuse = Fuse::new("client game events");
/// The client's notices from its server, off once raising one panicked.
static NOTICES_FUSE: Fuse = Fuse::new("server notices");
/// Frees lists handed to the engine.
static RETIRE_FUSE: Fuse = Fuse::new("freeing handed lists");

thread_local! {
    static BATCH: RefCell<Batch> = RefCell::new(Batch::default());
    static CLIENT_BATCH: RefCell<Batch> = RefCell::new(Batch::default());
}

/// Registers mod `owner`'s handler for events of `type_hash`, 0 for all.
pub(crate) fn register(
    owner: ModIndex,
    type_hash: u32,
    callback: GameEventFn,
    user: *mut c_void,
) -> Status {
    if SEALED.get().is_some() {
        return Status::TOO_LATE;
    }
    PENDING.relock().push(Subscriber {
        scope: Scope::Server,
        owner,
        type_hash,
        callback,
        user: user.expose_provenance(),
    });
    Status::OK
}

/// A separate, explicitly selected client-world observation subscription.
pub(crate) fn register_client(
    side: Side,
    owner: ModIndex,
    type_hash: u32,
    callback: GameEventFn,
    user: *mut c_void,
) -> Status {
    if SEALED.get().is_some() {
        return Status::TOO_LATE;
    }
    let mut pending = PENDING.relock();
    let count = pending
        .iter()
        .filter(|s| s.owner == owner && s.scope == Scope::Client)
        .count();
    let status = client_registration(side, type_hash, count);
    if !status.is_ok() {
        return status;
    }
    pending.push(Subscriber {
        scope: Scope::Client,
        owner,
        type_hash,
        callback,
        user: user.expose_provenance(),
    });
    Status::OK
}

/// Seals the subscriptions and links the event move when a mod subscribed.
pub(crate) fn link(loader: &'static Loader) {
    let subscribers = std::mem::take(&mut *PENDING.relock());
    let equipment = crate::inventory::stats::wanted()
        .then(|| {
            crate::engine::catalogue::component_hash(
                loader,
                crate::engine::catalogue::ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT,
            )
        })
        .flatten();
    let sealed = Sealed {
        wanted: subscribers
            .iter()
            .filter(|s| s.scope == Scope::Server)
            .map(|s| s.type_hash)
            .collect(),
        client_wanted: subscribers
            .iter()
            .filter(|s| s.scope == Scope::Client)
            .map(|s| s.type_hash)
            .collect(),
        every: subscribers
            .iter()
            .any(|s| s.scope == Scope::Server && s.type_hash == 0),
        subscribers: subscribers.into_boxed_slice(),
        equipment,
    };
    let count = sealed.subscribers.len() + usize::from(equipment.is_some());
    // A client always links the move, to raise the loader's notices in its
    // own world.
    if SEALED.set(sealed).is_err()
        || (count == 0
            && !crate::actions::vetoes::crafting()
            && !crate::inventory::item_actions::wanted()
            && loader.side != Side::CLIENT)
    {
        return;
    }
    // Without the layout the move still links: a client's notices and the
    // freeing of handed lists need no stream.
    let _first = LOADED.set((loader, stream_layout(loader)));
    let target = Target::Pinned(pinned::ecs::EVENT_MERGE);
    if loader.link_fused(&EVENTS_FUSE, target, &MERGE_NEXT, merge, HookFlags::NONE) {
        crate::log!(INFO, "game events: {count} subscriptions");
    }
}

/// The build's event stream layout; without it the delivery is off, listed
/// by the first entry the build lacks.
fn stream_layout(loader: &Loader) -> Option<Layout> {
    let constant = |name: Constant| {
        loader
            .build
            .constant(name.id())
            .and_then(|v| usize::try_from(v).ok())
            .ok_or(name)
    };
    let layout = (|| {
        Ok(Layout {
            streams: constant(pinned::ecs::EVENT_STREAMS)?,
            stream_count: constant(pinned::ecs::EVENT_STREAM_COUNT)?,
            stream_size: constant(pinned::ecs::EVENT_STREAM_SIZE)?,
            stream_used: constant(pinned::ecs::EVENT_STREAM_USED)?,
            record_size: constant(pinned::ecs::EVENT_RECORD_SIZE)?,
            record_header: constant(pinned::ecs::EVENT_RECORD_HEADER)?,
            record_align: constant(pinned::ecs::EVENT_RECORD_ALIGN)?.max(1),
        })
    })();
    layout
        .inspect_err(|missing: &Constant| {
            let _off = loader.fuse_off(&DELIVERY_FUSE, missing.id(), crate::engine::builds::ABSENT);
        })
        .ok()
}

crate::engine::chains::chain_detour! {
    /// The engine's event-stream move. Copies selected events before the
    /// move and delivers each scope afterwards under independent fuses.
    fn merge(entity_world: *mut u8) -> ();
    next MERGE_NEXT;
    fuse &EVENTS_FUSE, "the event move";
    |onward| {
        let Some(&(loader, layout)) = LOADED.get() else {
            return onward.call();
        };
        let server = is_server(entity_world);
        let client_own = loader.side == Side::CLIENT
            && !entity_world.is_null()
            && entity_world.addr() as u64 != crate::engine::world::server_world();
        let scope = scope_for(loader.side, server, client_own);
        if let Some(layout) = &layout {
            if scope == Some(Scope::Server) {
                onward.run(&DELIVERY_FUSE, "the event copy", |_| {
                    let _quiet = crate::engine::world::Quiet::enter();
                    // SAFETY: is_server checks the live server world and its
                    // simulation thread before the engine moves its streams.
                    unsafe { copy(layout, entity_world, Scope::Server) };
                });
            } else if scope == Some(Scope::Client) {
                onward.run(&CLIENT_FUSE, "the client event copy", |_| {
                    // SAFETY: the existing merge route supplies the non-null
                    // client world on its own simulation thread before merge.
                    unsafe { copy(layout, entity_world, Scope::Client) };
                });
            }
        }
        if client_own {
            onward.run(&NOTICES_FUSE, "the notice raise", |_| {
                // SAFETY: client_own identifies the game's live client-world
                // argument on its simulation thread before the merge.
                unsafe { crate::base::notices::raise(loader, entity_world.expose_provenance()) };
            });
        }
        onward.call();
        if client_own || (loader.side != Side::CLIENT && server) {
            onward.run(&RETIRE_FUSE, "freeing what the game was handed", |_| {
                crate::engine::retire::step();
            });
        }
        if layout.is_some() && scope == Some(Scope::Server) {
            onward.run(&DELIVERY_FUSE, "the event delivery", |_| {
                let _quiet = crate::engine::world::Quiet::enter();
                deliver(loader, Scope::Server);
            });
        } else if layout.is_some() && scope == Some(Scope::Client) {
            onward.run(&CLIENT_FUSE, "the client event delivery", |_| {
                deliver(loader, Scope::Client);
            });
        }
    }
}

/// Whether `entity_world` is the running server's, moved on its game thread.
fn is_server(entity_world: *mut u8) -> bool {
    let server = crate::engine::world::server_world();
    server != 0 && entity_world.addr() as u64 == server && crate::engine::world::reader().is_ok()
}

/// Copies wanted events from one explicitly selected simulation world into
/// its own thread-local batch.
///
/// # Safety
///
/// - `entity_world` is the live simulation world identified by `scope`.
/// - Called on that world's simulation thread before the engine moves its
///   event streams, so no system writes them during the call.
unsafe fn copy(layout: &Layout, entity_world: *mut u8, scope: Scope) {
    let Some(sealed) = SEALED.get() else {
        return;
    };
    if scope == Scope::Client && sealed.client_wanted.is_empty() {
        return;
    }
    let world = entity_world.expose_provenance();
    // SAFETY: `world` is the selected live simulation world on its thread
    // (the caller's contract); the table's `ecs.event_streams` offset holds
    // its stream array pointer.
    let streams: usize = unsafe { crate::engine::memory::read(world, layout.streams) };
    // SAFETY: `world` is the selected live simulation world on its thread
    // (the caller's contract); the table's `ecs.event_stream_count` offset
    // holds its stream count.
    let count: usize = unsafe { crate::engine::memory::read(world, layout.stream_count) };
    if streams == 0 {
        return;
    }
    let mut batch = take_batch(scope);
    let mut records = 0;
    let streams_count = if scope == Scope::Client {
        count.min(CLIENT_STREAMS)
    } else {
        count
    };
    if streams_count != count {
        client_drop();
    }
    'streams: for index in 0..streams_count {
        let stream = streams + index * layout.stream_size;
        // SAFETY: `streams` is non-zero (`streams == 0` returns first) and
        // holds `count` streams of the table's `ecs.event_stream_size` bytes,
        // `index < count`; each starts with its buffer pointer; no system
        // writes them during the call (the caller's contract).
        let base: usize = unsafe { crate::engine::memory::read(stream, 0) };
        // SAFETY: `stream` is stream `index < count` of the non-zero
        // `streams`; the table's `ecs.event_stream_used` offset holds its
        // `u32` count of bytes used; no system writes it during the call (the
        // caller's contract).
        let used =
            unsafe { crate::engine::memory::read::<u32>(stream, layout.stream_used) } as usize;
        if base == 0 {
            continue;
        }
        let mut at = 0;
        while at + layout.record_header <= used {
            if scope == Scope::Client && records >= CLIENT_RECORDS {
                client_drop();
                break 'streams;
            }
            records += 1;
            let record = base + at;
            // SAFETY: `base` is non-zero (`base == 0` skips the stream) and
            // `record` lies inside its bytes used, header included (`at +
            // record_header <= used`, checked by the loop); a record starts
            // with its `u32` type hash.
            let type_hash: u32 = unsafe { crate::engine::memory::read(record, 0) };
            // SAFETY: `record`'s header lies inside the bytes used (checked by
            // the loop) and holds its payload size byte at the table's
            // `ecs.event_record_size` offset.
            let size = usize::from(unsafe {
                crate::engine::memory::read::<u8>(record, layout.record_size)
            });
            if at + layout.record_header + size > used {
                break;
            }
            if scope == Scope::Server {
                // SAFETY: the payload is the server event of `type_hash`
                // after its header, `size` bytes inside the bytes used (the
                // check above breaks first), before the engine moves streams.
                unsafe {
                    crate::actions::vetoes::screen(
                        type_hash,
                        core::ptr::with_exposed_provenance_mut(record + layout.record_header),
                        size,
                    );
                }
            }
            // Item notices are after the fact: they go out with the batch,
            // after the merge, so a filter may change the world.
            if sealed.wants(scope, type_hash)
                || scope == Scope::Server && crate::inventory::item_actions::hears(type_hash)
            {
                if scope == Scope::Client
                    && (batch.events.len() >= CLIENT_EVENTS
                        || batch.bytes.len().saturating_add(size) > CLIENT_BYTES)
                {
                    client_drop();
                    break 'streams;
                }
                batch.events.push(Copied {
                    type_hash,
                    size: u32::try_from(size).unwrap_or(0),
                    start: batch.bytes.len(),
                });
                let payload = record + layout.record_header;
                let start = batch.bytes.len();
                batch.bytes.resize(start + size, 0);
                // SAFETY: the payload's `size` bytes lie inside the bytes used
                // (the `> used` check breaks first); no system writes them
                // during the call (the caller's contract).
                unsafe { crate::engine::memory::read_bytes(payload, 0, &mut batch.bytes[start..]) };
            }
            at += layout.record_header + size.next_multiple_of(layout.record_align);
        }
    }
    give_back(scope, batch);
}

/// Hands this thread's batch to the subscribers still on, and empties it.
fn deliver(loader: &Loader, scope: Scope) {
    let Some(sealed) = SEALED.get() else {
        return;
    };
    let mut batch = take_batch(scope);
    for event in &batch.events {
        if scope == Scope::Server && sealed.equipment == Some(event.type_hash) {
            crate::inventory::stats::equipment_changed();
        }
        if scope == Scope::Server
            && let Some(payload) = usize::try_from(event.size)
                .ok()
                .and_then(|size| batch.bytes.get(event.start..event.start + size))
        {
            // SAFETY: `payload` is the event's copy in this batch, its `size`
            // bytes, which nothing writes during the call.
            unsafe {
                crate::inventory::item_actions::screen(
                    event.type_hash,
                    payload.as_ptr(),
                    payload.len(),
                );
            }
        }
        let data = GameEventData {
            header: Header::of::<GameEventData>(),
            type_hash: event.type_hash,
            size: event.size,
            data: batch.bytes[event.start..].as_ptr(),
        };
        for subscriber in sealed.subscribers.iter().filter(|s| {
            s.scope.matches(scope, s.type_hash, event.type_hash)
                && loader
                    .mods
                    .get(s.owner.get())
                    .is_some_and(|m| !m.off.load(Ordering::Acquire))
        }) {
            // SAFETY: `callback` is a `GameEventFn` the mod registered, in
            // its DLL, which stays loaded; `user` is the pointer it
            // registered with it, and `data` and the bytes it points at live
            // in this batch for the call.
            crate::engine::profile::call(
                subscriber.owner,
                crate::engine::profile::Feature::GameEvents,
                || unsafe {
                    (subscriber.callback)(
                        &raw const data,
                        core::ptr::with_exposed_provenance_mut(subscriber.user),
                    );
                },
            );
        }
    }
    batch.events.clear();
    batch.bytes.clear();
    give_back(scope, batch);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_client_and_server_scopes_never_cross() {
        assert_eq!(scope_for(Side::CLIENT, true, false), Some(Scope::Server));
        assert_eq!(scope_for(Side::CLIENT, false, true), Some(Scope::Client));
        assert_eq!(scope_for(Side::SERVER, false, true), None);
        assert_eq!(scope_for(Side::CLIENT, false, false), None);
        assert!(!Scope::Client.matches(Scope::Server, 42, 42));
        assert!(!Scope::Server.matches(Scope::Client, 0, 42));
        assert!(Scope::Server.matches(Scope::Server, 0, 42));
        assert!(Scope::Client.matches(Scope::Client, 42, 42));
        assert!(!client_registration(Side::SERVER, 42, 0).is_ok());
        assert!(!client_registration(Side::CLIENT, 0, 0).is_ok());
        assert!(!client_registration(Side::CLIENT, 42, 32).is_ok());
        assert!(client_registration(Side::CLIENT, 42, 31).is_ok());
    }

    #[test]
    fn client_batches_are_separate_and_reentrant_copies_stay_bounded() {
        give_back(Scope::Server, Batch::default());
        give_back(Scope::Client, Batch::default());
        let mut server = take_batch(Scope::Server);
        copied(1, b"server", &mut server);
        give_back(Scope::Server, server);
        let mut client = take_batch(Scope::Client);
        copied(2, b"client", &mut client);
        give_back(Scope::Client, client);
        assert_eq!(
            payloads(&take_batch(Scope::Server)),
            [(1, b"server".to_vec())]
        );
        assert_eq!(
            payloads(&take_batch(Scope::Client)),
            [(2, b"client".to_vec())]
        );
        let mut outer = Batch::default();
        for _ in 0..CLIENT_EVENTS {
            copied(3, b"x", &mut outer);
        }
        CLIENT_BATCH.with_borrow_mut(|held| copied(4, b"y", held));
        give_back(Scope::Client, outer);
        let back = take_batch(Scope::Client);
        assert_eq!(back.events.len(), CLIENT_EVENTS);
        assert!(back.bytes.len() <= CLIENT_BYTES);
    }

    #[test]
    fn no_client_subscriptions_request_no_client_event_types() {
        let sealed = Sealed {
            subscribers: Box::new([]),
            wanted: [42].into_iter().collect(),
            client_wanted: FxHashSet::default(),
            every: true,
            equipment: Some(43),
        };
        assert!(!sealed.wants(Scope::Client, 42));
        assert!(!sealed.wants(Scope::Client, 43));
        assert!(sealed.wants(Scope::Server, 99));
    }

    fn copied(type_hash: u32, bytes: &[u8], into: &mut Batch) {
        into.events.push(Copied {
            type_hash,
            size: u32::try_from(bytes.len()).unwrap(),
            start: into.bytes.len(),
        });
        into.bytes.extend_from_slice(bytes);
    }

    fn payloads(batch: &Batch) -> Vec<(u32, Vec<u8>)> {
        batch
            .events
            .iter()
            .map(|e| {
                let end = e.start + e.size as usize;
                (e.type_hash, batch.bytes[e.start..end].to_vec())
            })
            .collect()
    }

    #[test]
    fn a_batch_taken_out_leaves_the_cell_free_and_keeps_events_copied_meanwhile() {
        let mut first = take_batch(Scope::Server);
        copied(1, b"ab", &mut first);
        BATCH.with_borrow_mut(|held| copied(2, b"cde", held));
        give_back(Scope::Server, first);
        let back = take_batch(Scope::Server);
        assert_eq!(payloads(&back), [(1, b"ab".to_vec()), (2, b"cde".to_vec())]);
        give_back(Scope::Server, Batch::default());
        BATCH.with_borrow(|held| assert!(held.events.is_empty() && held.bytes.is_empty()));
    }
    #[test]
    fn a_build_without_the_stream_layout_lists_only_the_delivery_off_by_the_entry_it_lacks() {
        let loader = crate::start::tests::bare_loader();
        assert!(stream_layout(&loader).is_none());
        assert!(DELIVERY_FUSE.is_off());
        assert!(!EVENTS_FUSE.is_off(), "the move still links for the rest");
        let reason = crate::base::off_reason(&DELIVERY_FUSE).unwrap_or_default();
        assert!(
            reason.starts_with(pinned::ecs::EVENT_STREAMS.id()),
            "{reason}"
        );
    }
}
