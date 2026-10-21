//! On-screen notices: a short text the game shows a player as it shows its
//! own "can't do that" messages.
//!
//! The game raises those messages on the client, as a `UiNegativeFeedbackEvent`
//! in the client's own world naming the player and a text tag, which its
//! interface turns into a message on screen. A server cannot raise it for a
//! client, so a notice travels as a mod message: the server sends the text
//! and the player's entity to the client's loader, which writes the text into
//! one of its notice text tags and raises the event in its world at the start
//! of the next step, as the game does. A player whose client lacks Enshwick
//! gets the text in chat instead.

use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::Status;

use crate::host::Loader;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "ecs.event_push",
    "ecs.event_streams",
    "world.entities",
    "world.time",
];

/// The longest notice text, in bytes.
pub(crate) const MAX_TEXT: usize = 255;
/// Text tags the loader cycles through, so a notice still on screen keeps
/// its text while the next one is written.
const SLOTS: usize = 8;
/// Notices waiting for the client's next step at most.
const MAX_WAITING: usize = 16;

/// The GUIDs of the loader's notice text tags.
#[expect(clippy::cast_possible_truncation, reason = "the slot fits a byte")]
const TAGS: [[u8; 16]; SLOTS] = {
    let mut tags = [[0u8; 16]; SLOTS];
    let mut i = 0;
    while i < SLOTS {
        // `e5c8a7d2-6b1f-4f0e-9c3a-0e57<slot>`, a range of the loader's own.
        tags[i] = [
            0xe5, 0xc8, 0xa7, 0xd2, 0x6b, 0x1f, 0x4f, 0x0e, 0x9c, 0x3a, 0x0e, 0x57, 0x4e, 0x0c,
            0x11, i as u8,
        ];
        i += 1;
    }
    tags
};

/// Each slot's text: an interned text, which never moves or changes, so the
/// game may read it whenever it asks, while the slot points at another.
static SLOT_TEXTS: [AtomicPtr<&'static str>; SLOTS] =
    [const { AtomicPtr::new(core::ptr::null_mut()) }; SLOTS];
/// Every notice text shown, kept for the process, so a repeated text takes
/// no more memory.
static INTERNED: Mutex<Option<rustc_hash::FxHashMap<&'static str, &'static &'static str>>> =
    Mutex::new(None);
static NEXT_SLOT: AtomicUsize = AtomicUsize::new(0);
/// Notices waiting for the client's next step: entity and text.
static WAITING: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());
static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

type PushFn =
    unsafe extern "system" fn(stream: *mut u8, hash: u32, size: u32, source: u16) -> *mut u8;
type TimeFn = unsafe extern "system" fn(world: *const u8) -> i64;

/// The event and the game functions, from reflection and the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    push: PushFn,
    time: TimeFn,
    hash: u32,
    size: u32,
    time_offset: usize,
    player: usize,
    tag: usize,
    streams: usize,
    world_entities: usize,
}

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let name = "keen::ecs::UiNegativeFeedbackEvent";
            let reflection = &loader.reflection;
            let info = reflection.type_info(name).ok()?;
            let offset = |ty: &str, field: &str| {
                reflection
                    .field_offset(ty, field)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let function = |name: &str| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let (push, time) = (function("ecs.event_push")?, function("world.time")?);
            Some(Layout {
                // SAFETY: each symbol is the game function with this
                // signature on this build.
                push: unsafe { core::mem::transmute::<usize, PushFn>(push) },
                // SAFETY: as above.
                time: unsafe { core::mem::transmute::<usize, TimeFn>(time) },
                hash: crate::world::type_hash(loader, info.va())?,
                size: info.size(),
                time_offset: offset("keen::ecs::GameEvent", "timeStamp")?,
                player: offset(name, "playerEntityId")?,
                tag: offset(name, "messageLocaTag")?,
                streams: loader.offset("ecs.event_streams")?,
                world_entities: loader.offset("world.entities")?,
            })
        })
        .as_ref()
}

/// The text of a notice text tag, by its id, while one is set.
pub(crate) fn text(id: u32) -> Option<&'static str> {
    let slot = TAGS
        .iter()
        .position(|tag| enshwick_loader_reflect::fnv1a32(tag) == id)?;
    let text = SLOT_TEXTS[slot].load(Ordering::Acquire);
    // SAFETY: a non-null slot points at an interned text, leaked for the
    // process and never written again.
    (!text.is_null()).then(|| unsafe { *text })
}

/// `text`, kept for the process: the same address for the same text.
fn intern(text: &str) -> &'static &'static str {
    let mut interned = INTERNED.lock().unwrap_or_else(PoisonError::into_inner);
    let interned = interned.get_or_insert_with(rustc_hash::FxHashMap::default);
    if let Some(&kept) = interned.get(text) {
        return kept;
    }
    let owned: &'static str = Box::leak(text.into());
    let kept: &'static &'static str = Box::leak(Box::new(owned));
    interned.insert(owned, kept);
    kept
}

/// Queues a notice the server sent for the client's next step.
pub(crate) fn queue(entity: u32, text: &str) {
    let mut waiting = WAITING.lock().unwrap_or_else(PoisonError::into_inner);
    if waiting.len() < MAX_WAITING {
        waiting.push((entity, text.to_owned()));
    }
}

/// Writes `text` into the next notice text tag; its GUID.
fn take_slot(text: &str) -> [u8; 16] {
    let index = NEXT_SLOT.fetch_add(1, Ordering::Relaxed) % SLOTS;
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let kept = intern(&text[..end]);
    SLOT_TEXTS[index].store(core::ptr::from_ref(kept).cast_mut(), Ordering::Release);
    TAGS[index]
}

/// Raises the waiting notices in the client's own world, on its thread
/// before the engine moves its event streams.
///
/// # Safety
///
/// `entity_world` is the client's own entity world, on its thread, before
/// the engine moves its streams.
pub(crate) unsafe fn raise(loader: &Loader, entity_world: usize) {
    let waiting = {
        let mut waiting = WAITING.lock().unwrap_or_else(PoisonError::into_inner);
        if waiting.is_empty() {
            return;
        }
        std::mem::take(&mut *waiting)
    };
    let Some(layout) = layout(loader) else {
        return;
    };
    // SAFETY: the entity world holds its stream array; the first stream is
    // the one the game's own game-thread code pushes into.
    let stream: usize = unsafe { crate::world::read(entity_world, layout.streams) };
    if stream == 0 {
        return;
    }
    // SAFETY: the World holds its time where the game's own function reads
    // it; the entity world lies inside the World.
    let now = unsafe {
        (layout.time)(core::ptr::with_exposed_provenance(
            entity_world - layout.world_entities,
        ))
    };
    for (entity, text) in waiting {
        let tag = take_slot(&text);
        // SAFETY: the client's first stream on its thread before the move,
        // where no system pushes; the size is under 256 as reflection says.
        let payload = unsafe {
            (layout.push)(
                core::ptr::with_exposed_provenance_mut(stream),
                layout.hash,
                layout.size,
                0,
            )
        };
        if payload.is_null() {
            return;
        }
        let size = usize::try_from(layout.size).unwrap_or(0);
        // SAFETY: the game handed out `size` zeroed bytes for the event.
        let out = unsafe { core::slice::from_raw_parts_mut(payload, size) };
        let mut put = |at: usize, bytes: &[u8]| {
            if let Some(slot) = out.get_mut(at..at + bytes.len()) {
                slot.copy_from_slice(bytes);
            }
        };
        put(layout.time_offset, &now.to_le_bytes());
        put(layout.player, &entity.to_le_bytes());
        put(layout.tag, &tag);
    }
}

/// Sends a notice to the client of the player with account key `key`: the
/// text with the player's entity, as a mod message. [`Status::MISSING`] when
/// that client takes no notices, so the caller falls back to chat.
pub(crate) fn send(loader: &Loader, key: u64, text: &str) -> Result<(), Status> {
    let player = crate::world::players(loader)?
        .into_iter()
        .find(|p| p.key == key)
        .ok_or(Status::MISSING)?;
    if !crate::net::ready(key) {
        return Err(Status::MISSING);
    }
    crate::net::notice(key, player.entity, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_takes_a_text_cut_at_a_character_and_its_tag_finds_it() {
        let long = "ж".repeat(200);
        let tag = take_slot(&long);
        let id = enshwick_loader_reflect::fnv1a32(&tag);
        let text = text(id).unwrap();
        assert!(text.len() <= MAX_TEXT && text.chars().all(|c| c == 'ж'));
        let tag = take_slot("Low fuel");
        assert_eq!(
            super::text(enshwick_loader_reflect::fnv1a32(&tag)),
            Some("Low fuel")
        );
        assert_eq!(super::text(1), None);
        let ids: std::collections::HashSet<u32> = TAGS
            .iter()
            .map(|t| enshwick_loader_reflect::fnv1a32(t))
            .collect();
        assert_eq!(ids.len(), SLOTS);
    }

    #[test]
    fn a_text_once_shown_keeps_its_address_while_its_slot_moves_on() {
        let first = text(enshwick_loader_reflect::fnv1a32(&take_slot("Kept text"))).unwrap();
        for n in 0..SLOTS * 3 {
            take_slot(&format!("other {n}"));
        }
        assert_eq!(first, "Kept text");
        let again = text(enshwick_loader_reflect::fnv1a32(&take_slot("Kept text"))).unwrap();
        assert_eq!(again.as_ptr(), first.as_ptr());
    }
}
