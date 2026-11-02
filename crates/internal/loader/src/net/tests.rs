//! Tests of the hello, the owed join data and the rates each side keeps per peer.

use super::server::{
    Greeting, Handover, MAX_GREETING, MAX_HANDOVERS, greet, handed, keep_greeting_in,
    make_greeting_room, owed_in, take_frame,
};
use super::*;
use std::time::Duration;

use crate::wire::{DATA_AGAIN, Frame, Partial, read_hello};

pub(super) fn parsed(lines: &[String]) -> Vec<Frame> {
    lines
        .iter()
        .map(|l| parse(l.strip_prefix(PREFIX).unwrap()).unwrap())
        .collect()
}

#[test]
fn data_dropped_on_the_way_is_owed_until_it_goes_whole() {
    let mut state = State::default();
    let now = Instant::now();
    assert!(greet(&mut state, 7, now).data);
    assert_eq!(
        handed(&mut state, 7, false, now),
        Handover::Again { first: true }
    );
    assert_eq!(owed_in(&mut state, &[7, 8], true), [7]);
    assert!(
        greet(&mut state, 7, now + Duration::from_secs(5)).data,
        "a hello before it went whole brings it again"
    );
    assert_eq!(
        handed(&mut state, 7, false, now),
        Handover::Again { first: false },
        "only the first miss is logged"
    );
    assert_eq!(
        handed(&mut state, 7, true, now + Duration::from_secs(6)),
        Handover::Whole
    );
    assert!(owed_in(&mut state, &[7], true).is_empty());
    assert!(!greet(&mut state, 7, now + Duration::from_secs(10)).data);
    handed(&mut state, 9, false, now);
    assert!(
        owed_in(&mut state, &[7], true).is_empty(),
        "no peer and no live slot, nothing owed"
    );
}

#[test]
fn owed_data_stops_after_a_few_tries_or_when_lines_for_one_player_are_off() {
    let mut state = State::default();
    let now = Instant::now();
    let _greeting = greet(&mut state, 7, now);
    let tries: Vec<Handover> = (0..MAX_HANDOVERS)
        .map(|_| handed(&mut state, 7, false, now))
        .collect();
    assert_eq!(tries.last(), Some(&Handover::GaveUp));
    assert!(owed_in(&mut state, &[7], true).is_empty());
    handed(&mut state, 7, false, now);
    assert!(
        owed_in(&mut state, &[7], false).is_empty(),
        "no line could go"
    );
}

#[test]
fn a_repeated_hello_keeps_the_peer_and_spends_its_rate() {
    let mut state = State::default();
    let now = Instant::now();
    assert_eq!(
        greet(&mut state, 7, now),
        Greeting {
            fresh: true,
            data: true
        }
    );
    handed(&mut state, 7, true, now);
    state.peers.get_mut(&7).unwrap().partial.insert(
        (1, 1),
        Partial {
            total: 2,
            parts: Vec::new(),
        },
    );
    let lines = control_frames(&hello("you.trading-post@0.3.0", 0, Language::FALLBACK)).unwrap();
    let hellos = core::iter::repeat_with(|| take_frame(&mut state, 7, parsed(&lines).remove(0)))
        .take(100)
        .take_while(Option::is_some)
        .count();
    assert!(hellos < 50, "{hellos}");
    assert_eq!(
        greet(&mut state, 7, now),
        Greeting {
            fresh: false,
            data: false
        }
    );
    assert_eq!(state.peers[&7].partial.len(), 1);
    assert_eq!(
        greet(&mut state, 7, now + DATA_AGAIN),
        Greeting {
            fresh: false,
            data: true
        },
        "a client back without a leave gets its data again"
    );
}

#[test]
fn a_new_client_s_hello_in_several_frames_comes_back_whole() {
    let many: Vec<String> = (0..100).map(|i| format!("mod-{i:04}@1.0.0")).collect();
    let mods = many.join(",");
    let lines = control_frames(&hello(&mods, 0, Language::FALLBACK)).unwrap();
    assert!(lines.len() > 1);
    let mut state = State::default();
    let mut whole = None;
    for frame in parsed(&lines) {
        whole = take_frame(&mut state, 7, frame);
    }
    let (channel, data, known) = whole.unwrap();
    assert_eq!((channel, known), (CONTROL, false));
    assert_eq!(read_hello(&data).map(|h| h.mods), Some(mods));
    assert!(state.peers.is_empty(), "a hello alone makes no peer");
}

#[test]
fn a_stranger_s_frames_on_a_mod_channel_are_not_taken() {
    let mut state = State::default();
    let frame = parsed(&frames(5, 1, b"data").unwrap()).remove(0);
    assert_eq!(take_frame(&mut state, 7, frame), None);
}

#[test]
fn frames_from_an_unknown_sender_are_not_taken() {
    let mut state = State::default();
    let frame = parsed(&control_frames(&hello("", 0, Language::FALLBACK)).unwrap()).remove(0);
    assert_eq!(take_frame(&mut state, 0, frame), None);
    assert!(state.greeting.is_empty());
}

#[test]
fn every_frame_of_a_stranger_takes_from_its_rate() {
    let mut state = State::default();
    let many: Vec<String> = (0..100).map(|i| format!("mod-{i:04}@1.0.0")).collect();
    let lines = control_frames(&hello(&many.join(","), 0, Language::FALLBACK)).unwrap();
    for _ in 0..200 {
        let _taken = take_frame(&mut state, 7, parsed(&lines).remove(1));
    }
    assert!(state.greeting[&7].tokens < 1.0, "the rate ran out");
}

#[test]
fn strangers_waiting_to_say_hello_are_bounded_and_follow_the_live_slots() {
    let mut state = State::default();
    let many: Vec<String> = (0..100).map(|i| format!("mod-{i:04}@1.0.0")).collect();
    let lines = control_frames(&hello(&many.join(","), 0, Language::FALLBACK)).unwrap();
    for key in 1..=1000 {
        let _taken = take_frame(&mut state, key, parsed(&lines).remove(0));
    }
    assert!(state.greeting.len() <= MAX_GREETING);
    keep_greeting_in(&mut state, &[3, 4]);
    let mut kept: Vec<u64> = state.greeting.keys().copied().collect();
    kept.sort_unstable();
    assert_eq!(kept, [3, 4]);
}

#[test]
fn a_hello_s_language_stays_with_its_client_and_goes_with_it() {
    let mut state = State::default();
    let lines = control_frames(&hello("", 0, Language::UK_UA)).unwrap();
    let _taken = take_frame(&mut state, 7, parsed(&lines).remove(0));
    remember_language(&mut state, 7, Language::UK_UA);
    remember_language(&mut state, 8, Language::DE_DE);
    assert_eq!(language_in(&state, 7), Some(Language::UK_UA));
    assert_eq!(language_in(&state, 8), None, "no frame, no peer to keep it");
    assert!(greet(&mut state, 7, Instant::now()).fresh);
    assert_eq!(language_in(&state, 7), Some(Language::UK_UA));
    state.peers.remove(&7);
    assert_eq!(language_in(&state, 7), None);
    let _again = take_frame(&mut state, 9, parsed(&lines).remove(0));
    remember_language(&mut state, 9, Language::DE_DE);
    keep_greeting_in(&mut state, &[]);
    assert_eq!(language_in(&state, 9), None);
}

#[test]
fn a_full_greeting_store_makes_room_from_keys_without_a_live_slot() {
    let mut state = State::default();
    let lines = control_frames(&hello("you.trading-post@0.3.0", 0, Language::FALLBACK)).unwrap();
    for key in 1..=MAX_GREETING as u64 {
        let _taken = take_frame(&mut state, key, parsed(&lines).remove(0));
    }
    assert_eq!(state.greeting.len(), MAX_GREETING);
    let newcomer = MAX_GREETING as u64 + 1;
    make_greeting_room(&mut state, 1, || unreachable!("a known key needs no room"));
    make_greeting_room(&mut state, newcomer, || None);
    assert_eq!(
        state.greeting.len(),
        MAX_GREETING,
        "no slots read, none forgotten"
    );
    make_greeting_room(&mut state, newcomer, || Some(vec![2, newcomer]));
    assert!(take_frame(&mut state, newcomer, parsed(&lines).remove(0)).is_some());
    let mut kept: Vec<u64> = state.greeting.keys().copied().collect();
    kept.sort_unstable();
    assert_eq!(kept, [2, newcomer]);
}

#[test]
fn a_client_takes_its_server_s_frames_up_to_a_rate() {
    let mut state = State::default();
    let start = Instant::now();
    let taken = (0..2000)
        .take_while(|_| admit_from_server(&mut state, start))
        .count();
    assert_eq!(taken, 512, "one burst");
    assert!(!admit_from_server(&mut state, start), "a flood is dropped");
    let later = start + Duration::from_millis(250);
    let refilled = (0..2000)
        .take_while(|_| admit_from_server(&mut state, later))
        .count();
    assert_eq!(refilled, 256, "a quarter second of the rate");
    // The server's own sending fits: 256 lines waiting, then 16 a step.
    let mut honest = State::default();
    assert!((0..256).all(|_| admit_from_server(&mut honest, start)));
    assert!((1..=60).all(|step| {
        let at = start + Duration::from_secs_f64(f64::from(step) / 60.0);
        (0..16).all(|_| admit_from_server(&mut honest, at))
    }));
}

#[test]
fn channel_ids_are_per_mod_and_never_the_protocol_s() {
    assert_ne!(channel_id("a", "x"), channel_id("b", "x"));
    assert_ne!(channel_id("a", "x"), CONTROL);
}
