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
//! game's server; only the server's entity world is followed.

use core::cell::RefCell;
use core::ffi::c_void;
use std::sync::atomic::Ordering;
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

/// A mod's handler for one event type, or for all with hash 0.
#[derive(Debug, Clone, Copy)]
struct Subscriber {
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
    every: bool,
    /// The equipment event, when stats follow it.
    equipment: Option<u32>,
}

impl Sealed {
    fn wants(&self, type_hash: u32) -> bool {
        self.every || self.wanted.contains(&type_hash) || self.equipment == Some(type_hash)
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
fn take_batch() -> Batch {
    BATCH.take()
}

/// Puts `batch` back into this thread's cell, after the events a callback
/// copied meanwhile, keeping its buffers when there were none.
fn give_back(mut batch: Batch) {
    BATCH.with_borrow_mut(|held| {
        if held.events.is_empty() && held.bytes.is_empty() {
            core::mem::swap(held, &mut batch);
        } else {
            let newer = core::mem::take(held);
            batch.append(&newer);
            *held = batch;
        }
    });
}

static PENDING: Mutex<Vec<Subscriber>> = Mutex::new(Vec::new());
static SEALED: OnceLock<Sealed> = OnceLock::new();
/// The loader, and the build's stream layout when it has one.
static LOADED: OnceLock<(&'static Loader, Option<Layout>)> = OnceLock::new();
/// The event move's link, off only when this build cannot link it or its
/// own checks panicked: the features it runs have fuses of their own, so
/// one of them failing leaves the others running.
static EVENTS_FUSE: Fuse = Fuse::new("game events");
/// The copy and delivery of the server's events to the mods.
static DELIVERY_FUSE: Fuse = Fuse::new("game event delivery");
/// The client's notices from its server, off once raising one panicked.
static NOTICES_FUSE: Fuse = Fuse::new("server notices");
/// The switch for freeing what the loader handed the game.
static RETIRE_FUSE: Fuse = Fuse::new("freeing handed lists");

thread_local! {
    static BATCH: RefCell<Batch> = RefCell::new(Batch::default());
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
        wanted: subscribers.iter().map(|s| s.type_hash).collect(),
        every: subscribers.iter().any(|s| s.type_hash == 0),
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
    /// The engine's move of the event streams at the table's
    /// `ecs.event_merge`, on the thread that steps `entity_world`: copies the
    /// server's wanted events before the move and hands them to the mods
    /// after it, raises a client's notices, and frees what the game was
    /// handed, each under its own fuse.
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
        let ours = layout.filter(|_| server);
        if let Some(layout) = &ours {
            onward.run(&DELIVERY_FUSE, "the event copy", |_| {
                let _quiet = crate::engine::world::Quiet::enter();
                // SAFETY: `entity_world` is the running server's entity world
                // and this is its game thread (checked by `is_server`); the
                // game passed it to `ecs.event_merge`, which has not moved its
                // streams yet.
                unsafe { copy(layout, entity_world) };
            });
        } else if client_own {
            onward.run(&NOTICES_FUSE, "the notice raise", |_| {
                // SAFETY: on a client, `entity_world` is non-null and not the
                // hosted server's (`client_own`), so the client's own live
                // entity world, which the game passed to `ecs.event_merge` on
                // its game thread before moving its streams.
                unsafe { crate::base::notices::raise(loader, entity_world.expose_provenance()) };
            });
        }
        onward.call();
        // The process's own world: the server's on a dedicated server, the
        // client's own on a client, hosting or not.
        if client_own || (loader.side != Side::CLIENT && server) {
            onward.run(&RETIRE_FUSE, "freeing what the game was handed", |_| {
                crate::engine::retire::step();
            });
        }
        if ours.is_some() {
            onward.run(&DELIVERY_FUSE, "the event delivery", |_| {
                let _quiet = crate::engine::world::Quiet::enter();
                deliver(loader);
            });
        }
    }
}

/// Whether `entity_world` is the running server's, moved on its game thread.
fn is_server(entity_world: *mut u8) -> bool {
    let server = crate::engine::world::server_world();
    server != 0 && entity_world.addr() as u64 == server && crate::engine::world::reader().is_ok()
}

/// Copies every wanted event of the server's streams into this thread's
/// batch.
///
/// # Safety
///
/// - `entity_world` is the running server's live entity world.
/// - Called on the server's game thread before the engine moves its event
///   streams, so no system writes them during the call.
unsafe fn copy(layout: &Layout, entity_world: *mut u8) {
    let Some(sealed) = SEALED.get() else {
        return;
    };
    let world = entity_world.expose_provenance();
    // SAFETY: `world` is the server's live entity world on its game thread
    // (the caller's contract); the table's `ecs.event_streams` offset holds
    // its stream array pointer.
    let streams: usize = unsafe { crate::engine::memory::read(world, layout.streams) };
    // SAFETY: `world` is the server's live entity world on its game thread
    // (the caller's contract); the table's `ecs.event_stream_count` offset
    // holds its stream count.
    let count: usize = unsafe { crate::engine::memory::read(world, layout.stream_count) };
    if streams == 0 {
        return;
    }
    let mut batch = take_batch();
    for index in 0..count {
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
            // SAFETY: the payload is the live event of `type_hash` after the
            // record's header, `size` bytes inside the bytes used (the `> used`
            // check breaks first), on the game thread before the engine moves
            // the streams (the caller's contract).
            unsafe {
                crate::actions::vetoes::screen(
                    type_hash,
                    core::ptr::with_exposed_provenance_mut(record + layout.record_header),
                    size,
                );
            };
            // Item notices are after the fact: they go out with the batch,
            // after the merge, so a filter may change the world.
            if sealed.wants(type_hash) || crate::inventory::item_actions::hears(type_hash) {
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
    give_back(batch);
}

/// Hands this thread's batch to the subscribers still on, and empties it.
fn deliver(loader: &Loader) {
    let Some(sealed) = SEALED.get() else {
        return;
    };
    let mut batch = take_batch();
    for event in &batch.events {
        if sealed.equipment == Some(event.type_hash) {
            crate::inventory::stats::equipment_changed();
        }
        if let Some(payload) = usize::try_from(event.size)
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
    give_back(batch);
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut first = take_batch();
        copied(1, b"ab", &mut first);
        BATCH.with_borrow_mut(|held| copied(2, b"cde", held));
        give_back(first);
        let back = take_batch();
        assert_eq!(payloads(&back), [(1, b"ab".to_vec()), (2, b"cde".to_vec())]);
        give_back(Batch::default());
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
