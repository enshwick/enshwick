//! The content mods serve, by hash: a fixed table that only grows, read
//! without a lock by the content request on the game's IO threads.
//!
//! The game keeps a served hash for as long as a resource names it, so
//! content is never freed; the table bounds it instead, by entries and by
//! bytes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_base::Relock;

use super::ContentHash;

pub(super) use enshwick_base::__internal::served_content::{MAX_BYTES, MAX_ENTRIES};

/// The table's slots, a power of two with a quarter of them always empty,
/// so a probe meets an empty slot soon.
const SLOTS: usize = 4096;
const _: () = assert!(SLOTS.is_power_of_two() && MAX_ENTRIES <= SLOTS / 4 * 3);

/// Why content was not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Full;

/// One served content: its hash and its bytes.
type Served = (ContentHash, &'static [u8]);
/// A slot of the table, set once.
type Slot = OnceLock<Served>;

/// Served content by hash; each slot is set once, by the one writer.
pub(super) struct Store {
    slots: [Slot; SLOTS],
    /// The entries set, read without the lock to skip an empty table.
    entries: AtomicUsize,
    /// The bytes served; holding it is the one writer's turn.
    bytes: Mutex<usize>,
}

impl Store {
    #[expect(
        clippy::large_stack_arrays,
        reason = "the loader builds its one store in a static's constant initializer, and tests on a test thread's 2 MiB stack"
    )]
    pub(super) const fn new() -> Self {
        Self {
            slots: [const { OnceLock::new() }; SLOTS],
            entries: AtomicUsize::new(0),
            bytes: Mutex::new(0),
        }
    }

    /// Whether nothing is served yet.
    pub(super) fn is_empty(&self) -> bool {
        self.entries.load(Ordering::Acquire) == 0
    }

    /// The content served under `hash`. No lock and no allocation.
    pub(super) fn get(&self, hash: &ContentHash) -> Option<&'static [u8]> {
        for (_, taken) in self.probe(hash) {
            let (at, data) = taken?;
            if at == hash {
                return Some(data);
            }
        }
        None
    }

    /// Serves `data` under `hash`, kept for the process; content already
    /// served under it stays as it is.
    ///
    /// # Errors
    ///
    /// [`Full`] past [`MAX_ENTRIES`] or [`MAX_BYTES`].
    pub(super) fn insert(&self, hash: ContentHash, data: &[u8]) -> Result<(), Full> {
        let mut bytes = self.bytes.relock();
        let mut free = None;
        for (slot, taken) in self.probe(&hash) {
            match taken {
                Some((at, _)) if *at == hash => return Ok(()),
                Some(_) => {}
                None => {
                    free = Some(slot);
                    break;
                }
            }
        }
        let total = bytes.checked_add(data.len()).filter(|&t| t <= MAX_BYTES);
        let room = self.entries.load(Ordering::Relaxed) < MAX_ENTRIES;
        let (Some(slot), Some(total), true) = (free, total, room) else {
            return Err(Full);
        };
        // Kept for the process: the game reads it for as long as a
        // resource names its hash.
        let kept: &'static [u8] = Box::leak(data.into());
        let _set = slot.set((hash, kept));
        *bytes = total;
        self.entries.fetch_add(1, Ordering::Release);
        Ok(())
    }

    /// The slots from `hash`'s own on, each with what it holds, until an
    /// empty one; every slot once at most.
    fn probe(&self, hash: &ContentHash) -> impl Iterator<Item = (&Slot, Option<&Served>)> {
        let start = hash[1] as usize;
        (0..SLOTS).map(move |step| {
            let slot = &self.slots[(start + step) & (SLOTS - 1)];
            (slot, slot.get())
        })
    }
}
