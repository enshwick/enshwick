//! Game events for mods, copied as the engine moves a world's event streams.
//!
//! Every ECS event a world's systems write goes into an event stream, and
//! the engine moves the streams into its event queues at the start of each
//! step, on the thread that steps that world, before any system runs. The
//! loader links that move, copies the events a mod subscribed to and hands
//! them over right after it, with the world still quiescent and the
//! entities they name still in it.
//!
//! The same pass lets the craft filters screen the craft requests in the
//! server's streams before the engine moves them (see the vetoes module).
//!
//! A client moves its own world's events as well as, while it hosts a game,
//! the built-in server's. Mods subscribe to each world apart: the client's
//! own events go to their handlers in a bounded batch of their own, with no
//! filter, veto or item action run on them.

use core::cell::RefCell;
use core::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::thread::LocalKey;

use enshwick_core::abi::{GameEventData, GameEventFn, Header, HookFlags, Side, Status};
use rustc_hash::FxHashSet;

use crate::engine::chains::Onward;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::pinned::Constant;
use crate::engine::registry::Registry;
use crate::engine::world::Whose;

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

/// The most events of the client's own world one move copies for its mods.
const CLIENT_EVENTS: usize = 256;
/// The most bytes of those events one move copies.
const CLIENT_BYTES: usize = 64 * 1024;
/// The most records one move of the client's own world reads, copied or not.
const CLIENT_RECORDS: usize = 65_536;
/// The most streams one move of the client's own world reads.
const CLIENT_STREAMS: usize = 256;

/// Tells the log, once, that a move of the client's own world held more
/// than its bounds let the loader copy.
#[cold]
fn client_drop() {
    crate::log_once!(
        WARN,
        "client game events: a move held more than {CLIENT_EVENTS} events, {CLIENT_BYTES} bytes, \
         {CLIENT_RECORDS} records or {CLIENT_STREAMS} streams; the rest of each such move is not \
         handed to mods"
    );
}

/// A mod's handler for one event type of one world, or for all with hash 0.
#[derive(Debug, Clone, Copy)]
struct Subscriber {
    whose: Whose,
    owner: ModIndex,
    type_hash: u32,
    callback: GameEventFn,
    user: usize,
}

/// One world's handlers and the event types they asked for.
#[derive(Debug, Default)]
struct Wanted {
    subscribers: &'static [Subscriber],
    types: FxHashSet<u32>,
    every: bool,
}

impl Wanted {
    /// What one world's `subscribers`, a run of the sealed slice, want.
    fn of(subscribers: &'static [Subscriber]) -> Self {
        let mut wanted = Self {
            subscribers,
            ..Self::default()
        };
        for subscriber in subscribers {
            if subscriber.type_hash == 0 {
                wanted.every = true;
            } else {
                wanted.types.insert(subscriber.type_hash);
            }
        }
        wanted
    }

    fn contains(&self, type_hash: u32) -> bool {
        self.every || self.types.contains(&type_hash)
    }

    fn is_empty(&self) -> bool {
        !self.every && self.types.is_empty()
    }
}

/// The subscriptions once every mod is initialised.
#[derive(Debug)]
struct Sealed {
    server: Wanted,
    client: Wanted,
    /// The server's equipment event, when stats follow it.
    equipment: Option<u32>,
    /// Whether the craft filters or item actions read the server's events.
    screened: bool,
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

/// Events copied during one move, on the thread that steps the world only.
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

    fn is_empty(&self) -> bool {
        self.events.is_empty() && self.bytes.is_empty()
    }
}

/// What differs between moving the server's events and the client's own:
/// one impl each, so the copy and the delivery hold no case of their own,
/// and the server's move compiles without the client's bounds.
trait Scope {
    /// Whether a move stops at the client's bounds.
    const BOUNDED: bool;

    /// This thread's batch of the world's copied events.
    fn batch() -> &'static LocalKey<RefCell<Batch>>;

    /// The world's handlers.
    fn wanted(sealed: &Sealed) -> &Wanted;

    /// Whether no handler and no part of the loader wants any event here.
    fn idle(sealed: &Sealed) -> bool;

    /// Whether a move copies the events of `type_hash`.
    fn copies(sealed: &Sealed, type_hash: u32) -> bool;

    /// Lets the loader's filters see the event of `type_hash` at `payload`
    /// in the stream, `size` bytes, before the engine moves it.
    ///
    /// # Safety
    ///
    /// `payload` is the live event's `size` bytes in a stream of this
    /// world, on its thread before the engine moves the streams.
    unsafe fn screen(type_hash: u32, payload: usize, size: usize);

    /// Lets the loader's own followers see a copied event before the mods.
    fn follow(sealed: &Sealed, type_hash: u32, payload: &[u8]);
}

/// The running server's world, dedicated or hosted by a client.
#[derive(Debug)]
struct ServerScope;

/// The client's own world.
#[derive(Debug)]
struct ClientScope;

impl Scope for ServerScope {
    const BOUNDED: bool = false;

    fn batch() -> &'static LocalKey<RefCell<Batch>> {
        &BATCH
    }

    fn wanted(sealed: &Sealed) -> &Wanted {
        &sealed.server
    }

    fn idle(sealed: &Sealed) -> bool {
        sealed.server.is_empty() && sealed.equipment.is_none() && !sealed.screened
    }

    #[inline]
    fn copies(sealed: &Sealed, type_hash: u32) -> bool {
        sealed.server.contains(type_hash)
            || sealed.equipment == Some(type_hash)
            || crate::inventory::item_actions::hears(type_hash)
    }

    #[inline]
    unsafe fn screen(type_hash: u32, payload: usize, size: usize) {
        // SAFETY: `payload` is the live server event's `size` bytes before
        // the engine moves the streams (this function's contract).
        unsafe {
            crate::actions::vetoes::screen(
                type_hash,
                core::ptr::with_exposed_provenance_mut(payload),
                size,
            );
        }
    }

    fn follow(sealed: &Sealed, type_hash: u32, payload: &[u8]) {
        if sealed.equipment == Some(type_hash) {
            crate::inventory::stats::equipment_changed();
        }
        // SAFETY: `payload` is the event's copy in the batch, which nothing
        // writes during the call.
        unsafe {
            crate::inventory::item_actions::screen(type_hash, payload.as_ptr(), payload.len());
        }
    }
}

impl Scope for ClientScope {
    const BOUNDED: bool = true;

    fn batch() -> &'static LocalKey<RefCell<Batch>> {
        &CLIENT_BATCH
    }

    fn wanted(sealed: &Sealed) -> &Wanted {
        &sealed.client
    }

    fn idle(sealed: &Sealed) -> bool {
        sealed.client.is_empty()
    }

    fn copies(sealed: &Sealed, type_hash: u32) -> bool {
        sealed.client.contains(type_hash)
    }

    unsafe fn screen(_type_hash: u32, _payload: usize, _size: usize) {}

    fn follow(_sealed: &Sealed, _type_hash: u32, _payload: &[u8]) {}
}

/// Puts `batch` back into this thread's cell of `S`, after the events a
/// callback copied meanwhile, keeping its buffers when there were none.
fn give_back<S: Scope>(mut batch: Batch) {
    S::batch().with_borrow_mut(|held| {
        if held.is_empty() {
            core::mem::swap(held, &mut batch);
            return;
        }
        let newer = core::mem::take(held);
        if S::BOUNDED
            && (batch.events.len() + newer.events.len() > CLIENT_EVENTS
                || batch.bytes.len() + newer.bytes.len() > CLIENT_BYTES)
        {
            client_drop();
        } else {
            batch.append(&newer);
        }
        *held = batch;
    });
}

/// The handlers mods register in their init, sealed at link.
static SUBSCRIBERS: Registry<Subscriber> = Registry::new();
static SEALED: OnceLock<Sealed> = OnceLock::new();
/// The build's stream layout when it has one, once linked.
static LINKED: OnceLock<Option<Layout>> = OnceLock::new();
/// The event move's link, off only when this build cannot link it or its
/// own checks panicked: the features it runs have fuses of their own, so
/// one of them failing leaves the others running.
static EVENTS_FUSE: Fuse = Fuse::new("game events");
/// The copy and delivery of the server's events to the mods.
static DELIVERY_FUSE: Fuse = Fuse::new("game event delivery");
/// The copy and delivery of the client's own events to the mods.
static CLIENT_FUSE: Fuse = Fuse::new("client game events");
/// The client's notices from its server, off once raising one panicked.
static NOTICES_FUSE: Fuse = Fuse::new("server notices");
/// The switch for freeing what the loader handed the game.
static RETIRE_FUSE: Fuse = Fuse::new("freeing handed lists");

thread_local! {
    static BATCH: RefCell<Batch> = RefCell::new(Batch::default());
    static CLIENT_BATCH: RefCell<Batch> = RefCell::new(Batch::default());
}

/// Registers mod `owner`'s handler for the events of `type_hash` in the
/// world `whose`, 0 for every type, on the loader of `side`, while `owner`
/// runs its `init`.
pub(crate) fn register(
    side: Side,
    whose: Whose,
    owner: ModIndex,
    in_init: bool,
    type_hash: u32,
    callback: GameEventFn,
    user: *mut c_void,
) -> Status {
    let subscriber = Subscriber {
        whose,
        owner,
        type_hash,
        callback,
        user: user.expose_provenance(),
    };
    keep(&SUBSCRIBERS, side, in_init, subscriber)
}

/// Keeps `subscriber` in `registry`: too late outside the mod's `init` or
/// once the registry is sealed, and OK without keeping it for a client
/// handler on a dedicated server, where no client world steps.
fn keep(
    registry: &Registry<Subscriber>,
    side: Side,
    in_init: bool,
    subscriber: Subscriber,
) -> Status {
    if !in_init {
        Status::TOO_LATE
    } else if subscriber.whose == Whose::Client && side != Side::CLIENT {
        Status::OK
    } else {
        registry
            .add(subscriber)
            .map_or_else(Status::from, |()| Status::OK)
    }
}

/// Orders sealed subscribers server first, each world's in the order mods
/// registered them, so each world's handlers are one run of the slice.
fn server_first(mut subscribers: Vec<Subscriber>) -> Vec<Subscriber> {
    subscribers.sort_by_key(|subscriber| subscriber.whose == Whose::Client);
    subscribers
}

/// Each world's handlers in `sealed`, which [`server_first`] ordered.
fn by_world(sealed: &'static [Subscriber]) -> (Wanted, Wanted) {
    let (server, client) =
        sealed.split_at(sealed.partition_point(|subscriber| subscriber.whose == Whose::Server));
    (Wanted::of(server), Wanted::of(client))
}

/// Seals the subscriptions and links the event move when a mod subscribed.
pub(crate) fn link(loader: &'static Loader) {
    let subscribers = SUBSCRIBERS.seal_with(server_first);
    let (server, client) = by_world(subscribers);
    let equipment = crate::inventory::stats::wanted()
        .then(|| {
            crate::engine::catalogue::component_hash(
                loader,
                crate::engine::catalogue::ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT,
            )
        })
        .flatten();
    let sealed = Sealed {
        server,
        client,
        equipment,
        screened: crate::actions::vetoes::crafting() || crate::inventory::item_actions::wanted(),
    };
    let count = subscribers.len() + usize::from(equipment.is_some());
    let client_wanted = !sealed.client.is_empty();
    // A client always links the move, to raise the loader's notices in its
    // own world.
    let idle = ServerScope::idle(&sealed) && loader.side != Side::CLIENT;
    if SEALED.set(sealed).is_err() || idle {
        return;
    }
    // Without the layout the move still links: a client's notices and the
    // freeing of handed lists need no stream.
    let _first = LINKED.set(stream_layout(loader, client_wanted));
    let target = Target::Pinned(pinned::ecs::EVENT_MERGE);
    if loader.link_fused(&EVENTS_FUSE, target, &MERGE_NEXT, merge, HookFlags::NONE) {
        crate::log!(INFO, "game events: {count} subscriptions");
    }
}

/// The build's event stream layout; without it the delivery is off, listed
/// by the first entry the build lacks, and on a client with `client_wanted`
/// handlers the delivery of its own events too.
fn stream_layout(loader: &Loader, client_wanted: bool) -> Option<Layout> {
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
            let absent = crate::engine::builds::ABSENT;
            let _off = loader.fuse_off(&DELIVERY_FUSE, missing.id(), absent);
            if loader.side == Side::CLIENT && client_wanted {
                let _off = loader.fuse_off(&CLIENT_FUSE, missing.id(), absent);
            }
        })
        .ok()
}

crate::engine::chains::chain_detour! {
    /// The engine's move of a world's event streams at the table's
    /// `ecs.event_merge`, on the thread that steps the world it is handed:
    /// copies the wanted events of a world the loader can tell before the
    /// move and hands them to the mods after it, raises a client's notices
    /// and frees what the game was handed, each under its own fuse.
    fn merge(entity_world: *mut u8) -> ();
    next MERGE_NEXT;
    fuse &EVENTS_FUSE, "the event move";
    |onward| {
        let Some((loader, &layout)) = crate::engine::loader::linked(&LINKED) else {
            return onward.call();
        };
        match crate::engine::world::whose(loader.side, entity_world.expose_provenance()) {
            Some(Whose::Server) => server_move(onward, loader, layout.as_ref(), entity_world),
            Some(Whose::Client) => client_move(onward, loader, layout.as_ref(), entity_world),
            None => onward.call(),
        }
    }
}

/// The move of the running server's world, around the game's.
fn server_move<N: Fn()>(
    onward: &Onward<(), N>,
    loader: &'static Loader,
    layout: Option<&Layout>,
    entity_world: *mut u8,
) {
    if let Some(layout) = layout {
        onward.before(&DELIVERY_FUSE, "the event copy", || {
            let _quiet = crate::engine::world::Quiet::enter();
            if let Some(sealed) = SEALED.get() {
                // SAFETY: `whose` told `entity_world` the running server's,
                // on its game thread; the game handed it to
                // `ecs.event_merge`, which has not moved its streams yet
                // (`before` never calls the chain).
                unsafe { copy::<ServerScope>(sealed, layout, entity_world) };
            }
        });
    }
    onward.call();
    // A client frees what it handed the game in its own world's move.
    if loader.side != Side::CLIENT {
        onward.run(&RETIRE_FUSE, "freeing what the game was handed", |_| {
            crate::engine::retire::step();
        });
    }
    if layout.is_some() {
        onward.run(&DELIVERY_FUSE, "the event delivery", |_| {
            let _quiet = crate::engine::world::Quiet::enter();
            deliver::<ServerScope>(loader);
        });
    }
}

/// The move of the client's own world, around the game's.
fn client_move<N: Fn()>(
    onward: &Onward<(), N>,
    loader: &'static Loader,
    layout: Option<&Layout>,
    entity_world: *mut u8,
) {
    // The copy comes first: the notices the raise adds to the client's
    // streams are the loader's own, not the game's events.
    if let Some(layout) = layout {
        onward.before(&CLIENT_FUSE, "the client event copy", || {
            if let Some(sealed) = SEALED.get() {
                // SAFETY: `whose` told `entity_world` the client's own,
                // handed to `ecs.event_merge` on the thread that steps it,
                // which has not moved its streams yet (`before` never calls
                // the chain).
                unsafe { copy::<ClientScope>(sealed, layout, entity_world) };
            }
        });
    }
    onward.before(&NOTICES_FUSE, "the notice raise", || {
        // SAFETY: `whose` told `entity_world` the client's own, handed to
        // `ecs.event_merge` on the thread that steps it, which has not moved
        // its streams yet (`before` never calls the chain).
        unsafe { crate::base::notices::raise(loader, entity_world.expose_provenance()) };
    });
    onward.call();
    onward.run(&RETIRE_FUSE, "freeing what the game was handed", |_| {
        crate::engine::retire::step();
    });
    if layout.is_some() {
        onward.run(&CLIENT_FUSE, "the client event delivery", |_| {
            deliver::<ClientScope>(loader);
        });
    }
}

/// Copies the events `S` wants of `sealed` from `entity_world`'s streams
/// into this thread's batch of `S`, letting the loader's filters screen
/// each first.
///
/// # Safety
///
/// - `entity_world` is the live entity world of `S`, whose streams lie as
///   `layout` says.
/// - Called on the thread that steps it, before the engine moves its event
///   streams, so no system writes them during the call.
unsafe fn copy<S: Scope>(sealed: &Sealed, layout: &Layout, entity_world: *mut u8) {
    if S::idle(sealed) {
        return;
    }
    let world = entity_world.expose_provenance();
    // SAFETY: `world` is the live entity world of `S` on its thread (the
    // caller's contract); the table's `ecs.event_streams` offset holds its
    // stream array pointer, which `ecs.event_merge` walks (client 26430625).
    let streams: usize = unsafe { crate::engine::memory::read(world, layout.streams) };
    // SAFETY: `world` is the live entity world of `S` on its thread (the
    // caller's contract); the table's `ecs.event_stream_count` offset holds
    // its stream count, which `ecs.event_merge` walks (client 26430625).
    let mut count: usize = unsafe { crate::engine::memory::read(world, layout.stream_count) };
    if streams == 0 {
        return;
    }
    if S::BOUNDED && count > CLIENT_STREAMS {
        client_drop();
        count = CLIENT_STREAMS;
    }
    let mut batch = S::batch().take();
    let mut records = 0;
    'streams: for index in 0..count {
        let stream = streams + index * layout.stream_size;
        // SAFETY: `streams` is non-zero (`streams == 0` returns first) and
        // holds at least `count` streams of the table's
        // `ecs.event_stream_size` bytes, `index < count`; each starts with
        // its buffer pointer; no system writes them during the call (the
        // caller's contract).
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
            if S::BOUNDED {
                if records == CLIENT_RECORDS {
                    client_drop();
                    break 'streams;
                }
                records += 1;
            }
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
            let payload = record + layout.record_header;
            // SAFETY: the payload is the live event of `type_hash` after the
            // record's header, `size` bytes inside the bytes used (the `> used`
            // check breaks first), on the world's thread before the engine
            // moves the streams (the caller's contract).
            unsafe { S::screen(type_hash, payload, size) };
            // Item notices are after the fact: they go out with the batch,
            // after the move, so a filter may change the world.
            if S::copies(sealed, type_hash) {
                if S::BOUNDED
                    && (batch.events.len() == CLIENT_EVENTS
                        || batch.bytes.len() + size > CLIENT_BYTES)
                {
                    client_drop();
                    break 'streams;
                }
                let start = batch.bytes.len();
                batch.events.push(Copied {
                    type_hash,
                    size: u32::try_from(size).unwrap_or(0),
                    start,
                });
                batch.bytes.resize(start + size, 0);
                // SAFETY: the payload's `size` bytes lie inside the bytes used
                // (the `> used` check breaks first); no system writes them
                // during the call (the caller's contract).
                unsafe { crate::engine::memory::read_bytes(payload, 0, &mut batch.bytes[start..]) };
            }
            at += layout.record_header + size.next_multiple_of(layout.record_align);
        }
    }
    give_back::<S>(batch);
}

/// Hands this thread's batch of `S` to the loader's followers and to the
/// handlers of mods still on, and empties it.
fn deliver<S: Scope>(loader: &Loader) {
    let Some(sealed) = SEALED.get() else {
        return;
    };
    let mut batch = S::batch().take();
    for event in &batch.events {
        let Some(payload) = usize::try_from(event.size)
            .ok()
            .and_then(|size| batch.bytes.get(event.start..event.start + size))
        else {
            continue;
        };
        S::follow(sealed, event.type_hash, payload);
        let data = GameEventData {
            header: Header::of::<GameEventData>(),
            type_hash: event.type_hash,
            size: event.size,
            data: payload.as_ptr(),
        };
        for subscriber in S::wanted(sealed).subscribers.iter().filter(|s| {
            (s.type_hash == 0 || s.type_hash == event.type_hash)
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
    give_back::<S>(batch);
}

/// The server's copy of one move with `wanted` the one type a handler asked
/// for, then the batch emptied, for `benches/event_copy.rs`; it leaves the
/// loader's own subscriptions alone. `offsets` are the stream layout in the
/// build table's order: streams, stream count, stream size, stream used,
/// record size, record header, record align.
///
/// # Safety
///
/// `entity_world` holds a stream array laid out as `offsets` says, readable
/// and written by nothing else during the call.
#[doc(hidden)]
pub unsafe fn bench_server_copy(entity_world: *mut u8, offsets: [usize; 7], wanted: u32) {
    let [
        streams,
        stream_count,
        stream_size,
        stream_used,
        record_size,
        record_header,
        record_align,
    ] = offsets;
    let sealed = Sealed {
        server: Wanted {
            types: [wanted].into_iter().collect(),
            ..Wanted::default()
        },
        client: Wanted::default(),
        equipment: None,
        screened: false,
    };
    let layout = Layout {
        streams,
        stream_count,
        stream_size,
        stream_used,
        record_size,
        record_header,
        record_align,
    };
    // SAFETY: `entity_world` holds streams laid out as `layout` says, which
    // nothing else reads or writes during the call (this function's
    // `# Safety`), all `copy` asks of a world and its thread.
    unsafe { copy::<ServerScope>(&sealed, &layout, entity_world) };
    BATCH.with_borrow_mut(|batch| {
        batch.events.clear();
        batch.bytes.clear();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn unused(_event: *const GameEventData, _user: *mut c_void) {}

    fn subscriber(whose: Whose, type_hash: u32) -> Subscriber {
        Subscriber {
            whose,
            owner: ModIndex::new(0),
            type_hash,
            callback: unused,
            user: 0,
        }
    }

    fn sealed_with(subscribers: &[Subscriber]) -> Sealed {
        let (server, client) = by_world(server_first(subscribers.to_vec()).leak());
        Sealed {
            server,
            client,
            equipment: None,
            screened: false,
        }
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
    fn a_dedicated_server_answers_a_client_handler_without_keeping_it() {
        let registry = Registry::new();
        let client = subscriber(Whose::Client, 7);
        assert_eq!(keep(&registry, Side::SERVER, true, client), Status::OK);
        assert_eq!(
            keep(&registry, Side::SERVER, true, subscriber(Whose::Server, 7)),
            Status::OK
        );
        assert_eq!(keep(&registry, Side::CLIENT, true, client), Status::OK);
        let kept: Vec<Whose> = registry.seal().iter().map(|s| s.whose).collect();
        assert_eq!(kept, [Whose::Server, Whose::Client]);
    }

    #[test]
    fn a_handler_registered_outside_its_mod_s_init_is_too_late_on_either_world() {
        let registry = Registry::new();
        for side in [Side::CLIENT, Side::SERVER] {
            for whose in [Whose::Server, Whose::Client] {
                assert_eq!(
                    keep(&registry, side, false, subscriber(whose, 7)),
                    Status::TOO_LATE,
                    "{whose:?}"
                );
            }
        }
        assert!(registry.seal().is_empty());
    }

    #[test]
    fn a_handler_registered_in_init_after_the_seal_is_too_late_and_not_kept() {
        let registry = Registry::new();
        let _sealed = registry.seal();
        for side in [Side::CLIENT, Side::SERVER] {
            assert_eq!(
                keep(&registry, side, true, subscriber(Whose::Server, 7)),
                Status::TOO_LATE,
                "{side:?}"
            );
        }
        assert!(registry.seal().is_empty());
    }

    #[test]
    fn each_world_keeps_its_handlers_in_the_order_mods_registered_them() {
        let sealed = sealed_with(&[
            subscriber(Whose::Client, 1),
            subscriber(Whose::Server, 2),
            subscriber(Whose::Client, 3),
            subscriber(Whose::Server, 4),
        ]);
        let hashes = |wanted: &Wanted| -> Vec<u32> {
            wanted.subscribers.iter().map(|s| s.type_hash).collect()
        };
        assert_eq!(hashes(&sealed.server), [2, 4]);
        assert_eq!(hashes(&sealed.client), [1, 3]);
    }

    #[test]
    fn each_world_copies_only_what_its_own_handlers_want() {
        let sealed = sealed_with(&[subscriber(Whose::Server, 42), subscriber(Whose::Client, 7)]);
        assert!(ServerScope::copies(&sealed, 42));
        assert!(!ServerScope::copies(&sealed, 7));
        assert!(ClientScope::copies(&sealed, 7));
        assert!(!ClientScope::copies(&sealed, 42));
        assert!(!ClientScope::idle(&sealed));
        let every = sealed_with(&[subscriber(Whose::Client, 0)]);
        assert!(ClientScope::copies(&every, 99));
        assert!(!ServerScope::copies(&every, 99));
        let hashes = |wanted: &Wanted| {
            wanted
                .subscribers
                .iter()
                .map(|s| s.type_hash)
                .collect::<Vec<_>>()
        };
        assert_eq!(hashes(ServerScope::wanted(&sealed)), [42]);
        assert_eq!(hashes(ClientScope::wanted(&sealed)), [7]);
    }

    #[test]
    fn a_server_world_nothing_wants_is_idle() {
        let client_only = sealed_with(&[subscriber(Whose::Client, 7)]);
        assert!(
            ServerScope::idle(&client_only),
            "no handler, stats, filter or item action"
        );
        assert!(!ServerScope::idle(&sealed_with(&[subscriber(
            Whose::Server,
            42
        )])));
        let equipment = Sealed {
            equipment: Some(43),
            ..sealed_with(&[])
        };
        assert!(!ServerScope::idle(&equipment));
        let screened = Sealed {
            screened: true,
            ..sealed_with(&[])
        };
        assert!(
            !ServerScope::idle(&screened),
            "a craft filter or item action reads them"
        );
    }

    #[test]
    fn a_client_with_only_server_handlers_copies_nothing_of_its_own() {
        let sealed = Sealed {
            equipment: Some(43),
            ..sealed_with(&[subscriber(Whose::Server, 0)])
        };
        assert!(ClientScope::idle(&sealed));
        assert!(!ClientScope::copies(&sealed, 43));
        assert!(ServerScope::copies(&sealed, 99));
    }

    #[test]
    fn the_worlds_batches_are_apart_and_events_copied_meanwhile_stay_bounded() {
        let mut server = ServerScope::batch().take();
        copied(1, b"server", &mut server);
        give_back::<ServerScope>(server);
        let mut client = ClientScope::batch().take();
        copied(2, b"client", &mut client);
        give_back::<ClientScope>(client);
        assert_eq!(
            payloads(&ServerScope::batch().take()),
            [(1, b"server".to_vec())]
        );
        assert_eq!(
            payloads(&ClientScope::batch().take()),
            [(2, b"client".to_vec())]
        );
        let mut outer = Batch::default();
        for _ in 0..CLIENT_EVENTS {
            copied(3, b"x", &mut outer);
        }
        CLIENT_BATCH.with_borrow_mut(|held| copied(4, b"y", held));
        give_back::<ClientScope>(outer);
        let back = ClientScope::batch().take();
        assert_eq!(back.events.len(), CLIENT_EVENTS);
        assert!(back.bytes.len() <= CLIENT_BYTES);
    }

    #[test]
    fn a_batch_taken_out_leaves_the_cell_free_and_keeps_events_copied_meanwhile() {
        let mut first = ServerScope::batch().take();
        copied(1, b"ab", &mut first);
        BATCH.with_borrow_mut(|held| copied(2, b"cde", held));
        give_back::<ServerScope>(first);
        let back = ServerScope::batch().take();
        assert_eq!(payloads(&back), [(1, b"ab".to_vec()), (2, b"cde".to_vec())]);
        give_back::<ServerScope>(Batch::default());
        BATCH.with_borrow(|held| assert!(held.is_empty()));
    }

    #[test]
    fn a_build_without_the_stream_layout_lists_only_the_delivery_off_by_the_entry_it_lacks() {
        let loader = crate::start::tests::bare_loader();
        assert!(stream_layout(&loader, true).is_none());
        assert!(DELIVERY_FUSE.is_off());
        assert!(!EVENTS_FUSE.is_off(), "the move still links for the rest");
        let reason = crate::base::off_reason(&DELIVERY_FUSE).unwrap_or_default();
        assert!(
            reason.starts_with(pinned::ecs::EVENT_STREAMS.id()),
            "{reason}"
        );
    }

    #[test]
    fn a_client_build_without_the_stream_layout_lists_its_client_events_off_too() {
        let loader = crate::start::tests::pinning_loader(Side::CLIENT, &[], &[]);
        assert!(stream_layout(&loader, false).is_none());
        assert!(
            !CLIENT_FUSE.is_off(),
            "no mod asked for client events, so none is listed off"
        );
        assert!(stream_layout(&loader, true).is_none());
        assert!(DELIVERY_FUSE.is_off());
        assert!(
            CLIENT_FUSE.is_off(),
            "a client mod's handler would never run"
        );
        let reason = crate::base::off_reason(&CLIENT_FUSE).unwrap_or_default();
        assert!(
            reason.starts_with(pinned::ecs::EVENT_STREAMS.id()),
            "{reason}"
        );
    }
}
