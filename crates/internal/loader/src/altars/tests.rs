use super::*;

/// Held by every test that touches the altar state, the targets or the
/// world counter, which the tests of this file share.
static SHARED: Mutex<()> = Mutex::new(());

unsafe extern "system" fn wider(zone: *mut AltarZone, _user: *mut c_void) {
    // SAFETY: the loader's zone, writable for the call.
    let zone = unsafe { &mut *zone };
    zone.grow += 2;
}

unsafe extern "system" fn broken(zone: *mut AltarZone, _user: *mut c_void) {
    // SAFETY: the loader's zone, writable for the call.
    let zone = unsafe { &mut *zone };
    zone.altar = enshwick::Entity(1);
    zone.grow = -5;
    zone.height = 0;
}

unsafe extern "system" fn lists(zone: *mut AltarZone, _user: *mut c_void) {
    let _altars = list();
    // SAFETY: the loader's zone, writable for the call.
    let zone = unsafe { &mut *zone };
    zone.height += 1;
}

fn vanilla() -> AltarZone {
    AltarZone {
        header: Header::of::<AltarZone>(),
        altar: enshwick::Entity(4096),
        level: 2,
        reserved: [0; 3],
        grow: 0,
        height: 60,
    }
}

#[test]
fn filters_chain_on_the_zone_so_far_and_vanilla_gives_no_target() {
    let none = core::ptr::null_mut();
    assert_eq!(chain(vanilla(), core::iter::empty()), None);
    let twice: [(AltarZoneFn, *mut c_void); 2] = [(wider, none), (wider, none)];
    assert_eq!(chain(vanilla(), twice.into_iter()), Some((4, 60)));
}

#[test]
fn a_filter_cannot_move_the_altar_or_shrink_the_zone_below_vanilla() {
    let none = core::ptr::null_mut();
    let filters: [(AltarZoneFn, *mut c_void); 2] = [(wider, none), (broken, none)];
    assert_eq!(chain(vanilla(), filters.into_iter()), None);
    let filters: [(AltarZoneFn, *mut c_void); 2] = [(broken, none), (wider, none)];
    assert_eq!(chain(vanilla(), filters.into_iter()), Some((2, 60)));
}

#[test]
fn a_filter_may_read_the_altars_while_the_zones_are_sized() {
    let _shared = SHARED.relock();
    let now = When {
        at: Instant::now(),
        step: 0,
    };
    {
        let mut state = STATE.relock();
        let placed = Placed {
            position: [0; 3],
            orientation: [0.0, 0.0, 0.0, 1.0],
        };
        let _ = state.placed.merge([(4096, placed)], now);
        let mut sizes = Sizes::new();
        sizes.push([4.0, 30.0, 4.0]);
        let _ = state.levels.merge([(4096, sizes)], now);
    }
    let sizer = Sizer::new(ModIndex::new(0), lists, core::ptr::null_mut());
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        size_zones_with(&[&sizer]);
        let _sent = done.send(());
    });
    assert!(
        finished.recv_timeout(Duration::from_secs(10)).is_ok(),
        "sizing deadlocked"
    );
    assert!(
        TARGETS
            .load()
            .as_ref()
            .is_some_and(|t| t.contains_key(&4096))
    );
    forget();
}

#[test]
fn entries_held_back_from_a_gone_world_never_reach_the_next() {
    let _shared = SHARED.relock();
    drop_stale_buffers();
    BUILT.with_borrow_mut(|built| built.push((7, [1.0; 3])));
    drop_stale_buffers();
    assert_eq!(BUILT.with_borrow(Vec::len), 1);
    forget();
    drop_stale_buffers();
    assert!(BUILT.with_borrow(Vec::is_empty));
}

#[test]
fn seen_merges_batches_and_reports_changes() {
    let now = When {
        at: Instant::now(),
        step: 0,
    };
    let later = When {
        at: now.at + FORGET_AFTER,
        step: FORGET_STEPS,
    };
    let mut seen: Seen<u8> = Seen::new();
    assert!(seen.merge([(1, 10), (2, 20)], now));
    assert!(!seen.merge([(1, 10)], now));
    assert!(seen.merge([(2, 21)], now));
    assert_eq!(seen.get(2), Some(&21));
    assert_eq!(seen.iter().count(), 2);
    assert!(seen.merge([], later));
    assert_eq!(seen.iter().count(), 0);
}

#[test]
fn a_stalled_server_keeps_its_altars_and_the_last_one_goes() {
    let start = When {
        at: Instant::now(),
        step: 100,
    };
    let mut seen: Seen<u8> = Seen::new();
    assert!(seen.merge([(1, 10), (2, 20)], start));
    let stalled = When {
        at: start.at + FORGET_AFTER * 30,
        step: start.step + 1,
    };
    assert!(
        !seen.merge([(1, 10)], stalled),
        "a long step forgets nothing"
    );
    assert!(!seen.expire(stalled));
    let fast = When {
        at: start.at,
        step: start.step + FORGET_STEPS * 10,
    };
    assert!(!seen.expire(fast), "fast steps alone forget nothing");
    let gone = When {
        at: stalled.at + FORGET_AFTER,
        step: stalled.step + FORGET_STEPS,
    };
    assert!(seen.expire(gone), "no pass sees them any more");
    assert_eq!(seen.iter().count(), 0);
}

#[test]
fn an_upright_altar_reports_its_zone_and_levels() {
    let placed = Placed {
        position: [
            3636_i64 << 32 | 3 << 30,
            860_i64 << 32 | 1 << 31,
            1339_i64 << 32 | 3 << 30,
        ],
        orientation: [0.0, 0.0, 0.0, 1.0],
    };
    let altar = info(
        7,
        &placed,
        Some(&[20.0; 3]),
        Some(&[[40.0; 3]].into_iter().collect()),
    );
    assert_eq!(altar.entity, enshwick::Entity(7));
    assert_eq!(altar.level, 1);
    assert_eq!(altar.tilted, 0);
    assert_eq!(
        altar.zone,
        Bounds {
            min: [7234, 1681, 2640],
            max: [7314, 1761, 2720]
        }
    );
    assert_eq!(altar.level_count, 1);
    assert_eq!(altar.levels[0], altar.zone);
}

#[test]
fn a_tilted_altar_has_no_zone() {
    let placed = Placed {
        position: [0; 3],
        orientation: [0.3, 0.0, 0.0, 0.95],
    };
    let altar = info(7, &placed, Some(&[20.0; 3]), None);
    assert_eq!(altar.tilted, 1);
    assert_eq!(altar.zone, Bounds::default());
}
