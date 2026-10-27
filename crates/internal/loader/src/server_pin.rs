//! Pins on the running server for readers off its game thread: while a pin
//! is held, the end of a hosted game waits before its memory goes.
//!
//! One word holds the count of pins and an "ending" bit, and only
//! read-modify-writes change it, so each pin comes either before the end,
//! which then waits for it, or after, and fails. The game thread itself
//! reads without a pin: the end runs on it, after its last step.

// The loom model swaps in loom's atomics, which record every load and
// store for the model to permute.
#[cfg(loom)]
use loom::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
#[cfg(not(loom))]
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// The bit of [`ServerCell::state`] set once the running server's end began.
const ENDING: u32 = 1 << 31;

/// The running server's address, 0 for none, and the pins on it.
#[derive(Debug)]
pub(crate) struct ServerCell {
    server: AtomicUsize,
    /// The pins held, with [`ENDING`] once the end began.
    state: AtomicU32,
}

impl ServerCell {
    /// A cell with no server.
    #[cfg(not(loom))]
    pub(crate) const fn new() -> Self {
        Self {
            server: AtomicUsize::new(0),
            state: AtomicU32::new(ENDING),
        }
    }

    /// A cell with no server.
    #[cfg(loom)]
    pub(crate) fn new() -> Self {
        Self {
            server: AtomicUsize::new(0),
            state: AtomicU32::new(ENDING),
        }
    }

    /// Makes `server` the running one; on its game thread, after each
    /// step. Only a new server costs more than a load.
    pub(crate) fn publish(&self, server: usize) {
        if self.server.load(Ordering::Relaxed) == server {
            return;
        }
        self.server.store(server, Ordering::Release);
        if server != 0 {
            self.state.fetch_and(!ENDING, Ordering::Release);
        }
    }

    /// The running server, 0 for none, for its game thread, which needs
    /// no pin.
    pub(crate) fn current(&self) -> usize {
        self.server.load(Ordering::Acquire)
    }

    /// A pin on the running server, from any thread; `None` while none
    /// runs or once its end began.
    pub(crate) fn pin(&self) -> Option<ServerPin<'_>> {
        let mut pin = ServerPin {
            cell: self,
            server: 0,
        };
        if self.state.fetch_add(1, Ordering::Acquire) & ENDING != 0 {
            return None;
        }
        pin.server = self.server.load(Ordering::Acquire);
        (pin.server != 0).then_some(pin)
    }

    /// Ends the running server: no pin is taken from now on, and while
    /// `keep_waiting` says so, waits for the pins taken before to drop.
    /// Whether they all dropped; on its game thread, before the server's
    /// memory goes.
    pub(crate) fn retire(&self, mut keep_waiting: impl FnMut() -> bool) -> bool {
        self.state.fetch_or(ENDING, Ordering::AcqRel);
        self.server.store(0, Ordering::Release);
        loop {
            if self.state.load(Ordering::Acquire) & !ENDING == 0 {
                return true;
            }
            if !keep_waiting() {
                return false;
            }
        }
    }
}

/// A pin on the running server: its memory stays while the pin is held.
/// Reads through it are short and take no lock the game thread holds.
#[derive(Debug)]
pub(crate) struct ServerPin<'a> {
    cell: &'a ServerCell,
    server: usize,
}

impl ServerPin<'_> {
    /// The pinned server's address, non-zero.
    pub(crate) const fn server(&self) -> usize {
        self.server
    }
}

impl Drop for ServerPin<'_> {
    fn drop(&mut self) {
        self.cell.state.fetch_sub(1, Ordering::Release);
    }
}

#[cfg(loom)]
mod model;

#[cfg(not(loom))]
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use super::*;

    #[test]
    fn a_pin_holds_the_running_server_and_none_without_one() {
        let cell = ServerCell::new();
        assert!(cell.pin().is_none());
        cell.publish(0x1000);
        let pin = cell.pin().unwrap();
        assert_eq!(pin.server(), 0x1000);
        assert_eq!(cell.current(), 0x1000);
        drop(pin);
        assert!(cell.retire(|| false));
        assert!(cell.pin().is_none(), "no pin once the end began");
        assert_eq!(cell.current(), 0);
    }

    #[test]
    fn the_end_waits_for_a_pin_taken_before_it() {
        let cell = Arc::new(ServerCell::new());
        cell.publish(0x1000);
        let pinned = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let reader = {
            let (cell, pinned, release) =
                (Arc::clone(&cell), Arc::clone(&pinned), Arc::clone(&release));
            std::thread::spawn(move || {
                let pin = cell.pin().unwrap();
                pinned.store(true, Ordering::SeqCst);
                while !release.load(Ordering::SeqCst) {
                    std::thread::yield_now();
                }
                drop(pin);
            })
        };
        while !pinned.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        let mut waits = 0;
        let left = cell.retire(|| {
            waits += 1;
            if waits == 100 {
                release.store(true, Ordering::SeqCst);
            }
            std::thread::yield_now();
            true
        });
        assert!(left);
        assert!(waits >= 100, "the end waited for the pin: {waits}");
        reader.join().unwrap();
    }

    #[test]
    fn the_end_gives_up_when_told_and_says_so() {
        let cell = ServerCell::new();
        cell.publish(0x1000);
        let pin = cell.pin().unwrap();
        let mut waits = 0;
        assert!(!cell.retire(|| {
            waits += 1;
            waits < 3
        }));
        assert_eq!(waits, 3);
        drop(pin);
        assert!(cell.retire(|| false));
    }

    #[test]
    fn many_readers_never_hold_a_pin_past_the_end() {
        let cell = Arc::new(ServerCell::new());
        let freed = Arc::new(AtomicBool::new(false));
        for round in 1..=50usize {
            cell.publish(round);
            freed.store(false, Ordering::SeqCst);
            let readers: Vec<_> = (0..4)
                .map(|_| {
                    let (cell, freed) = (Arc::clone(&cell), Arc::clone(&freed));
                    std::thread::spawn(move || {
                        for _ in 0..200 {
                            if let Some(pin) = cell.pin() {
                                assert!(!freed.load(Ordering::SeqCst), "a read after the free");
                                assert_ne!(pin.server(), 0);
                            }
                        }
                    })
                })
                .collect();
            assert!(cell.retire(|| {
                std::thread::yield_now();
                true
            }));
            freed.store(true, Ordering::SeqCst);
            for reader in readers {
                reader.join().unwrap();
            }
        }
    }
}
