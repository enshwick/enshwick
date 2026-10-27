//! Model tests of the server pins under loom: a reader that holds a pin
//! never reads the server's memory once its end has freed it.
//!
//! Run with `RUSTFLAGS="--cfg loom" cargo test -p enshwick-loader --lib server_pin::model --release`.

#[cfg(test)]
mod tests {
    use loom::cell::UnsafeCell;
    use loom::sync::Arc;
    use loom::thread;

    use crate::server_pin::ServerCell;

    /// The server's memory: a word the end overwrites when it frees it.
    struct Memory(UnsafeCell<usize>);

    // SAFETY: the model shares the word between the reader and the end
    // only as the pins allow; loom reports any access the pins do not
    // order.
    unsafe impl Sync for Memory {}

    const LIVE: usize = 0x5e;
    const FREED: usize = 0;

    #[test]
    fn a_pinned_read_never_meets_the_freed_server() {
        loom::model(|| {
            let cell = Arc::new(ServerCell::new());
            let memory = Arc::new(Memory(UnsafeCell::new(LIVE)));
            cell.publish(0x1000);
            let reader = {
                let (cell, memory) = (Arc::clone(&cell), Arc::clone(&memory));
                thread::spawn(move || {
                    if let Some(pin) = cell.pin() {
                        assert_eq!(pin.server(), 0x1000);
                        // SAFETY: the pin keeps the end from writing the
                        // word until it drops; loom checks the order.
                        let word = memory.0.with(|word| unsafe { *word });
                        assert_eq!(word, LIVE);
                    }
                })
            };
            let left = cell.retire(|| {
                thread::yield_now();
                true
            });
            assert!(left);
            // SAFETY: every pin taken before the end has dropped (`retire`
            // returned true) and no later pin is taken; loom checks it.
            memory.0.with_mut(|word| unsafe { *word = FREED });
            reader.join().unwrap();
        });
    }

    #[test]
    fn two_readers_and_the_end_agree() {
        loom::model(|| {
            let cell = Arc::new(ServerCell::new());
            let memory = Arc::new(Memory(UnsafeCell::new(LIVE)));
            cell.publish(0x1000);
            let readers: Vec<_> = (0..2)
                .map(|_| {
                    let (cell, memory) = (Arc::clone(&cell), Arc::clone(&memory));
                    thread::spawn(move || {
                        if let Some(_pin) = cell.pin() {
                            // SAFETY: the pin keeps the end from writing
                            // the word until it drops; loom checks it.
                            let word = memory.0.with(|word| unsafe { *word });
                            assert_eq!(word, LIVE);
                        }
                    })
                })
                .collect();
            assert!(cell.retire(|| {
                thread::yield_now();
                true
            }));
            // SAFETY: every pin taken before the end has dropped and no
            // later pin is taken; loom checks it.
            memory.0.with_mut(|word| unsafe { *word = FREED });
            for reader in readers {
                reader.join().unwrap();
            }
        });
    }
}
