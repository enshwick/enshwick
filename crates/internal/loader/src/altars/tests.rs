use super::*;

#[test]
fn entries_held_back_from_a_gone_world_never_reach_the_next() {
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
    assert_eq!(altar.entity, 7);
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
