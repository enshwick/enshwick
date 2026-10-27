//! Pins on the running server for readers off its game thread: while a pin
//! is held, the server's end waits before its memory goes.
//!
//! One word holds the count of pins and an "ending" bit, and only
//! read-modify-writes change it, so each pin comes either before the end
//! began, and the wait before the free sees it, or after, and fails. The
//! game thread itself reads without a pin: the end runs on it, after its
//! last step.

// The loom model swaps in loom's atomics, which record every load and
// store for the model to permute.
#[cfg(loom)]
use loom::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};
#[cfg(not(loom))]
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};

/// The bit of [`ServerCell::state`] set once the running server's end began.
const ENDING: u32 = 1 << 31;

/// A value on a cache line of its own, so the game thread's loads of the
/// server do not share a line with the pins' read-modify-writes.
#[derive(Debug)]
#[repr(align(64))]
struct Line<T>(T);

/// Where a pin was taken, for the warning of a stalled end.
pub(crate) type Place = &'static &'static str;

/// The running server's address, 0 for none, and the pins on it.
#[derive(Debug)]
pub(crate) struct ServerCell {
    server: Line<AtomicUsize>,
    /// The pins held, with [`ENDING`] once the end began.
    state: Line<AtomicU32>,
    /// The place of the last pin taken.
    last: AtomicPtr<&'static str>,
}

/// The place of a cell no pin was taken from yet.
static NO_PLACE: &str = "no pin yet";

impl ServerCell {
    /// A cell with no server.
    #[cfg(not(loom))]
    pub(crate) const fn new() -> Self {
        Self {
            server: Line(AtomicUsize::new(0)),
            state: Line(AtomicU32::new(ENDING)),
            last: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// A cell with no server.
    #[cfg(loom)]
    pub(crate) fn new() -> Self {
        Self {
            server: Line(AtomicUsize::new(0)),
            state: Line(AtomicU32::new(ENDING)),
            last: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// Makes `server` the running one; on its game thread, after each
    /// step. Only a new server costs more than a load; 0 ends the running
    /// one as [`ServerCell::retire`] does and forgets it.
    pub(crate) fn publish(&self, server: usize) {
        if server == 0 {
            self.retire();
            self.server.0.store(0, Ordering::Release);
            return;
        }
        if self.server.0.load(Ordering::Relaxed) == server {
            return;
        }
        self.server.0.store(server, Ordering::Release);
        self.state.0.fetch_and(!ENDING, Ordering::Release);
    }

    /// The running server, 0 for none, for its game thread, which needs
    /// no pin: still there after [`ServerCell::retire`], for the shutdown
    /// work, and 0 from [`ServerCell::tear_down`] on, before the game frees
    /// it.
    pub(crate) fn current(&self) -> usize {
        self.server.0.load(Ordering::Acquire)
    }

    /// A pin on the running server taken at `place`, from any thread;
    /// `None` while none runs or once its end began.
    pub(crate) fn pin(&self, place: Place) -> Option<ServerPin<'_>> {
        let mut pin = ServerPin {
            cell: self,
            server: 0,
        };
        if self.state.0.fetch_add(1, Ordering::Acquire) & ENDING != 0 {
            return None;
        }
        self.last
            .store(core::ptr::from_ref(place).cast_mut(), Ordering::Relaxed);
        pin.server = self.server.0.load(Ordering::Acquire);
        (pin.server != 0).then_some(pin)
    }

    /// The running server for a read at `place`: as its game thread sees
    /// it when `on_game_thread`, with no pin, since the end and the
    /// teardown run there too; else through a pin. `None` while none runs,
    /// or off the game thread once its end began.
    pub(crate) fn access(&self, place: Place, on_game_thread: bool) -> Option<ServerAccess<'_>> {
        if on_game_thread {
            let server = self.current();
            return (server != 0).then_some(ServerAccess::GameThread(server));
        }
        self.pin(place).map(ServerAccess::Pinned)
    }

    /// Begins the running server's end: no pin is taken from now on. The
    /// game thread still reads it through [`ServerCell::current`].
    pub(crate) fn retire(&self) {
        self.state.0.fetch_or(ENDING, Ordering::AcqRel);
    }

    /// Waits until every pin taken before [`ServerCell::retire`] has
    /// dropped, calling `stalled` between looks with the place of the last
    /// pin taken; on the game thread, before the server's memory goes.
    pub(crate) fn wait_unpinned(&self, mut stalled: impl FnMut(&'static str)) {
        while self.state.0.load(Ordering::Acquire) & !ENDING != 0 {
            stalled(self.last_place());
        }
    }

    /// Ends the running server before its teardown frees it, on its game
    /// thread: no pin from now on, as [`ServerCell::retire`] (again, in
    /// case the shutdown did not), the wait of
    /// [`ServerCell::wait_unpinned`], then no server, even for the game
    /// thread.
    pub(crate) fn tear_down(&self, stalled: impl FnMut(&'static str)) {
        self.retire();
        self.wait_unpinned(stalled);
        self.publish(0);
    }

    /// The place of the last pin taken.
    fn last_place(&self) -> &'static str {
        let last = self.last.load(Ordering::Relaxed);
        // SAFETY: `last` is null or was stored from a `Place`, a reference
        // to a `&'static str` that lives for the program.
        unsafe { last.as_ref() }.copied().unwrap_or(NO_PLACE)
    }
}

/// A pin on the running server: its memory stays while the pin is held.
/// Reads through it are short and take no lock the game thread holds.
#[derive(Debug)]
pub(crate) struct ServerPin<'a> {
    cell: &'a ServerCell,
    server: usize,
}

/// The running server for one read: on its game thread, or through a pin
/// elsewhere. Either way its memory stays until the access drops.
#[derive(Debug)]
pub(crate) enum ServerAccess<'a> {
    /// On the game thread, which alone runs the teardown that frees it.
    GameThread(usize),
    /// Off the game thread, held by a pin.
    Pinned(ServerPin<'a>),
}

impl ServerAccess<'_> {
    /// The server's address, non-zero.
    pub(crate) const fn server(&self) -> usize {
        match self {
            Self::GameThread(server) => *server,
            Self::Pinned(pin) => pin.server,
        }
    }
}

impl Drop for ServerPin<'_> {
    fn drop(&mut self) {
        self.cell.state.0.fetch_sub(1, Ordering::Release);
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

    const HERE: Place = &"a test";

    /// Ends the cell's server and waits for its pins, yielding meanwhile.
    fn end(cell: &ServerCell) {
        cell.retire();
        cell.wait_unpinned(|_| std::thread::yield_now());
    }

    #[test]
    fn a_pin_holds_the_running_server_and_none_without_one() {
        let cell = ServerCell::new();
        assert!(cell.pin(HERE).is_none());
        cell.publish(0x1000);
        let pin = cell.pin(HERE).unwrap();
        assert_eq!(pin.server, 0x1000);
        assert_eq!(cell.current(), 0x1000);
        drop(pin);
        end(&cell);
        assert!(cell.pin(HERE).is_none(), "no pin once the end began");
        assert_eq!(cell.current(), 0x1000, "the game thread still reads it");
        cell.publish(0x1000);
        assert!(cell.pin(HERE).is_none(), "the same server stays ended");
        cell.publish(0);
        assert_eq!(cell.current(), 0);
        assert!(cell.pin(HERE).is_none());
    }

    #[test]
    fn the_game_thread_reads_after_the_end_began_and_others_do_not() {
        let cell = ServerCell::new();
        cell.publish(0x1000);
        cell.retire();
        assert_eq!(cell.access(HERE, true).map(|a| a.server()), Some(0x1000));
        assert!(cell.access(HERE, false).is_none());
        cell.wait_unpinned(|_| panic!("a game-thread access takes no pin"));
        cell.publish(0);
        assert!(cell.access(HERE, true).is_none());
    }

    #[test]
    fn off_the_game_thread_an_access_is_a_pin() {
        let cell = ServerCell::new();
        cell.publish(0x1000);
        let access = cell.access(HERE, false).unwrap();
        assert!(matches!(access, ServerAccess::Pinned(_)));
        assert_eq!(access.server(), 0x1000);
        cell.retire();
        assert_ne!(
            cell.state.0.load(Ordering::Acquire) & !ENDING,
            0,
            "the end sees it"
        );
        drop(access);
        cell.wait_unpinned(|_| panic!("the access dropped"));
    }

    #[test]
    fn the_game_thread_gets_none_after_the_teardown_point() {
        let cell = ServerCell::new();
        cell.publish(0x1000);
        cell.retire();
        assert!(
            cell.access(HERE, true).is_some(),
            "shutdown work still reads"
        );
        cell.tear_down(|_| panic!("no pin to wait for"));
        assert!(cell.access(HERE, true).is_none());
        assert!(cell.access(HERE, false).is_none());
        assert_eq!(cell.current(), 0);
    }

    #[test]
    fn the_teardown_ends_the_server_even_when_the_shutdown_did_not() {
        let cell = ServerCell::new();
        cell.publish(0x1000);
        let pin = cell.pin(HERE).unwrap();
        let mut stalls = 0;
        let mut pin = Some(pin);
        cell.tear_down(|_| {
            stalls += 1;
            assert!(cell.pin(HERE).is_none(), "no pin once the teardown began");
            drop(pin.take());
        });
        assert_eq!(stalls, 1);
        assert!(cell.access(HERE, true).is_none());
    }

    #[test]
    fn a_new_server_takes_pins_again() {
        let cell = ServerCell::new();
        cell.publish(0x1000);
        end(&cell);
        cell.publish(0x2000);
        assert_eq!(cell.pin(HERE).map(|pin| pin.server), Some(0x2000));
    }

    #[test]
    fn publishing_none_ends_the_server() {
        let cell = ServerCell::new();
        cell.publish(0x1000);
        cell.publish(0);
        assert!(cell.pin(HERE).is_none());
        cell.wait_unpinned(|_| panic!("no pin to wait for"));
    }

    #[test]
    fn the_end_waits_for_a_pin_taken_before_it_and_names_its_place() {
        const READER: Place = &"the reader";
        let cell = Arc::new(ServerCell::new());
        cell.publish(0x1000);
        let pinned = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let reader = {
            let (cell, pinned, release) =
                (Arc::clone(&cell), Arc::clone(&pinned), Arc::clone(&release));
            std::thread::spawn(move || {
                let pin = cell.pin(READER).unwrap();
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
        cell.retire();
        let mut waits = 0;
        cell.wait_unpinned(|place| {
            assert_eq!(place, "the reader");
            waits += 1;
            if waits == 100 {
                release.store(true, Ordering::SeqCst);
            }
            std::thread::yield_now();
        });
        assert!(waits >= 100, "the end waited for the pin: {waits}");
        reader.join().unwrap();
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
                            if let Some(pin) = cell.pin(HERE) {
                                assert!(!freed.load(Ordering::SeqCst), "a read after the free");
                                assert_ne!(pin.server, 0);
                            }
                        }
                    })
                })
                .collect();
            end(&cell);
            freed.store(true, Ordering::SeqCst);
            for reader in readers {
                reader.join().unwrap();
            }
        }
    }
}
