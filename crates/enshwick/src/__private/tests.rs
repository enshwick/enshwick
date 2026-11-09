use super::*;

#[test]
fn each_handle_has_its_own_place_and_the_chunks_double() {
    let places: Vec<_> = (0..7)
        .map(|h| PackLevels::place(ModHandle(h)).unwrap())
        .collect();
    assert_eq!(
        places,
        [(0, 0), (1, 0), (1, 1), (2, 0), (2, 1), (2, 2), (2, 3)]
    );
    assert_eq!(
        PackLevels::place(ModHandle(usize::MAX - 1)),
        Some((63, (1 << 63) - 1))
    );
    assert_eq!(PackLevels::place(ModHandle(usize::MAX)), None);
}

#[test]
fn every_made_word_lists_under_its_own_handle() {
    let levels = PackLevels([const { OnceLock::new() }; PACK_CHUNKS]);
    for h in [2, 5] {
        levels
            .word(ModHandle(h))
            .unwrap()
            .store(h as u64, Ordering::Relaxed);
    }
    let made: Vec<_> = levels
        .made()
        .map(|(h, w)| (h.0, w.load(Ordering::Relaxed)))
        .collect();
    assert_eq!(made, [(1, 0), (2, 2), (3, 0), (4, 0), (5, 5), (6, 0)]);
}

#[test]
fn a_handle_past_the_last_chunk_keeps_no_word_and_makes_nothing() {
    let levels = PackLevels([const { OnceLock::new() }; PACK_CHUNKS]);
    let last = (1 << PACK_CHUNKS) - 2;
    assert!(levels.word(ModHandle(usize::MAX - 1)).is_none());
    assert!(levels.word(ModHandle(last + 1)).is_none());
    assert_eq!(levels.made().count(), 0, "no chunk made for them");
    assert!(levels.word(ModHandle(last)).is_some());
}

/// A site state whose making panics, as a panic in `Box::new` would.
#[cfg(feature = "test")]
#[derive(Debug)]
struct Refused;

#[cfg(feature = "test")]
impl Default for Refused {
    fn default() -> Self {
        panic!("no state")
    }
}

#[cfg(feature = "test")]
#[test]
fn a_pack_s_site_after_a_poisoned_lock_logs_the_lock_and_works() {
    static BROKEN: Refused = Refused;
    static SITE: Once = Once::new();
    let loader = crate::test::FakeHost::new().install();
    // SAFETY: the fake host's table lives for the process.
    unsafe { install_host(loader.api(), ModHandle(9)) };
    let panicked = std::panic::catch_unwind(|| {
        // SAFETY: the fake host's table lives for the process.
        unsafe {
            as_pack(loader.api(), ModHandle(2), || {
                let _state = for_pack(&BROKEN);
            });
        }
    });
    assert!(panicked.is_err());
    // SAFETY: the fake host's table lives for the process.
    let first = unsafe { as_pack(loader.api(), ModHandle(2), || SITE.first()) };
    assert!(first);
    assert!(
        loader
            .sent()
            .iter()
            .any(|(by, _, line)| *by == ModHandle(2) && line.contains("panicked holding")),
        "{:?}",
        loader.sent()
    );
}

/// The site a [`Reentering`] state asks for while it is made.
#[cfg(feature = "test")]
static INNER: Once = Once::new();
/// Whether that inner ask got the shared site back.
#[cfg(feature = "test")]
static INNER_SHARED: AtomicU8 = AtomicU8::new(0);

/// A site state whose making reaches [`for_pack`] again, as a line logged
/// while the states' lock is held does.
#[cfg(feature = "test")]
#[derive(Debug)]
struct Reentering;

#[cfg(feature = "test")]
impl Default for Reentering {
    fn default() -> Self {
        let inner = for_pack(&INNER);
        INNER_SHARED.store(
            u8::from(core::ptr::eq(inner, &raw const INNER)),
            Ordering::Relaxed,
        );
        Self
    }
}

#[cfg(feature = "test")]
#[test]
fn a_site_reached_again_while_the_states_lock_is_held_takes_the_shared_state() {
    static OUTER: Reentering = Reentering;
    let loader = crate::test::FakeHost::new().install();
    // SAFETY: the fake host's table lives for the process.
    unsafe { install_host(loader.api(), ModHandle(9)) };
    // SAFETY: the fake host's table lives for the process.
    let outer = unsafe { as_pack(loader.api(), ModHandle(3), || for_pack(&OUTER)) };
    assert!(
        !core::ptr::eq(outer, &raw const OUTER),
        "the pack's own state"
    );
    assert_eq!(INNER_SHARED.load(Ordering::Relaxed), 1);
    // SAFETY: the fake host's table lives for the process.
    let later = unsafe { as_pack(loader.api(), ModHandle(3), || for_pack(&INNER)) };
    assert!(
        !core::ptr::eq(later, &raw const INNER),
        "the lock is free again"
    );
}
