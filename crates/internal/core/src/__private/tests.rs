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
