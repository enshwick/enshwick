//! What the loader keeps of one mod's sounds: a slot per sound, whose state
//! any thread reads, and the queue of starts and changes the client's next
//! frame runs.
//!
//! A sound's id is its slot plus one, with the slot's generation in the
//! high half: a slot is reused only once its sound ended, under the next
//! generation, so an old id never names the new sound.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crossbeam_queue::ArrayQueue;
use enshwick_core::abi::{SoundInfo, SoundPlay, SoundSteer};

/// The most sounds one mod keeps at once, queued or playing.
pub(crate) const MAX_SOUNDS: usize = 256;
/// The most starts and changes one mod queues for one frame.
pub(crate) const MAX_QUEUED: usize = 256;

/// A sound slot's place in its mod's table and the generation it was taken
/// under, as an id names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pub(crate) index: usize,
    pub(crate) generation: u32,
}

impl Slot {
    /// The slot an id names; `None` for an id no slot could have given.
    pub(crate) fn of(id: u64) -> Option<Self> {
        let index = usize::try_from(id & u64::from(u32::MAX))
            .ok()?
            .checked_sub(1)?;
        let generation = u32::try_from(id >> 32).ok()?;
        (index < MAX_SOUNDS && generation != 0).then_some(Self { index, generation })
    }

    /// The id a mod gets for this slot.
    pub(crate) fn id(self) -> u64 {
        u64::from(self.generation) << 32 | (self.index as u64 + 1)
    }
}

/// What the next frame does for a mod.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Op {
    /// Starts the sound of `slot`.
    Start(Slot, SoundPlay),
    /// Changes the sound of `slot`, if it still plays.
    Steer(Slot, SoundSteer),
    /// Stops every sound of the mod over the fade, in milliseconds.
    StopAll(u32),
}

/// Why a start or a change is not queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum QueueError {
    /// The mod reached its starts for this frame, its kept sounds or its
    /// queue.
    #[error("the mod's sounds or starts for this frame are full")]
    Full,
}

/// A slot's state, generation, and the reason a failed one failed, in one
/// word: generation in bits 32 to 63, reason in 8 to 15, state in 0 to 7.
/// A word of generation 0 is a slot never taken.
fn word(generation: u32, state: u32, reason: u32) -> u64 {
    u64::from(generation) << 32 | u64::from(reason & 0xff) << 8 | u64::from(state & 0xff)
}

/// The generation, state and reason a word holds.
fn unpack(word: u64) -> (u32, u32, u32) {
    (
        (word >> 32) as u32,
        (word & 0xff) as u32,
        ((word >> 8) & 0xff) as u32,
    )
}

/// Whether a slot in `word` may take a new sound: never taken, or its sound
/// ended or failed.
fn free(word: u64) -> bool {
    let (generation, state, _) = unpack(word);
    generation == 0 || matches!(state, SoundInfo::ENDED | SoundInfo::FAILED)
}

/// One mod's sounds.
pub(crate) struct ModSounds {
    slots: Box<[AtomicU64]>,
    queue: ArrayQueue<Op>,
    /// The starts queued since the last frame ran the queue.
    starts: AtomicU32,
}

impl ModSounds {
    /// No sound kept or queued.
    pub(crate) fn new() -> Self {
        Self {
            slots: (0..MAX_SOUNDS).map(|_| AtomicU64::new(0)).collect(),
            queue: ArrayQueue::new(MAX_QUEUED),
            starts: AtomicU32::new(0),
        }
    }

    /// Takes a slot for `play` and queues its start; the slot.
    ///
    /// # Errors
    ///
    /// [`QueueError::Full`] past [`SoundPlay::MAX_STARTS`] starts waiting
    /// for one frame, with every slot taken or the queue full.
    pub(crate) fn start(&self, play: SoundPlay) -> Result<Slot, QueueError> {
        if self.starts.fetch_add(1, Ordering::AcqRel) >= SoundPlay::MAX_STARTS {
            self.starts.fetch_sub(1, Ordering::AcqRel);
            return Err(QueueError::Full);
        }
        let Some(slot) = self.take() else {
            self.starts.fetch_sub(1, Ordering::AcqRel);
            return Err(QueueError::Full);
        };
        if self.queue.push(Op::Start(slot, play)).is_err() {
            self.starts.fetch_sub(1, Ordering::AcqRel);
            self.settle(slot, SoundInfo::ENDED, 0);
            return Err(QueueError::Full);
        }
        Ok(slot)
    }

    /// Queues `op`, a change.
    ///
    /// # Errors
    ///
    /// [`QueueError::Full`] while the queue is full.
    pub(crate) fn queue(&self, op: Op) -> Result<(), QueueError> {
        self.queue.push(op).map_err(|_| QueueError::Full)
    }

    /// The next start or change, oldest first; a start taken frees its
    /// place among the starts waiting.
    pub(crate) fn next(&self) -> Option<Op> {
        let op = self.queue.pop();
        if let Some(Op::Start(..)) = op {
            self.starts.fetch_sub(1, Ordering::AcqRel);
        }
        op
    }

    /// The starts and changes queued when a frame asks, oldest first; those
    /// queued meanwhile wait for the next frame, so a mod that keeps
    /// queueing cannot hold the frame.
    pub(crate) fn batch(&self) -> impl Iterator<Item = Op> + '_ {
        (0..self.queue.len()).map_while(|_| self.next())
    }

    /// Takes the first free slot under its next generation, pending.
    fn take(&self) -> Option<Slot> {
        for (index, cell) in self.slots.iter().enumerate() {
            let now = cell.load(Ordering::Acquire);
            if !free(now) {
                continue;
            }
            let generation = match unpack(now).0.wrapping_add(1) {
                0 => 1,
                next => next,
            };
            let taken = word(generation, SoundInfo::PENDING, 0);
            if cell
                .compare_exchange(now, taken, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(Slot { index, generation });
            }
        }
        None
    }

    /// Sets `slot`'s state and reason, while it still holds its generation;
    /// whether it did.
    pub(crate) fn settle(&self, slot: Slot, state: u32, reason: u32) -> bool {
        let Some(cell) = self.slots.get(slot.index) else {
            return false;
        };
        let now = cell.load(Ordering::Acquire);
        unpack(now).0 == slot.generation
            && cell
                .compare_exchange(
                    now,
                    word(slot.generation, state, reason),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
    }

    /// Where the sound `id` stands: ended for an id of an older sound or of
    /// none.
    pub(crate) fn info(&self, id: u64) -> SoundInfo {
        let ended = enshwick_abi::headed!(SoundInfo {
            state: SoundInfo::ENDED,
            ..SoundInfo::EMPTY
        });
        let Some(slot) = Slot::of(id) else {
            return ended;
        };
        let (generation, state, reason) = unpack(self.slots[slot.index].load(Ordering::Acquire));
        if generation == slot.generation {
            enshwick_abi::headed!(SoundInfo {
                state,
                reason,
                ..SoundInfo::EMPTY
            })
        } else {
            ended
        }
    }

    /// Whether `slot` still holds the sound it was taken for and it has not
    /// ended.
    pub(crate) fn open(&self, slot: Slot) -> bool {
        let (generation, state, _) = unpack(self.slots[slot.index].load(Ordering::Acquire));
        generation == slot.generation && matches!(state, SoundInfo::PENDING | SoundInfo::PLAYING)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_names_its_slot_and_generation_and_no_other() {
        let slot = Slot {
            index: 7,
            generation: 3,
        };
        assert_eq!(Slot::of(slot.id()), Some(slot));
        assert_eq!(Slot::of(0), None);
        assert_eq!(Slot::of((MAX_SOUNDS as u64 + 1) | (1 << 32)), None);
        assert_eq!(Slot::of(1), None, "generation 0 is a slot never taken");
    }

    #[test]
    fn the_sixty_fifth_start_of_a_frame_is_refused_until_the_frame_runs() {
        let sounds = ModSounds::new();
        for _ in 0..SoundPlay::MAX_STARTS {
            assert!(sounds.start(SoundPlay::EMPTY).is_ok());
        }
        assert_eq!(sounds.start(SoundPlay::EMPTY), Err(QueueError::Full));
        while sounds.next().is_some() {}
        assert!(sounds.start(SoundPlay::EMPTY).is_ok());
    }

    #[test]
    fn a_stale_id_reads_ended_and_never_names_the_slot_s_next_sound() {
        let sounds = ModSounds::new();
        let first = sounds.start(SoundPlay::EMPTY).unwrap();
        assert_eq!(sounds.info(first.id()).state, SoundInfo::PENDING);
        assert!(sounds.settle(first, SoundInfo::ENDED, 0));
        let second = sounds.start(SoundPlay::EMPTY).unwrap();
        assert_eq!(second.index, first.index);
        assert_ne!(second.id(), first.id());
        assert_eq!(sounds.info(first.id()).state, SoundInfo::ENDED);
        assert!(!sounds.settle(first, SoundInfo::PLAYING, 0));
        assert_eq!(sounds.info(second.id()).state, SoundInfo::PENDING);
    }

    #[test]
    fn a_failed_sound_keeps_its_reason() {
        let sounds = ModSounds::new();
        let slot = sounds.start(SoundPlay::EMPTY).unwrap();
        assert!(sounds.settle(slot, SoundInfo::FAILED, SoundInfo::NO_SCENE));
        let info = sounds.info(slot.id());
        assert_eq!(
            (info.state, info.reason),
            (SoundInfo::FAILED, SoundInfo::NO_SCENE)
        );
        assert!(!sounds.open(slot));
    }

    #[test]
    fn every_slot_taken_refuses_the_next_start() {
        let sounds = ModSounds::new();
        for _ in 0..MAX_SOUNDS {
            let _taken = sounds.start(SoundPlay::EMPTY);
            while sounds.next().is_some() {}
        }
        assert_eq!(sounds.start(SoundPlay::EMPTY), Err(QueueError::Full));
    }
}
