//! Model tests of the server pins under loom: a reader that holds a pin
//! never reads the server's memory once its end has freed it, and a pin
//! taken across a handover finds the new server or none.
//!
//! Run with `RUSTFLAGS="--cfg loom" cargo test -p enshwick-loader --lib server_pin::model --release`.

#[cfg(test)]
mod tests {
    use loom::cell::UnsafeCell;
    use loom::sync::Arc;
    use loom::thread;

    use crate::server_pin::{Place, ServerCell};

    /// A server's memory: a word its end overwrites when it frees it.
    struct Memory(UnsafeCell<usize>);

    // SAFETY: the model shares the word between the readers and the end
    // only as the pins allow; loom reports any access the pins do not
    // order.
    unsafe impl Sync for Memory {}

    const LIVE: usize = 0x5e;
    const FREED: usize = 0;
    const A: usize = 0x1000;
    const B: usize = 0x2000;
    const READER: Place = &"the model's reader";

    fn memory() -> Arc<Memory> {
        Arc::new(Memory(UnsafeCell::new(LIVE)))
    }

    /// A reader that, when it gets a pin, reads the memory of the server
    /// it pinned and finds it live.
    fn reader(
        cell: &Arc<ServerCell>,
        a: &Arc<Memory>,
        b: Option<&Arc<Memory>>,
    ) -> thread::JoinHandle<()> {
        let (cell, a, b) = (Arc::clone(cell), Arc::clone(a), b.cloned());
        thread::spawn(move || {
            if let Some(pin) = cell.pin(READER) {
                let memory = match pin.server {
                    A => &a,
                    other => {
                        assert_eq!(other, B);
                        b.as_ref().unwrap()
                    }
                };
                // SAFETY: the pin keeps the end from writing the word until
                // it drops; loom checks the order.
                let word = memory.0.with(|word| unsafe { *word });
                assert_eq!(word, LIVE);
            }
        })
    }

    /// The server's end on the game thread: no new pins, the wait, then
    /// the free.
    fn end(cell: &ServerCell, memory: &Memory) {
        cell.retire();
        cell.wait_unpinned(|_| thread::yield_now());
        // SAFETY: every pin taken before the end has dropped
        // (`wait_unpinned` returned) and no later pin is taken; loom
        // checks it.
        memory.0.with_mut(|word| unsafe { *word = FREED });
    }

    #[test]
    fn a_pinned_read_never_meets_the_freed_server() {
        loom::model(|| {
            let cell = Arc::new(ServerCell::new());
            let a = memory();
            cell.publish(A);
            let reader = reader(&cell, &a, None);
            end(&cell, &a);
            reader.join().unwrap();
        });
    }

    #[test]
    fn two_readers_and_the_end_agree() {
        loom::model(|| {
            let cell = Arc::new(ServerCell::new());
            let a = memory();
            cell.publish(A);
            let readers = [reader(&cell, &a, None), reader(&cell, &a, None)];
            end(&cell, &a);
            for reader in readers {
                reader.join().unwrap();
            }
        });
    }

    #[test]
    fn a_pin_across_a_handover_finds_a_live_server() {
        loom::model(|| {
            let cell = Arc::new(ServerCell::new());
            let (a, b) = (memory(), memory());
            cell.publish(A);
            let reader = reader(&cell, &a, Some(&b));
            end(&cell, &a);
            cell.publish(B);
            reader.join().unwrap();
        });
    }

    #[test]
    fn a_pin_racing_a_publish_finds_the_server_or_none() {
        loom::model(|| {
            let cell = Arc::new(ServerCell::new());
            let b = memory();
            let reader = {
                let (cell, b) = (Arc::clone(&cell), Arc::clone(&b));
                thread::spawn(move || {
                    if let Some(pin) = cell.pin(READER) {
                        assert_eq!(pin.server, B);
                        // SAFETY: the pin keeps the end from writing the
                        // word until it drops; loom checks the order.
                        let word = b.0.with(|word| unsafe { *word });
                        assert_eq!(word, LIVE);
                    }
                })
            };
            cell.publish(B);
            end(&cell, &b);
            reader.join().unwrap();
        });
    }
}
