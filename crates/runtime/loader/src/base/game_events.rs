//! Game events for mods, copied as the engine moves a world's event streams.
//!
//! Every ECS event a world's systems write goes into an event stream, and
//! the engine moves the streams into its event queues at the start of each
//! step, on the thread that steps that world, before any system runs. The
//! loader links that move, copies the events a mod subscribed to and hands
//! them over right after it, with the world still quiescent and the
//! entities they name still in it.
//!
//! The same pass hands the loader's own followers, which the parts name in
//! their lists, the server's events they read: a screen sees the live event
//! before the engine moves it, a follow its copy after the move.
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

use enshwick_core::abi::{GameEventData, GameEventFn, HookFlags, Side};
use rustc_hash::FxHashSet;

use crate::engine::chains::Onward;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::parts::{EventFollow, EventFollower, EventScreen};
use crate::engine::pinned;
use crate::engine::pinned::Constant;
use crate::engine::registry::{JoinFeatureError, Registry};
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
/// The most streams a world's move may claim. The engine gives each world
/// one stream per worker of its task queue, counted from a 64-bit worker
/// mask that the world's creator clamps to 8 workers (client 26430625,
/// server 26430253), so a larger count is a corrupted one.
const STREAMS: usize = 64;

/// Tells the log, once, that a move of the client's own world held more
/// than its bounds let the loader copy.
#[cold]
fn client_drop() {
    crate::log_once!(
        WARN,
        "client game events: a move held more than {CLIENT_EVENTS} events, {CLIENT_BYTES} bytes \
         or {CLIENT_RECORDS} records; the rest of each such move is not handed to mods"
    );
}

/// Tells the log, once, that a world claimed more event streams than the
/// engine can hold.
#[cold]
fn too_many_streams(count: usize) {
    crate::log_once!(
        WARN,
        "game events: a world claimed {count} event streams, more than {STREAMS}; such a move is \
         not read"
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

/// The subscriptions once every mod is initialised, and what the loader's
/// own followers read of the server's events.
#[derive(Debug, Default)]
struct Sealed {
    /// The server's handlers, with the types the followers follow among
    /// those the move copies.
    server: Wanted,
    client: Wanted,
    /// Each type a follower screens, with its screen.
    screens: Box<[(u32, EventScreen)]>,
    /// Each type a follower follows, with its follow.
    follows: Box<[(u32, EventFollow)]>,
    /// The switches of the followers that read the server's events.
    fuses: Box<[&'static Fuse]>,
    /// Whether a follower reads the server's events.
    followed: bool,
}

impl Sealed {
    /// Adds what `followers` read, each with its types on this run.
    fn with_followers(mut self, followers: &[(EventFollower, Vec<u32>)]) -> Self {
        let mut screens = Vec::new();
        let mut follows = Vec::new();
        for (follower, types) in followers {
            if let Some(screen) = follower.screen {
                screens.extend(types.iter().map(|&hash| (hash, screen)));
            }
            // A screen sees the live event, so only a follow needs the copy.
            if let Some(follow) = follower.follow {
                self.server.types.extend(types);
                follows.extend(types.iter().map(|&hash| (hash, follow)));
            }
        }
        self.screens = screens.into();
        self.follows = follows.into();
        self.fuses = followers.iter().filter_map(|(f, _)| f.fuse).collect();
        self.followed = !followers.is_empty();
        self
    }

    /// Lists the switch of every follower that reads the server's events
    /// off, by the entry `id` the build lacks for `reason`: they read them
    /// only in the event move.
    fn unfollowed(&self, loader: &Loader, id: &str, reason: &str) {
        for &fuse in &self.fuses {
            let _off = loader.fuse_off(fuse, id, reason);
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

    /// Lets the loader's followers screen the event of `type_hash` at
    /// `payload` in the stream, `size` bytes, before the engine moves it.
    ///
    /// # Safety
    ///
    /// `payload` is the live event's `size` bytes in a stream of this
    /// world, on its thread before the engine moves the streams.
    unsafe fn screen(sealed: &Sealed, type_hash: u32, payload: usize, size: usize);

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
        sealed.server.is_empty() && !sealed.followed
    }

    #[inline]
    fn copies(sealed: &Sealed, type_hash: u32) -> bool {
        sealed.server.contains(type_hash)
    }

    #[inline]
    unsafe fn screen(sealed: &Sealed, type_hash: u32, payload: usize, size: usize) {
        for &(hash, screen) in &sealed.screens {
            if hash == type_hash {
                // SAFETY: `payload` is the live server event of `type_hash`,
                // `size` bytes, on its world's thread before the engine
                // moves the streams (this function's contract), all an
                // `EventScreen` asks.
                unsafe {
                    screen(
                        type_hash,
                        core::ptr::with_exposed_provenance_mut(payload),
                        size,
                    );
                };
            }
        }
    }

    fn follow(sealed: &Sealed, type_hash: u32, payload: &[u8]) {
        for &(hash, follow) in &sealed.follows {
            if hash == type_hash {
                follow(type_hash, payload);
            }
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

    unsafe fn screen(_sealed: &Sealed, _type_hash: u32, _payload: usize, _size: usize) {}

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
///
/// # Errors
///
/// [`JoinFeatureError::TooLate`] outside the mod's `init`.
pub(crate) fn register(
    side: Side,
    whose: Whose,
    owner: ModIndex,
    in_init: bool,
    type_hash: u32,
    callback: GameEventFn,
    user: *mut c_void,
) -> Result<(), JoinFeatureError> {
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
/// once the registry is sealed, and fine without keeping it for a client
/// handler on a dedicated server, where no client world steps.
fn keep(
    registry: &Registry<Subscriber>,
    side: Side,
    in_init: bool,
    subscriber: Subscriber,
) -> Result<(), JoinFeatureError> {
    if !in_init {
        Err(JoinFeatureError::TooLate)
    } else if subscriber.whose == Whose::Client && side != Side::CLIENT {
        Ok(())
    } else {
        Ok(registry.add(subscriber)?)
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

/// Seals the subscriptions in the seal phase, so no link sees them open.
pub(crate) fn seal(_loader: &'static Loader) {
    let _sealed = SUBSCRIBERS.seal_with(server_first);
}

/// Links the event move when a mod subscribed; the subscriptions sealed in
/// [`seal`], or here for a test that links alone.
pub(crate) fn link(loader: &'static Loader) {
    let subscribers = SUBSCRIBERS.seal_with(server_first);
    let (server, client) = by_world(subscribers);
    let followers: Vec<(EventFollower, Vec<u32>)> = loader
        .parts
        .iter()
        .filter(|part| part.runs())
        .filter_map(|part| part.game_events)
        .filter_map(|follower| Some((follower, (follower.types)(loader)?)))
        .collect();
    let sealed = Sealed {
        server,
        client,
        ..Sealed::default()
    }
    .with_followers(&followers);
    let count = subscribers.len();
    // A client always links the move, to raise the loader's notices in its
    // own world.
    let idle = ServerScope::idle(&sealed) && loader.side != Side::CLIENT;
    if SEALED.set(sealed).is_err() || idle {
        return;
    }
    let Some(sealed) = SEALED.get() else {
        return;
    };
    // Without the layout the move still links: a client's notices and the
    // freeing of handed lists need no stream.
    let _first = LINKED.set(stream_layout(loader, sealed));
    let target = Target::Pinned(pinned::ecs::EVENT_MERGE);
    if loader.link_feature(&EVENTS_FUSE, target, &MERGE_NEXT, merge, HookFlags::NONE) {
        crate::log!(INFO, "game events: {count} subscriptions");
    } else {
        // shortcut: a move that links but fails to arm later turns off only
        // its own fuse (`Loader::unarmed`); list the followers there too once
        // a build shows that.
        sealed.unfollowed(loader, pinned::ecs::EVENT_MERGE.id(), MOVE_OFF);
    }
}

/// Turns the client's notices, and its own game events when a mod
/// subscribed to them, off by the entry `id` it lacks for `reason`, when
/// the loader cannot tell a hosted server's world from the client's.
pub(crate) fn untold(loader: &Loader, id: &str, reason: &str) {
    if SUBSCRIBERS.any(|subscriber| subscriber.whose == Whose::Client) {
        let _off = loader.fuse_off(&CLIENT_FUSE, id, reason);
    }
    let _off = loader.fuse_off(&NOTICES_FUSE, id, reason);
}

/// Why the followers are off when the event move does not link.
const MOVE_OFF: &str = "the event move is off";

/// The build's event stream layout; without it the delivery is off, listed
/// by the first entry the build lacks, with the followers of `sealed` and,
/// on a client whose mods handle its own events, their delivery too.
fn stream_layout(loader: &Loader, sealed: &Sealed) -> Option<Layout> {
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
            sealed.unfollowed(loader, missing.id(), absent);
            if loader.side == Side::CLIENT && !sealed.client.is_empty() {
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
    let count: usize = unsafe { crate::engine::memory::read(world, layout.stream_count) };
    if streams == 0 {
        return;
    }
    if count > STREAMS {
        // A corrupted count: the array holds fewer streams, so none is read.
        too_many_streams(count);
        return;
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
            unsafe { S::screen(sealed, type_hash, payload, size) };
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
        let data = enshwick_abi::headed!(GameEventData {
            type_hash: event.type_hash,
            size: event.size,
            data: payload.as_ptr(),
        });
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
#[cfg(feature = "test")]
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
        ..Sealed::default()
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
mod tests;
