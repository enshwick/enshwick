//! A few words one thread writes and any thread copies whole, without a
//! lock: the writer keeps a sequence count odd while it writes, and a reader
//! keeps a copy only when the count was even and the same before and after.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};

/// Tries a reader spins before it yields to the writer.
const SPINS: u32 = 4;
/// Tries a reader makes before it gives up on a writer stopped mid-write.
const TRIES: u32 = 64;

/// `N` words behind a sequence count, written by one thread at a time.
#[derive(Debug)]
pub(crate) struct Copied<const N: usize> {
    seq: AtomicU32,
    words: [AtomicU64; N],
}

impl<const N: usize> Copied<N> {
    /// All words zero, as never written.
    pub(crate) const fn new() -> Self {
        Self {
            seq: AtomicU32::new(0),
            words: [const { AtomicU64::new(0) }; N],
        }
    }

    /// Stores `words`; a writer takes the odd count only from an even one,
    /// so a second writer waits for the first instead of passing a reader
    /// half of each.
    pub(crate) fn write(&self, words: [u64; N]) {
        let mut seq = self.seq.load(Ordering::Relaxed) & !1;
        while let Err(now) = self.seq.compare_exchange_weak(
            seq,
            seq.wrapping_add(1),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            seq = now & !1;
            core::hint::spin_loop();
        }
        fence(Ordering::Release);
        for (word, value) in self.words.iter().zip(words) {
            word.store(value, Ordering::Relaxed);
        }
        self.seq.store(seq.wrapping_add(2), Ordering::Release);
    }

    /// The last whole write, or `None` while the writer stays mid-write
    /// through every try: a few spins, then yields, so a writer preempted
    /// mid-write gets the time to finish.
    pub(crate) fn load(&self) -> Option<[u64; N]> {
        for attempt in 0..TRIES {
            let before = self.seq.load(Ordering::Acquire);
            if before % 2 == 1 {
                if attempt < SPINS {
                    core::hint::spin_loop();
                } else {
                    std::thread::yield_now();
                }
                continue;
            }
            let words = self.words.each_ref().map(|w| w.load(Ordering::Relaxed));
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == before {
                return Some(words);
            }
        }
        None
    }

    /// Opens a write and leaves it open, as a writer stopped mid-write.
    #[cfg(test)]
    pub(crate) fn begin_write(&self) {
        self.seq.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_comes_back_whole_and_none_while_a_write_stays_open() {
        let copied = Copied::<3>::new();
        assert_eq!(copied.load(), Some([0; 3]), "never written");
        copied.write([1, 2, 3]);
        assert_eq!(copied.load(), Some([1, 2, 3]));
        copied.begin_write();
        assert_eq!(copied.load(), None, "mid-write");
    }

    #[test]
    fn readers_on_other_threads_never_see_half_of_a_write() {
        let copied = std::sync::Arc::new(Copied::<4>::new());
        let writer = {
            let copied = std::sync::Arc::clone(&copied);
            std::thread::spawn(move || {
                for n in 0..20_000_u64 {
                    copied.write([n; 4]);
                }
            })
        };
        while !writer.is_finished() {
            if let Some(words) = copied.load() {
                assert!(words.iter().all(|w| *w == words[0]), "{words:?}");
            }
        }
        writer.join().unwrap();
    }

    #[test]
    fn a_second_writer_waits_until_the_first_has_written() {
        let copied = Copied::<1>::new();
        copied.begin_write();
        std::thread::scope(|scope| {
            let second = scope.spawn(|| copied.write([7]));
            std::thread::sleep(std::time::Duration::from_millis(50));
            assert_eq!(copied.seq.load(Ordering::Relaxed), 1, "the second waits");
            copied.seq.store(2, Ordering::Release);
            assert!(second.join().is_ok());
        });
        assert_eq!(copied.seq.load(Ordering::Relaxed), 4);
        assert_eq!(copied.load(), Some([7]));
    }
}
