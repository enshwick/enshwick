//! A command the interface frame hands to a detour of the game's own code,
//! without a lock: one word holds the command's id and its stage, and the
//! detour's cheap check is one load of it.

use std::sync::atomic::{AtomicU64, Ordering};

/// The stage of a word whose payload the interface frame still fills.
const FILLING: u8 = u8::MAX;

/// One command at a time, from the interface frame to one detour.
#[derive(Debug)]
pub(crate) struct Handoff {
    /// `id << 8 | stage`, 0 while nothing is handed.
    word: AtomicU64,
}

/// The word of command `id` at `stage`.
const fn pack(id: u64, stage: u8) -> u64 {
    id << 8 | stage as u64
}

impl Handoff {
    pub(crate) const fn new() -> Self {
        Self {
            word: AtomicU64::new(0),
        }
    }

    /// Hands command `id` over at `stage` while nothing is handed: `fill`
    /// writes its payload first, which the detour reads only after it sees
    /// the stage. Whether it was handed.
    pub(crate) fn arm(&self, id: u64, stage: u8, fill: impl FnOnce()) -> bool {
        debug_assert!(stage != 0 && stage != FILLING, "a handed stage");
        if self
            .word
            .compare_exchange(0, pack(id, FILLING), Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return false;
        }
        fill();
        self.word.store(pack(id, stage), Ordering::Release);
        true
    }

    /// The handed command and its stage; `None` while nothing is handed or
    /// its payload is still being filled.
    #[inline]
    pub(crate) fn current(&self) -> Option<(u64, u8)> {
        let word = self.word.load(Ordering::Acquire);
        let stage = (word & 0xff) as u8;
        (word != 0 && stage != FILLING).then_some((word >> 8, stage))
    }

    /// Moves command `id` from stage `from` to `to`; whether it stood at
    /// `from`, so one of a detour and the frame wins a race.
    pub(crate) fn advance(&self, id: u64, from: u8, to: u8) -> bool {
        self.word
            .compare_exchange(
                pack(id, from),
                pack(id, to),
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    /// Command `id`'s stage; `None` while another or none is handed.
    pub(crate) fn stage(&self, id: u64) -> Option<u8> {
        self.current()
            .and_then(|(handed, stage)| (handed == id).then_some(stage))
    }

    /// Takes command `id` back, whatever its stage, so the next command can
    /// be handed; the stage it had, `None` when it was not handed.
    pub(crate) fn release(&self, id: u64) -> Option<u8> {
        let mut word = self.word.load(Ordering::Acquire);
        loop {
            let stage = (word & 0xff) as u8;
            if word == 0 || word >> 8 != id || stage == FILLING {
                return None;
            }
            match self
                .word
                .compare_exchange(word, 0, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return Some(stage),
                Err(now) => word = now,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_command_waits_until_the_first_is_released() {
        let handoff = Handoff::new();
        assert_eq!(handoff.current(), None);
        let mut filled = false;
        assert!(handoff.arm(7, 1, || filled = true));
        assert!(filled);
        assert_eq!(handoff.current(), Some((7, 1)));
        assert!(!handoff.arm(8, 1, || unreachable!("a busy handoff fills nothing")));
        assert_eq!(handoff.release(8), None, "another command's release");
        assert_eq!(handoff.release(7), Some(1));
        assert!(handoff.arm(8, 2, || {}));
        assert_eq!(handoff.stage(8), Some(2));
        assert_eq!(handoff.stage(7), None);
    }

    #[test]
    fn only_one_of_two_advances_from_the_same_stage_wins() {
        let handoff = Handoff::new();
        assert!(handoff.arm(3, 1, || {}));
        assert!(handoff.advance(3, 1, 2));
        assert!(!handoff.advance(3, 1, 4), "the stage moved on");
        assert!(!handoff.advance(9, 2, 4), "another command");
        assert_eq!(handoff.stage(3), Some(2));
    }
}
