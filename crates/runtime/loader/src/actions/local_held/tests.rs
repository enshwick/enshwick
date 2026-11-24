//! Tests of the held controls, the host reads and the walk to the local player.

use super::*;
use crate::engine::ecs::double::Query;

#[test]
fn the_walk_finds_the_local_row_after_other_players_from_where_the_system_left_the_counter_and_puts_it_back()
 {
    // The check's word holds the local player's id at byte 4; a word
    // whose id is 0 is a row before any local player was named.
    let local = Box::new([0u32, 9]);
    let unnamed = Box::new([0u32, 0]);
    let word = |id: &[u32; 2]| id.as_ptr().expose_provenance() as u64;
    let rows = vec![
        (0, vec![0, word(&unnamed)]),
        (5, vec![0, word(&local)]),
        (9, vec![0, word(&local)]),
        (12, vec![0, word(&local)]),
        (14, vec![0, 0]),
    ];
    let at_end = 5;
    let mut double = Query::new(rows, at_end);
    let walk = Query::walk();
    let ctx = double.ctx();
    // SAFETY: the double's own context, on this thread.
    let read = || unsafe { local_player(ctx, &walk, 1, 4) };
    // SAFETY: the double's own context, on this thread.
    let found = unsafe { row_kept(&walk, ctx, read) };
    assert_eq!(found, Some(Entity(9)));
    assert_eq!(
        double.row(),
        at_end,
        "the system's own walk goes on as it was"
    );
}

#[test]
fn cached_diagnostics_distinguish_never_read_stale_and_disabled_gates() {
    use enshwick_core::abi::LocalInputInfo as Info;
    let never = cached_status(false, true, true, true, 0, 100);
    assert_eq!(
        never.flags,
        Info::LAYOUT_AVAILABLE | Info::LINK_AVAILABLE | Info::READ_ENABLED
    );
    assert_eq!(never.age_ms, u64::MAX);
    let fresh = cached_status(true, true, true, true, 100, 1100);
    assert_eq!(
        fresh.flags,
        Info::REQUESTED
            | Info::LAYOUT_AVAILABLE
            | Info::LINK_AVAILABLE
            | Info::READ_ENABLED
            | Info::SAMPLE_PRESENT
            | Info::FRESH
    );
    assert_eq!(fresh.age_ms, 1000);
    let stale = cached_status(true, true, true, true, 100, 1101);
    assert_eq!(stale.flags & Info::FRESH, 0);
    assert_eq!(stale.age_ms, 1001);
    for (requested, layout, link, read) in [
        (false, true, true, true),
        (true, false, true, true),
        (true, true, false, true),
        (true, true, true, false),
    ] {
        let disabled = cached_status(requested, layout, link, read, 100, 101);
        assert_eq!(disabled.flags & Info::FRESH, 0);
        assert_ne!(disabled.flags & Info::SAMPLE_PRESENT, 0);
        assert_eq!(disabled.age_ms, 1);
    }
}

#[test]
fn diagnostics_neither_request_sampling_nor_read_game_state() {
    assert!(!STATE.asked.load(Ordering::Relaxed));
    let info = status();
    assert_eq!(info.flags, enshwick_core::abi::LocalInputInfo::READ_ENABLED);
    assert_eq!(info.age_ms, u64::MAX);
    assert!(!STATE.asked.load(Ordering::Relaxed));
}

#[test]
fn a_panic_in_the_held_read_leaves_the_host_reads_linked() {
    use enshwick_base::Relock;

    let loader = crate::start::tests::bare_loader();
    assert!(link_system(&loader, Some(0x7000)));
    let head: &'static core::sync::atomic::AtomicUsize =
        Box::leak(Box::new(core::sync::atomic::AtomicUsize::new(0)));
    {
        let mut chains = loader.chains.relock();
        chains.seal();
        for chain in chains.iter_mut() {
            chain.head = Some(head);
            chain.original = 0xff;
            chain.wire(|_| false);
        }
    }
    let ours = crate::engine::chains::Next::address_of(
        player_ui as unsafe extern "system" fn(*mut u64) -> u64,
    );
    assert_eq!(head.load(Ordering::Acquire), ours);
    let caught = FUSE.run(FEATURE, || panic!("a test"));
    assert_eq!(caught, None::<()>);
    loader.rewire();
    assert!(FUSE.is_off());
    assert!(!HOST_FUSE.is_off() && !LINK_FUSE.is_off());
    assert_eq!(
        head.load(Ordering::Acquire),
        ours,
        "the host reads still run in the system"
    );
}

#[test]
fn a_system_that_does_not_link_lists_only_the_shared_link_off() {
    let loader = crate::start::tests::bare_loader();
    assert!(!link_system(&loader, None));
    assert!(LINK_FUSE.is_off());
    assert!(!FUSE.is_off() && !HOST_FUSE.is_off());
    let listed = crate::engine::diag::parts_off();
    let parts: Vec<&str> = listed.iter().map(|p| p.part.as_str()).collect();
    assert!(parts.contains(&LINK_FUSE.feature()), "{parts:?}");
    assert!(
        !parts.contains(&FEATURE) && !parts.contains(&HOST_FEATURE),
        "one cause, one entry: {parts:?}"
    );
}

#[test]
fn a_client_without_player_ui_lists_only_the_shared_link_off() {
    let mut loader = crate::start::tests::bare_loader();
    loader.side = Side::CLIENT;
    link(&loader);
    let listed = crate::engine::diag::parts_off();
    let parts: Vec<&str> = listed.iter().map(|p| p.part.as_str()).collect();
    assert_eq!(parts, [LINK_FUSE.feature()], "one cause, one entry");
    assert!(!FUSE.is_off() && !HOST_FUSE.is_off());
}

#[test]
fn the_held_read_takes_the_first_row_whose_entity_is_the_local_player() {
    let rows = |pairs: &[(u32, u32)]| {
        first_local(pairs.iter().map(|&(id, row)| (Entity(id), Entity(row))))
    };
    assert_eq!(
        rows(&[(0, 0)]),
        None,
        "entity 0, as at the system's entry, is never the local player"
    );
    assert_eq!(rows(&[(7, 3), (7, 7), (9, 9)]), Some(Entity(7)));
    assert_eq!(rows(&[(7, 3), (7, 4)]), None, "another player's rows");
    assert_eq!(rows(&[]), None);
}

#[test]
fn the_local_player_word_comes_from_the_routed_frame_and_a_reshuffled_one_turns_the_read_off() {
    assert_eq!(
        local_player_word(0x3c8, 0x70, 0x190),
        Some(36),
        "client 26430625"
    );
    assert_eq!(
        local_player_word(0x3d0, 0x70, 0x190),
        None,
        "another iterator size"
    );
    assert_eq!(
        local_player_word(0x3c8, 0x70, 0x194),
        None,
        "not a whole word"
    );
    assert_eq!(
        local_player_word(0x3c8, 0x190, 0x70),
        None,
        "before the iterator"
    );
    assert_eq!(
        local_player_word(0x3c8, 0x70, 0x70 + 0x3c8),
        None,
        "past its end"
    );
    assert_eq!(
        local_player_word(0x3c8, 0x70, 0x70 + 0x3c0),
        Some(WORDS - 1)
    );
}

#[test]
fn the_held_read_s_iterator_layout_matches_player_ui_on_the_installed_client() {
    use enshwick_gamedata::reflect::{Image, Layout, Reflection};

    let Some(exe) = std::env::var_os("ENSH_CLIENT_EXE") else {
        return;
    };
    let bytes = std::fs::read(exe).unwrap();
    let image = Image::parse(&bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let system = reflection.system(SYSTEM).unwrap();
    assert_eq!(system.reads(), READS);
    assert_eq!(system.writes(), WRITES);
}

#[test]
fn the_first_ask_starts_the_reads_and_answers_missing() {
    assert!(
        !STATE.asked.load(Ordering::Relaxed),
        "the detour reads nothing"
    );
    assert_eq!(held(), Err(HeldError));
    assert!(STATE.asked.load(Ordering::Relaxed));
}

/// The raw state of a mask `bits` read at `read`, on and with no miss.
fn raw(read: u64, bits: u64) -> Raw {
    Raw {
        off: false,
        read,
        bits,
        miss: 0,
        miss_ms: 0,
    }
}

#[test]
fn a_mask_read_with_no_control_held_is_a_fresh_sample_of_zero_bits() {
    assert_eq!(
        sample_at(raw(100, 0), 150),
        Sample::Fresh {
            bits: 0,
            age_ms: 50
        }
    );
}

#[test]
fn a_stale_sample_keeps_its_mask_and_age_and_stays_apart_from_never_read() {
    let now = 101 + FRESH_MS;
    let missed = |read: u64, bits: u64, miss: Miss| Raw {
        miss: miss as u8,
        miss_ms: now,
        ..raw(read, bits)
    };
    let stale = sample_at(missed(100, 1 << 30, Miss::NoPlayer), now);
    assert_eq!(
        stale,
        Sample::Stale {
            bits: 1 << 30,
            age_ms: FRESH_MS + 1,
            miss: Some(Miss::NoPlayer)
        }
    );
    assert_eq!(
        sample_at(missed(0, 0, Miss::NoInput), now),
        Sample::Never {
            miss: Some(Miss::NoInput)
        }
    );
    assert_eq!(sample_at(raw(0, 0), 5), Sample::Never { miss: None });
    assert_eq!(
        sample_at(
            Raw {
                off: true,
                ..raw(100, 1)
            },
            150
        ),
        Sample::Off,
        "a read turned off answers off, however fresh"
    );
}

#[test]
fn a_miss_the_last_step_left_before_the_steps_stopped_names_no_cause() {
    let left = Raw {
        miss: Miss::NoPlayer as u8,
        miss_ms: 200,
        ..raw(100, 1)
    };
    assert_eq!(
        sample_at(left, 201 + FRESH_MS),
        Sample::Stale {
            bits: 1,
            age_ms: FRESH_MS + 101,
            miss: None
        }
    );
}

#[test]
fn a_client_whose_shared_link_is_off_samples_off_not_never_read() {
    let mut loader = crate::start::tests::bare_loader();
    loader.side = Side::CLIENT;
    link(&loader);
    assert!(LINK_FUSE.is_off() && !FUSE.is_off());
    assert_eq!(sample_at(super::raw(), now_ms()), Sample::Off);
}

#[test]
fn an_unknown_raw_miss_reads_as_none() {
    assert_eq!(Miss::from_raw(0), None);
    assert_eq!(Miss::from_raw(1), Some(Miss::NoPlayer));
    assert_eq!(Miss::from_raw(2), Some(Miss::NoInput));
    assert_eq!(Miss::from_raw(3), None);
}

#[test]
fn a_host_read_comes_back_whole_and_only_for_its_own_entity() {
    let read = HostRead::new();
    assert_eq!(read.snapshot(), Some((0, [0; 16], [0; 3])));
    let template = core::array::from_fn(|i| u8::try_from(i + 1).unwrap());
    read.write(42, template, [1 << 32, -(5 << 32), 7]);
    assert_eq!(
        read.snapshot(),
        Some((42, template, [1 << 32, -(5 << 32), 7]))
    );
    read.0.begin_write();
    assert_eq!(read.snapshot(), None, "mid-write");
}

#[test]
fn the_interface_asks_for_its_host_and_gets_it_once_a_step_read_it() {
    assert_eq!(screen_host(42), None);
    assert_eq!(STATE.host_wanted.load(Ordering::Relaxed), 42);
    STATE.host.write(42, [9; 16], [3, 4, 5]);
    STATE.host_ms.store(now_ms(), Ordering::Release);
    assert_eq!(screen_host(42), Some(([9; 16], [3, 4, 5])));
    assert_eq!(screen_host(43), None, "another host's read");
    STATE.host.write(43, [0; 16], [0; 3]);
    assert_eq!(screen_host(43), None, "no template read");
    assert_eq!(screen_host(0), None);
    assert_eq!(STATE.host_wanted.load(Ordering::Relaxed), 0);
}

#[test]
fn a_host_read_no_step_renewed_is_no_answer() {
    assert!(!fresh(0, 5), "never read");
    assert!(fresh(100, 100 + FRESH_MS));
    assert!(
        !fresh(100, 101 + FRESH_MS),
        "a read from before a world load"
    );
}

#[test]
fn the_player_s_keys_are_those_of_the_read_without_the_presses_bits() {
    let (block, jump) = (1 << 30, 1 << 32);
    STATE.bits.store(block | jump, Ordering::Relaxed);
    STATE.players.store(block, Ordering::Relaxed);
    STATE.read_ms.store(now_ms().max(1), Ordering::Release);
    assert_eq!(
        players_held(),
        Some(block),
        "the press's jump is not theirs"
    );
    STATE.read_ms.store(0, Ordering::Release);
    assert_eq!(players_held(), None, "no fresh read");
}

#[test]
fn a_read_after_the_reads_turned_off_or_went_stale_is_no_answer() {
    let read = Some((42, [9; 16], [3, 4, 5]));
    assert_eq!(answer(42, true, read), Some(([9; 16], [3, 4, 5])));
    assert_eq!(answer(42, false, read), None);
}
