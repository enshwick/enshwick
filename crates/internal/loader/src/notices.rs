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

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::Entity;
use enshwick::abi::Status;

use crate::host::Loader;
use crate::locks::Relock;
use crate::pinned;
use crate::pinned::Symbol;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::EVENT_PUSH.id(),
    pinned::ecs::EVENT_STREAMS.id(),
    pinned::world::ENTITIES.id(),
    pinned::world::TIME.id(),
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

/// One slot's text, in a buffer of its own that never moves: the game may
/// keep a view of it while the notice is on screen. Writing the slot again
/// overwrites the buffer in place, so a kept view never dangles; at worst a
/// notice still showing when its slot comes round again shows the new text,
/// as reusing its tag already does.
struct SlotText {
    /// The text, at most [`MAX_TEXT`] bytes, and a zero after it.
    bytes: [AtomicU8; MAX_TEXT + 1],
    /// One more than the bytes of `bytes` the text takes; 0 while the slot
    /// is unset, so an empty text is still a text.
    len: AtomicUsize,
}

impl SlotText {
    const fn new() -> Self {
        Self {
            bytes: [const { AtomicU8::new(0) }; MAX_TEXT + 1],
            len: AtomicUsize::new(0),
        }
    }

    /// Writes `text`, at most [`MAX_TEXT`] bytes, over the slot's text.
    fn write(&self, text: &str) {
        let text = &text.as_bytes()[..text.len().min(MAX_TEXT)];
        for (byte, &new) in self.bytes.iter().zip(text) {
            byte.store(new, Ordering::Relaxed);
        }
        self.bytes[text.len()].store(0, Ordering::Relaxed);
        self.len.store(text.len() + 1, Ordering::Release);
    }

    /// The slot's text as a view the game may keep; `None` while unset.
    fn view(&self) -> Option<crate::content::Handed> {
        let len = self.len.load(Ordering::Acquire).checked_sub(1)?;
        Some(crate::content::Handed::new(
            self.bytes.as_ptr().cast::<u8>(),
            len,
        ))
    }
}

static SLOT_TEXTS: [SlotText; SLOTS] = [const { SlotText::new() }; SLOTS];
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
            let function = |name: Symbol| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let (push, time) = (
                function(pinned::ecs::EVENT_PUSH)?,
                function(pinned::world::TIME)?,
            );
            Some(Layout {
                // SAFETY: the build table's `ecs.event_push` is the address
                // of the game function with the `PushFn` signature on this
                // build.
                push: unsafe { core::mem::transmute::<usize, PushFn>(push) },
                // SAFETY: the build table's `world.time` is the address of the
                // game function with the `TimeFn` signature on this build.
                time: unsafe { core::mem::transmute::<usize, TimeFn>(time) },
                hash: crate::world::type_hash(info.va())?,
                size: info.size(),
                time_offset: offset("keen::ecs::GameEvent", "timeStamp")?,
                player: offset(name, "playerEntityId")?,
                tag: offset(name, "messageLocaTag")?,
                streams: loader.offset(pinned::ecs::EVENT_STREAMS)?,
                world_entities: loader.offset(pinned::world::ENTITIES)?,
            })
        })
        .as_ref()
}

/// The text of a notice text tag, by its id, while one is set.
pub(crate) fn text(id: u32) -> Option<crate::content::Handed> {
    let slot = TAGS
        .iter()
        .position(|tag| enshwick_gamedata::reflect::fnv1a32(tag) == id)?;
    SLOT_TEXTS[slot].view()
}

/// Queues a notice the server sent for the client's next step.
pub(crate) fn queue(entity: Entity, text: &str) {
    let mut waiting = WAITING.relock();
    if waiting.len() < MAX_WAITING {
        waiting.push((entity.0, text.to_owned()));
    }
}

/// `text` cut to the [`MAX_TEXT`] bytes the game shows, at a character.
fn cut(text: &str) -> &str {
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Writes `text` into the next notice text tag; its GUID.
fn take_slot(text: &str) -> [u8; 16] {
    let index = NEXT_SLOT.fetch_add(1, Ordering::Relaxed) % SLOTS;
    SLOT_TEXTS[index].write(cut(text));
    TAGS[index]
}

/// Raises the waiting notices in the client's own world, on its thread
/// before the engine moves its event streams.
///
/// # Safety
///
/// - `entity_world` is the address of the client's own live entity world.
/// - The call runs on the client's game thread, before the engine moves its
///   event streams, so no system pushes into them during the call.
pub(crate) unsafe fn raise(loader: &Loader, entity_world: usize) {
    let waiting = {
        let mut waiting = WAITING.relock();
        if waiting.is_empty() {
            return;
        }
        std::mem::take(&mut *waiting)
    };
    let Some(layout) = layout(loader) else {
        return;
    };
    // SAFETY: the caller's contract makes `entity_world` the client's live
    // entity world on its game thread; it holds the pointer to its first event
    // stream at the table's `ecs.event_streams`.
    let stream = unsafe { crate::memory::read_ptr::<u8>(entity_world, layout.streams) };
    if stream.is_null() {
        return;
    }
    // SAFETY: the caller's contract makes `entity_world` the client's live
    // entity world on its game thread; it lies inside the World at the
    // table's `world.entities`, so the subtraction gives the World the game's
    // own time function takes.
    let now = unsafe {
        (layout.time)(core::ptr::with_exposed_provenance(
            entity_world - layout.world_entities,
        ))
    };
    for (entity, text) in waiting {
        let tag = take_slot(&text);
        // SAFETY: `stream` is the client's first event stream, non-null by
        // the earlier check; by the caller's contract the call runs on the
        // client's game thread before the engine moves the streams, so no
        // system pushes; `hash` and `size` are the reflected event type's.
        let payload = unsafe { (layout.push)(stream, layout.hash, layout.size, 0) };
        if payload.is_null() {
            return;
        }
        let size = usize::try_from(layout.size).unwrap_or(0);
        let put = |at: usize, bytes: &[u8]| {
            if at + bytes.len() <= size {
                // SAFETY: the push returned a non-null payload (checked
                // earlier) of `size` bytes reserved for this event, which
                // nothing else touches until the engine moves the stream after
                // this call; the span ends inside it, checked before.
                unsafe { enshwick::raw::write_bytes_at(payload, at, bytes) };
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
        .find(|p| p.key == enshwick::PlayerKey::new(key))
        .ok_or(Status::MISSING)?;
    if !crate::net::ready(key) {
        return Err(Status::MISSING);
    }
    // Cut here too: a longer text would take several frames, which the
    // client does not read as a notice.
    crate::net::notice(key, player.entity, cut(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sent_notice_fits_one_frame() {
        let long = "ж".repeat(600);
        let text = cut(&long);
        assert!(text.len() <= MAX_TEXT && text.chars().all(|c| c == 'ж'));
        let mut data = vec![0u8; 6];
        data.extend_from_slice(text.as_bytes());
        assert_eq!(crate::net::control_frames(&data).map(|f| f.len()), Some(1));
    }

    /// Serialises the tests that take slots, which all tests share.
    static SLOTS_IN_TEST: Mutex<()> = Mutex::new(());

    /// The text of the slot with tag id `id`, read as the game would.
    fn read(id: u32) -> Option<String> {
        let slot = TAGS
            .iter()
            .position(|tag| enshwick_gamedata::reflect::fnv1a32(tag) == id)?;
        let slot = &SLOT_TEXTS[slot];
        let len = slot.len.load(Ordering::Acquire).checked_sub(1)?;
        let bytes: Vec<u8> = slot.bytes[..len]
            .iter()
            .map(|b| b.load(Ordering::Relaxed))
            .collect();
        Some(String::from_utf8(bytes).unwrap())
    }

    #[test]
    fn a_slot_takes_a_text_cut_at_a_character_and_its_tag_finds_it() {
        let _alone = SLOTS_IN_TEST.relock();
        let long = "ж".repeat(200);
        let tag = take_slot(&long);
        let text = read(enshwick_gamedata::reflect::fnv1a32(&tag)).unwrap();
        assert!(text.len() <= MAX_TEXT && text.chars().all(|c| c == 'ж'));
        let tag = take_slot("Low fuel");
        assert_eq!(
            read(enshwick_gamedata::reflect::fnv1a32(&tag)).as_deref(),
            Some("Low fuel")
        );
        assert_eq!(super::text(1), None);
        let tag = take_slot("");
        assert_eq!(
            read(enshwick_gamedata::reflect::fnv1a32(&tag)).as_deref(),
            Some(""),
            "an empty text is still the slot's text"
        );
        let ids: std::collections::HashSet<u32> = TAGS
            .iter()
            .map(|t| enshwick_gamedata::reflect::fnv1a32(t))
            .collect();
        assert_eq!(ids.len(), SLOTS);
    }

    #[test]
    fn a_slot_keeps_its_buffer_and_a_new_text_overwrites_it_in_place() {
        let _alone = SLOTS_IN_TEST.relock();
        let id = enshwick_gamedata::reflect::fnv1a32(&take_slot("Kept text"));
        let first = text(id).unwrap();
        for n in 1..SLOTS {
            take_slot(&format!("other {n}"));
        }
        let again = enshwick_gamedata::reflect::fnv1a32(&take_slot("Next"));
        assert_eq!(again, id, "the slots go round");
        let next = text(id).unwrap();
        assert_eq!(next.at(), first.at(), "the buffer stays where it was");
        assert_eq!(read(id).as_deref(), Some("Next"));
        let index = TAGS
            .iter()
            .position(|tag| enshwick_gamedata::reflect::fnv1a32(tag) == id)
            .unwrap();
        assert_eq!(SLOT_TEXTS[index].bytes[4].load(Ordering::Relaxed), 0);
    }
}
