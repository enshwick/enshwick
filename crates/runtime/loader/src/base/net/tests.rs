//! Tests of the hello, the owed join data and the rates each side keeps per peer.

use super::outbox::{
    Hold, MAX_HELD, MAX_HELD_BYTES, Taken, hold, put_back, refused, stalled, take, unwelcome,
    welcomed,
};
use super::server::{
    Greeting, Handover, MAX_GREETING, MAX_HANDOVERS, greet, handed, keep_greeting_in,
    make_greeting_room, owed_in, take_message,
};
use super::*;
use std::time::{Duration, Instant};

use crate::base::wire::DATA_AGAIN;

#[test]
fn data_dropped_on_the_way_is_owed_until_it_goes_whole() {
    let mut state = Links::default();
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
fn a_language_named_after_the_hello_fills_only_a_peer_whose_hello_named_none() {
    let (silent, named) = (0x1a96_0001, 0x1a96_0002);
    state(|s| {
        s.peers.insert(silent, Peer::new());
        let mut peer = Peer::new();
        peer.language = Some(Language::FALLBACK);
        s.peers.insert(named, peer);
    });
    language_named(silent, u32::MAX);
    assert_eq!(language_of(silent), None, "an unknown number is dropped");
    language_named(silent, Language::UK_UA.0);
    language_named(named, Language::UK_UA.0);
    language_named(0x1a96_0003, Language::UK_UA.0);
    assert_eq!(language_of(silent), Some(Language::UK_UA));
    assert_eq!(language_number(silent), Language::UK_UA.0);
    assert_eq!(language_number(0x1a96_0003), 0, "the fallback");
    assert_eq!(
        language_of(named),
        Some(Language::FALLBACK),
        "the hello's wins"
    );
    assert_eq!(language_of(0x1a96_0003), None, "no peer, nothing kept");
    state(|s| {
        s.peers.remove(&silent);
        s.peers.remove(&named);
    });
}

#[test]
fn the_hosting_client_s_own_player_reads_the_game_s_language_before_any_hello() {
    struct Unhost;
    impl Drop for Unhost {
        fn drop(&mut self) {
            crate::base::carrier::unhost_for_test();
        }
    }
    let _alone = crate::base::carrier::tests::alone();
    let (host, guest) = (0x1a96_0011, 0x1a96_0012);
    crate::base::carrier::host_for_test(host);
    let unhost = Unhost;
    let game = crate::content::served::language();
    assert_eq!(language_of(host), Some(game));
    assert_eq!(language_number(host), game.0);
    assert_eq!(
        language_of(guest),
        None,
        "another player waits for its hello"
    );
    drop(unhost);
    assert_eq!(language_of(host), None, "a client that stopped hosting");
}

#[test]
fn a_game_language_the_abi_does_not_list_reads_as_unknown() {
    assert_eq!(known(Language::FALLBACK), Some(Language::FALLBACK));
    assert_eq!(known(Language(u32::MAX)), None);
}

#[test]
fn owed_data_stops_after_a_few_tries() {
    let mut state = Links::default();
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
    let mut state = Links::default();
    let now = Instant::now();
    assert_eq!(
        greet(&mut state, 7, now),
        Greeting {
            fresh: true,
            data: true
        }
    );
    handed(&mut state, 7, true, now);
    let hellos = core::iter::repeat_with(|| take_message(&mut state, 7, CONTROL, false))
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
    let mut state = Links::default();
    assert_eq!(take_message(&mut state, 7, 5, false), None);
    assert_eq!(take_message(&mut state, 7, CONTROL, false), Some(false));
    assert!(state.peers.is_empty(), "a hello alone makes no peer");
}

#[test]
fn a_message_from_an_unknown_sender_is_not_taken() {
    let mut state = Links::default();
    assert_eq!(take_message(&mut state, 0, CONTROL, false), None);
    assert!(state.greeting.is_empty());
}

#[test]
fn every_message_of_a_stranger_takes_from_its_rate() {
    let mut state = Links::default();
    for _ in 0..200 {
        let _taken = take_message(&mut state, 7, CONTROL, false);
    }
    assert!(state.greeting[&7].rate.tokens() < 1.0, "the rate ran out");
}

#[test]
fn strangers_waiting_to_say_hello_are_bounded_and_follow_the_live_slots() {
    let mut state = Links::default();
    for key in 1..=1000 {
        let _taken = take_message(&mut state, key, CONTROL, false);
    }
    assert!(state.greeting.len() <= MAX_GREETING);
    keep_greeting_in(&mut state, &[3, 4]);
    let mut kept: Vec<u64> = state.greeting.keys().copied().collect();
    kept.sort_unstable();
    assert_eq!(kept, [3, 4]);
}

#[test]
fn a_hello_s_language_stays_with_its_client_and_goes_with_it() {
    let mut state = Links::default();
    let _taken = take_message(&mut state, 7, CONTROL, false);
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
    let _again = take_message(&mut state, 9, CONTROL, false);
    remember_language(&mut state, 9, Language::DE_DE);
    keep_greeting_in(&mut state, &[]);
    assert_eq!(language_in(&state, 9), None);
}

#[test]
fn a_hosting_client_s_burst_to_its_own_server_half_is_refused_past_its_rate_and_the_rest_arrives() {
    static ARRIVED: AtomicU64 = AtomicU64::new(0);
    unsafe extern "system" fn arrived(_: *const NetMessage, _: *mut c_void) {
        ARRIVED.fetch_add(1, Ordering::Relaxed);
    }
    const CHANNEL: u32 = 5;
    // The server's peer in the net's state, and whether a send goes, are
    // shared with the carrier's tests.
    let _link = crate::base::carrier::tests::alone();
    let (host, wired) = (7, 9);
    let in_burst = |count: u64| {
        let burst = crate::base::wire::MESSAGE_RATE.burst;
        (burst..=burst + 2.0).contains(&f64::from(u32::try_from(count).unwrap()))
    };
    let _pending = CHANNELS.edit(|pending| {
        pending.push(Channel {
            id: CHANNEL,
            name: "test/burst".to_owned(),
            handler: Registered::new(ModIndex::LOADER, arrived, core::ptr::null_mut()),
        });
    });
    let _sealed = CHANNELS.seal();
    crate::base::carrier::host_for_test(host);
    state(|s| {
        s.peers.insert(NetMessage::SERVER.get(), Peer::new());
        let _host = greet(s, host, Instant::now());
        let _wired = greet(s, wired, Instant::now());
        // A client sends only once the server's welcome reached it.
        welcomed(s, host, false);
        welcomed(s, wired, false);
    });
    let dropped = STATE.dropped.load(Ordering::Relaxed);
    let refused = (0..200)
        .filter(|_| send_to_server(CHANNEL, b"burst") == Err(NetError::Throttled))
        .count();
    let admitted = 200 - u64::try_from(refused).unwrap();
    assert!(
        in_burst(admitted),
        "{admitted} sent before the rate refused"
    );
    assert!(STATE.dropped.load(Ordering::Relaxed) - dropped >= 200 - admitted);
    crate::base::carrier::take_hosted();
    assert_eq!(
        ARRIVED.load(Ordering::Relaxed),
        admitted,
        "the server half takes every admitted one"
    );
    let before = ARRIVED.load(Ordering::Relaxed);
    for _ in 0..200 {
        server_carried(wired, CHANNEL, b"burst", false);
    }
    assert!(
        in_burst(ARRIVED.load(Ordering::Relaxed) - before),
        "a wire client still runs out of its receive rate"
    );
    crate::base::carrier::unhost_for_test();
    link_lost();
    state(|s| {
        s.sending.remove(&NetMessage::SERVER.get());
    });
}

#[test]
fn a_full_greeting_store_makes_room_from_keys_without_a_live_slot() {
    let mut state = Links::default();
    for key in 1..=MAX_GREETING as u64 {
        let _taken = take_message(&mut state, key, CONTROL, false);
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
    assert!(take_message(&mut state, newcomer, CONTROL, false).is_some());
    let mut kept: Vec<u64> = state.greeting.keys().copied().collect();
    kept.sort_unstable();
    assert_eq!(kept, [2, newcomer]);
}

#[test]
fn a_settings_snapshot_from_the_server_is_never_read_as_stat_values() {
    let _link = crate::base::carrier::tests::alone();
    client_carried(CONTROL, &control(WELCOME));
    let snapshot = crate::base::remote::Message::Snapshot {
        id: String::new(),
        admin: false,
        schema: String::new(),
        text: "you.bells@1.0.0 a long enough list of mods".to_owned(),
    };
    client_carried(CONTROL, &remote_data(&snapshot).expect("a short snapshot"));
    assert!(
        STATE.own_stats.load().is_none(),
        "the snapshot reached the own stats"
    );
    // The welcome made the server ready; the next test sharing the net's
    // state starts without a link.
    link_lost();
}

#[test]
fn each_side_reads_every_control_kind_by_one_number() {
    use crate::base::remote as r;
    use crate::base::wire as w;
    let client_reads = [
        WELCOME,
        NOTICE,
        ITEM_DATA,
        UI_DEMO,
        w::PROBE,
        w::MODS,
        STAT_VALUES,
        SOUND,
        r::SYNC,
        r::SNAPSHOT,
        r::RESULT,
        r::RECIPES,
        r::CONTROLS,
        r::WINDOW,
        r::WINDOW_CLOSE,
        r::FOG_UNCOVER,
    ];
    let server_reads = [
        w::HELLO,
        w::PROBE,
        r::QUERY,
        r::EDIT,
        r::ACTIONS_LANGUAGE,
        r::ACTION_RUN,
        r::WINDOW_CLOSE,
    ];
    for kinds in [&client_reads[..], &server_reads[..]] {
        let mut sorted = kinds.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            kinds.len(),
            "a kind number used twice: {kinds:?}"
        );
    }
}

#[test]
fn the_server_is_ready_from_its_welcome_until_the_link_goes_and_refusals_cost_no_rate() {
    // The server's peer in the net's state, and whether a send goes, are
    // shared with the carrier's tests.
    let _link = crate::base::carrier::tests::alone();
    let server = NetMessage::SERVER.get();
    assert!(!ready(server));
    client_carried(5, b"before the welcome");
    client_carried(CONTROL, &control(WELCOME));
    assert!(ready(server) && STATE.answered.load(Ordering::Acquire));
    link_lost();
    assert!(!ready(server), "a new link waits for its own welcome");
    assert!(
        !STATE.answered.load(Ordering::Acquire),
        "a frame looks again"
    );
    // One test, since both read the net's shared state.
    client_carried(CONTROL, &control(WELCOME));
    for _ in 0..200 {
        assert_eq!(
            send_to_server(5, b"x"),
            Err(NetError::Carrier(
                crate::base::carrier::CarrierError::NoLink
            )),
            "no link"
        );
    }
    let tokens = state(|s| s.sending[&server].rate.tokens());
    assert!(tokens >= 39.0, "{tokens}");
}

#[test]
fn a_game_s_end_forgets_its_peers_and_answer_but_keeps_the_next_join_s_flag_and_the_run_s_drops() {
    let state = State::new();
    state
        .links
        .relock()
        .get_or_insert_with(Links::default)
        .peers
        .insert(7, Peer::new());
    state.answered.store(true, Ordering::Release);
    state.server_runs_enshwick.store(true, Ordering::Release);
    state.dropped.store(3, Ordering::Relaxed);
    state.forget();
    assert!(state.links.relock().is_none());
    assert!(!state.answered.load(Ordering::Acquire));
    assert!(
        state.server_runs_enshwick.load(Ordering::Acquire),
        "a join checked before the hosted game's end keeps its answer"
    );
    assert_eq!(state.dropped.load(Ordering::Relaxed), 3);
}

#[test]
fn a_message_between_a_hello_and_a_failed_welcome_waits_for_the_next_welcome() {
    let mut state = Links::default();
    let now = Instant::now();
    assert_eq!(hold(&mut state, 7, CONTROL, b"early"), Hold::Unknown);
    let greeting = greet(&mut state, 7, now);
    assert!(greeting.fresh && greeting.data);
    assert_eq!(hold(&mut state, 7, CONTROL, b"one"), Hold::Held);
    assert_eq!(
        take(&mut state, 7),
        Taken::Nothing,
        "the welcome did not go: nothing is sent"
    );
    assert_eq!(hold(&mut state, 7, 5, b"two"), Hold::Held);
    let _again = greet(&mut state, 7, now + Duration::from_secs(5));
    welcomed(&mut state, 7, greeting.data);
    assert_eq!(
        take(&mut state, 7),
        Taken::Messages([(CONTROL, b"one".to_vec()), (5, b"two".to_vec())].into()),
        "in the order sent"
    );
    assert_eq!(
        hold(&mut state, 7, CONTROL, b"three"),
        Hold::Held,
        "behind them"
    );
    assert!(matches!(take(&mut state, 7), Taken::Messages(m) if m.len() == 1));
    assert_eq!(take(&mut state, 7), Taken::Ready { data: true });
    assert_eq!(hold(&mut state, 7, CONTROL, b"four"), Hold::Ready);
}

#[test]
fn a_link_that_starts_over_holds_messages_until_its_next_welcome_and_owes_the_join_data_again() {
    let mut state = Links::default();
    let now = Instant::now();
    let _greeting = greet(&mut state, 7, now);
    welcomed(&mut state, 7, true);
    assert_eq!(take(&mut state, 7), Taken::Ready { data: true });
    handed(&mut state, 7, true, now);
    state.peers.get_mut(&7).unwrap().language = Some(Language::UK_UA);
    unwelcome(&mut state, 7);
    unwelcome(&mut state, 8);
    assert!(!state.peers.contains_key(&8), "no peer, nothing made");
    assert_eq!(hold(&mut state, 7, CONTROL, b"held"), Hold::Held);
    assert_eq!(state.peers[&7].language, Some(Language::UK_UA));
    let greeting = greet(&mut state, 7, now + Duration::from_secs(1));
    assert!(!greeting.fresh && greeting.data, "a new link gets the data");
    welcomed(&mut state, 7, greeting.data);
    assert_eq!(
        take(&mut state, 7),
        Taken::Messages([(CONTROL, b"held".to_vec())].into())
    );
    assert_eq!(take(&mut state, 7), Taken::Ready { data: true });
}

#[test]
fn an_outbox_holds_up_to_its_bound_and_what_did_not_go_goes_back_first() {
    let mut state = Links::default();
    let _greeting = greet(&mut state, 7, Instant::now());
    for _ in 0..MAX_HELD {
        assert_eq!(hold(&mut state, 7, CONTROL, b"x"), Hold::Held);
    }
    assert_eq!(hold(&mut state, 7, CONTROL, b"x"), Hold::Full);
    let _greeting = greet(&mut state, 8, Instant::now());
    assert_eq!(
        hold(&mut state, 8, CONTROL, &vec![0; MAX_HELD_BYTES + 1]),
        Hold::Full
    );
    welcomed(&mut state, 7, false);
    let Taken::Messages(mut batch) = take(&mut state, 7) else {
        panic!("held messages go");
    };
    let _sent = batch.pop_front();
    assert_eq!(hold(&mut state, 7, 5, b"later"), Hold::Held);
    put_back(&mut state, 7, batch);
    assert_eq!(
        stalled(&state, &[7, 8]),
        [7],
        "only a welcomed one waits for room"
    );
    let Taken::Messages(batch) = take(&mut state, 7) else {
        panic!("the rest goes");
    };
    assert_eq!(batch.len(), MAX_HELD);
    assert_eq!(batch.back(), Some(&(5, b"later".to_vec())));
}

#[test]
fn a_client_whose_link_started_over_sends_only_its_hello_until_its_next_welcome() {
    let mut state = Links::default();
    let now = Instant::now();
    let _greeting = greet(&mut state, 7, now);
    welcomed(&mut state, 7, true);
    assert_eq!(take(&mut state, 7), Taken::Ready { data: true });
    assert_eq!(take_message(&mut state, 7, 5, false), Some(true));
    unwelcome(&mut state, 7);
    assert_eq!(
        take_message(&mut state, 7, 5, false),
        None,
        "no mod message on the new link before its welcome"
    );
    assert_eq!(
        take_message(&mut state, 7, CONTROL, false),
        Some(false),
        "a hello, never a settings edit"
    );
    refused(&mut state, 7);
    assert!(
        !state.peers.contains_key(&7) && !state.unwelcomed.contains_key(&7),
        "a hello kept out on the new link forgets the client"
    );
    let _greeting = greet(&mut state, 8, now);
    welcomed(&mut state, 8, false);
    refused(&mut state, 8);
    assert!(
        state.peers.contains_key(&8),
        "a client whose welcome went stays"
    );
}

#[test]
fn what_goes_back_to_an_outbox_stays_within_its_bound_and_the_newest_are_dropped() {
    let mut state = Links::default();
    let _greeting = greet(&mut state, 7, Instant::now());
    for _ in 0..MAX_HELD {
        assert_eq!(hold(&mut state, 7, CONTROL, b"old"), Hold::Held);
    }
    welcomed(&mut state, 7, false);
    let Taken::Messages(batch) = take(&mut state, 7) else {
        panic!("held messages go");
    };
    for _ in 0..MAX_HELD {
        assert_eq!(
            hold(&mut state, 7, 5, b"new"),
            Hold::Held,
            "held while the batch is out"
        );
    }
    assert_eq!(put_back(&mut state, 7, batch), MAX_HELD);
    let Taken::Messages(batch) = take(&mut state, 7) else {
        panic!("the rest goes");
    };
    assert_eq!(batch.len(), MAX_HELD);
    assert!(
        batch.iter().all(|&(channel, _)| channel == CONTROL),
        "the oldest stay"
    );
    let _greeting = greet(&mut state, 8, Instant::now());
    welcomed(&mut state, 8, false);
    let big = vec![0; MAX_HELD_BYTES / 2 + 1];
    assert_eq!(
        put_back(
            &mut state,
            8,
            [(CONTROL, big.clone()), (CONTROL, big)].into()
        ),
        1,
        "the bytes bound holds too"
    );
}

#[test]
fn channel_ids_are_per_mod_and_never_the_protocol_s() {
    assert_ne!(channel_id("a", "x"), channel_id("b", "x"));
    assert_ne!(channel_id("a", "x"), CONTROL);
}
