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
    zone.altar = enshwick_core::Entity(1);
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
        altar: enshwick_core::Entity(4096),
        level: 2,
        reserved: [0; 3],
        grow: 0,
        height: 60,
    }
}

unsafe extern "system" fn blamed(_zone: *mut AltarZone, user: *mut c_void) {
    // SAFETY: the test's own cell, alive for the call.
    let seen = unsafe { &*user.cast::<Cell<Option<(usize, crate::engine::profile::Feature)>>>() };
    seen.set(crate::engine::profile::current_callback());
}

type Link = (ModIndex, AltarZoneFn, *mut c_void);

#[test]
fn filters_chain_on_the_zone_so_far_and_vanilla_gives_no_target() {
    let (none, mod0) = (core::ptr::null_mut(), ModIndex::new(0));
    assert_eq!(chain(vanilla(), core::iter::empty()), None);
    let twice: [Link; 2] = [(mod0, wider, none), (mod0, wider, none)];
    assert_eq!(chain(vanilla(), twice.into_iter()), Some((4, 60)));
}

#[test]
fn a_filter_cannot_move_the_altar_or_shrink_the_zone_below_vanilla() {
    let (none, mod0) = (core::ptr::null_mut(), ModIndex::new(0));
    let filters: [Link; 2] = [(mod0, wider, none), (mod0, broken, none)];
    assert_eq!(chain(vanilla(), filters.into_iter()), None);
    let filters: [Link; 2] = [(mod0, broken, none), (mod0, wider, none)];
    assert_eq!(chain(vanilla(), filters.into_iter()), Some((2, 60)));
}

#[test]
fn a_zone_filter_runs_as_its_owner() {
    let seen: Cell<Option<(usize, crate::engine::profile::Feature)>> = Cell::new(None);
    let filter: Link = (
        ModIndex::new(3),
        blamed,
        (&raw const seen).cast_mut().cast(),
    );
    let _vanilla = chain(vanilla(), core::iter::once(filter));
    assert_eq!(
        seen.get(),
        Some((3, crate::engine::profile::Feature::Altars))
    );
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
    assert!(!seen.merge([], later), "a pass forgets nothing");
    assert_eq!(seen.iter().count(), 2);
    assert!(seen.expire(later));
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
    assert_eq!(altar.entity, enshwick_core::Entity(7));
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

#[test]
fn the_altars_arm_both_systems_or_neither_and_report_why() {
    let loader = crate::start::tests::bare_loader();
    assert!(!link_systems(
        &loader,
        [Some(0x1000), None],
        HookFlags::NONE
    ));
    assert!(ALTARS.is_off());
    let mut chains = loader.chains.relock();
    chains.seal();
    assert!(
        chains.is_empty(),
        "the position pass went with the range pass, so its system is never patched"
    );
    drop(chains);
    let listed = crate::engine::diag::parts_off();
    assert!(
        listed
            .iter()
            .any(|p| p.part == "altars" && p.reason.starts_with(UPGRADE_SYSTEM)),
        "{listed:?}"
    );
}

#[test]
fn a_range_pass_that_writes_nothing_or_panics_puts_the_row_back_and_one_that_writes_keeps_it() {
    use crate::engine::pinned::{altar, balancing, ecs};
    let loader = crate::start::tests::pinning_loader(
        Side::SERVER,
        &[
            ecs::ITERATOR_INIT,
            ecs::ITERATOR_NEXT,
            ecs::MARK_CHANGED,
            ecs::ATTRIBUTE_FIND,
            balancing::ZONE_SIZE,
            ecs::COMPONENT_READ,
        ],
        &[
            (ecs::CONTEXT_ROW, 8),
            (ecs::DESCRIPTOR_ENTITY_RECORDS, 0),
            (ecs::ENTITY_RECORD_SIZE, 0),
            (ecs::CONTEXT_ENTITY_WORLD, 0),
            (ecs::LEVEL_FLAGS, 0),
            (ecs::LEVEL_DATA_OFFSET, 0),
            (ecs::ATTRIBUTE_SLOT, 0),
            (balancing::ZONE_SIZE_VALID, 0),
            (altar::LEVEL_ATTRIBUTE, 0),
        ],
    );
    let engine: &'static Engine = Box::leak(Box::new(Engine::resolve(&loader).unwrap()));
    let mut room = [0u64; 4];
    let ctx = room.as_mut_ptr();
    let pass = |keep: bool, panics: bool| {
        // SAFETY: `room` stands for a live context on this thread, its u32
        // row counter at the table's offset 8, inside its 32 bytes.
        unsafe { engine.set_row(ctx, 3) };
        let _caught = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            // SAFETY: `room` stands for a live context on this thread
            // until the rewind drops at the end of this closure.
            let rewind = unsafe { Rewind::at(engine, ctx) };
            // SAFETY: the row counter of `room`, at offset 8.
            unsafe { engine.set_row(ctx, 7) };
            assert!(!panics, "a range pass that panics part way");
            if keep {
                rewind.keep();
            }
        }));
        // SAFETY: the row counter of `room`, at offset 8.
        unsafe { engine.row(ctx) }
    };
    assert_eq!(pass(true, false), 7, "a pass that wrote keeps its rows");
    assert_eq!(
        pass(false, false),
        3,
        "vanilla runs the tick from its start"
    );
    assert_eq!(pass(true, true), 3, "and so after a pass that panicked");
}

#[test]
fn levels_are_read_after_vanilla_whenever_the_range_pass_writes_nothing() {
    let ran = RefCell::new(Vec::new());
    let run = |sized, wrote| {
        ran.borrow_mut().clear();
        tick(
            sized,
            || {
                ran.borrow_mut().push("write");
                wrote
            },
            || ran.borrow_mut().push("vanilla"),
            || ran.borrow_mut().push("read"),
        );
        ran.borrow().clone()
    };
    assert_eq!(
        run(true, true),
        ["write"],
        "the range pass replaces vanilla"
    );
    assert_eq!(run(true, false), ["write", "vanilla", "read"]);
    assert_eq!(run(false, true), ["vanilla", "read"]);
}
