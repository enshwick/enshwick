//! Tests of the hello, the owed join data and the rates each side keeps per peer.

use super::server::{
    Greeting, Handover, MAX_GREETING, MAX_HANDOVERS, greet, handed, keep_greeting_in,
    make_greeting_room, owed_in, take_message,
};
use super::*;
use std::time::{Duration, Instant};

use crate::wire::DATA_AGAIN;

#[test]
fn data_dropped_on_the_way_is_owed_until_it_goes_whole() {
    let mut state = State::default();
    let now = Instant::now();
    assert!(greet(&mut state, 7, now).data);
    assert_eq!(
        handed(&mut state, 7, false, now),
        Handover::Again { first: true }
    );
    assert_eq!(owed_in(&mut state, &[7, 8]), [7]);
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
    assert!(owed_in(&mut state, &[7]).is_empty());
    assert!(!greet(&mut state, 7, now + Duration::from_secs(10)).data);
    handed(&mut state, 9, false, now);
    assert!(
        owed_in(&mut state, &[7]).is_empty(),
        "no peer and no live slot, nothing owed"
    );
}

#[test]
fn owed_data_stops_after_a_few_tries() {
    let mut state = State::default();
    let now = Instant::now();
    let _greeting = greet(&mut state, 7, now);
    let tries: Vec<Handover> = (0..MAX_HANDOVERS)
        .map(|_| handed(&mut state, 7, false, now))
        .collect();
    assert_eq!(tries.last(), Some(&Handover::GaveUp));
    assert!(owed_in(&mut state, &[7]).is_empty());
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
    let hellos = core::iter::repeat_with(|| take_message(&mut state, 7, CONTROL))
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
fn a_stranger_s_message_on_a_mod_channel_is_not_taken() {
    let mut state = State::default();
    assert_eq!(take_message(&mut state, 7, 5), None);
    assert_eq!(take_message(&mut state, 7, CONTROL), Some(false));
    assert!(state.peers.is_empty(), "a hello alone makes no peer");
}

#[test]
fn a_message_from_an_unknown_sender_is_not_taken() {
    let mut state = State::default();
    assert_eq!(take_message(&mut state, 0, CONTROL), None);
    assert!(state.greeting.is_empty());
}

#[test]
fn every_message_of_a_stranger_takes_from_its_rate() {
    let mut state = State::default();
    for _ in 0..200 {
        let _taken = take_message(&mut state, 7, CONTROL);
    }
    assert!(state.greeting[&7].tokens < 1.0, "the rate ran out");
}

#[test]
fn strangers_waiting_to_say_hello_are_bounded_and_follow_the_live_slots() {
    let mut state = State::default();
    for key in 1..=1000 {
        let _taken = take_message(&mut state, key, CONTROL);
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
    let _taken = take_message(&mut state, 7, CONTROL);
    remember_language(&mut state, 7, Language::UK_UA);
    remember_language(&mut state, 8, Language::DE_DE);
    assert_eq!(language_in(&state, 7), Some(Language::UK_UA));
    assert_eq!(
        language_in(&state, 8),
        None,
        "no message, no peer to keep it"
    );
    assert!(greet(&mut state, 7, Instant::now()).fresh);
    assert_eq!(language_in(&state, 7), Some(Language::UK_UA));
    state.peers.remove(&7);
    assert_eq!(language_in(&state, 7), None);
    let _again = take_message(&mut state, 9, CONTROL);
    remember_language(&mut state, 9, Language::DE_DE);
    keep_greeting_in(&mut state, &[]);
    assert_eq!(language_in(&state, 9), None);
}

#[test]
fn a_full_greeting_store_makes_room_from_keys_without_a_live_slot() {
    let mut state = State::default();
    for key in 1..=MAX_GREETING as u64 {
        let _taken = take_message(&mut state, key, CONTROL);
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
    assert!(take_message(&mut state, newcomer, CONTROL).is_some());
    let mut kept: Vec<u64> = state.greeting.keys().copied().collect();
    kept.sort_unstable();
    assert_eq!(kept, [2, newcomer]);
}

#[test]
fn the_server_is_ready_from_its_welcome_until_the_link_goes_and_refusals_cost_no_rate() {
    let server = NetMessage::SERVER.get();
    assert!(!ready(server));
    client_carried(5, b"before the welcome");
    client_carried(CONTROL, &control(WELCOME));
    assert!(ready(server) && ANSWERED.load(Ordering::Acquire));
    link_lost();
    assert!(!ready(server), "a new link waits for its own welcome");
    assert!(!ANSWERED.load(Ordering::Acquire), "a frame looks again");
    // One test, since both read the net's shared state.
    client_carried(CONTROL, &control(WELCOME));
    for _ in 0..200 {
        assert_eq!(send_to_server(5, b"x"), Err(Status::MISSING), "no link");
    }
    let tokens = state(|s| s.sending[&server].tokens);
    assert!(tokens >= 39.0, "{tokens}");
}

#[test]
fn channel_ids_are_per_mod_and_never_the_protocol_s() {
    assert_ne!(channel_id("a", "x"), channel_id("b", "x"));
    assert_ne!(channel_id("a", "x"), CONTROL);
}
