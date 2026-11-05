use super::*;

/// Serialises the tests, which share the carrier's state.
static SHARED: Mutex<()> = Mutex::new(());

fn alone() -> std::sync::MutexGuard<'static, ()> {
    let guard = SHARED.relock();
    forget();
    *CARRIER.server.relock() = None;
    CARRIER.retired.relock().clear();
    CARRIER.client_retired.store(false, Ordering::Release);
    guard
}

#[test]
fn a_hosted_message_waits_until_its_half_takes_it_whole() {
    let _alone = alone();
    assert!(loop_back(true, 7, 5, b"to the server").is_ok());
    assert!(loop_back(false, 7, 6, b"to the player").is_ok());
    let taken = take_looped(true);
    assert_eq!(taken.len(), 1);
    assert_eq!(
        (taken[0].channel, taken[0].data.as_slice()),
        (5, &b"to the server"[..])
    );
    assert!(take_looped(true).is_empty(), "taken once");
    assert!(
        !CARRIER.server_waiting.load(Ordering::Acquire),
        "an idle step takes no lock"
    );
    assert_eq!(take_looped(false).len(), 1, "the other half's stays");
}

#[test]
fn the_hosted_queues_are_bounded_and_forget_empties_them() {
    let _alone = alone();
    for _ in 0..MAX_LOOPED {
        assert!(loop_back(true, 7, 5, b"x").is_ok());
    }
    assert_eq!(loop_back(true, 7, 5, b"x"), Err(Status::REFUSED));
    CARRIER.host_key.store(9, Ordering::Release);
    CARRIER.hosted_hello.store(true, Ordering::Release);
    assert!(loop_back(false, 7, 6, b"y").is_ok());
    forget();
    assert!(take_looped(true).is_empty() && take_looped(false).is_empty());
    assert_eq!(CARRIER.host_key.load(Ordering::Acquire), 0);
    assert!(
        !CARRIER.hosted_hello.load(Ordering::Acquire),
        "a next hosted game says hello again"
    );
}

#[test]
fn a_new_host_makes_the_client_half_say_hello_again() {
    let _alone = alone();
    set_host(7);
    CARRIER.hosted_hello.store(true, Ordering::Release);
    set_host(7);
    assert!(
        CARRIER.hosted_hello.load(Ordering::Acquire),
        "the same host"
    );
    set_host(8);
    assert!(
        !CARRIER.hosted_hello.load(Ordering::Acquire),
        "another world's host"
    );
    assert!(!client_hosts(), "no game calls, no hosting");
}

#[test]
fn a_payload_that_may_not_travel_is_refused_before_any_link() {
    let _alone = alone();
    assert_eq!(send(7, 5, b"\x7fELF\x02"), Err(Status::BAD_ARGS));
    let refused = send_many(&[7, 8], 5, &[1; MAX_SEALED], |_, _| {
        panic!("a refused payload names no key")
    });
    assert_eq!(refused, Err(Status::BAD_ARGS));
    assert_eq!(send(7, 5, b"fine"), Err(Status::MISSING), "no link yet");
}

#[test]
fn each_key_without_a_link_is_named_and_the_rest_still_go() {
    let _alone = alone();
    let mut failed = Vec::new();
    let sent = send_many(&[7, 8], 5, b"fine", |key, status| {
        failed.push((key, status));
    });
    assert_eq!(sent, Ok(()));
    assert_eq!(failed, [(7, Status::MISSING), (8, Status::MISSING)]);
}

fn peer(heard: bool) -> Peer {
    Peer {
        machine: 0,
        socket: 0,
        slot: 0,
        link: Link::new(1200, 1).expect("a packet that holds a message"),
        heard,
        rate: crate::wire::Peer::new(),
        opened: Instant::now(),
        nudged: false,
        probe: Probe::default(),
    }
}

#[test]
fn keys_of_both_sides_in_one_send_each_go_on_their_own_side() {
    let _alone = alone();
    // The server sends nothing first, so its link is never heard before.
    for (half, key, heard) in [(&CARRIER.server, 7, true), (&CARRIER.client, SERVER, false)] {
        let mut guard = half.relock();
        fresh(&mut guard, 1).peers.insert(key, peer(heard));
    }
    for keys in [[SERVER, 7], [7, SERVER]] {
        let mut failed = Vec::new();
        let sent = send_many(&keys, 5, b"fine", |key, status| {
            failed.push((key, status));
        });
        assert_eq!((sent, failed), (Ok(()), Vec::new()), "{keys:?}");
    }
}

#[test]
fn an_inbox_goes_back_emptied_only_to_its_own_session() {
    let _alone = alone();
    let mut guard = CARRIER.server.relock();
    fresh(&mut guard, 1);
    drop(guard);
    let mut inbox = Inbox::default();
    inbox.bytes.extend_from_slice(&[1; 100]);
    inbox.items.push(Item {
        from: 7,
        channel: 5,
        span: (0, 100),
    });
    give_back(&CARRIER.server, 2, inbox);
    let guard = CARRIER.server.relock();
    assert_eq!(guard.as_ref().map(|s| s.inbox.bytes.capacity()), Some(0));
    drop(guard);
    let mut inbox = Inbox::default();
    inbox.bytes.extend_from_slice(&[1; 100]);
    give_back(&CARRIER.server, 1, inbox);
    let guard = CARRIER.server.relock();
    let kept = guard
        .as_ref()
        .map(|s| (s.inbox.bytes.len(), s.inbox.bytes.capacity() >= 100));
    assert_eq!(kept, Some((0, true)));
}

/// The packet size the fake session answers, and how often the fake game
/// was asked for limits, sockets and their end.
static PACKET: AtomicU32 = AtomicU32::new(0);
static LIMITS: AtomicU32 = AtomicU32::new(0);
static CREATES: AtomicU32 = AtomicU32::new(0);
static DESTROYS: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn fake_limit(_: usize, _: u8) -> u32 {
    LIMITS.fetch_add(1, Ordering::Relaxed);
    PACKET.load(Ordering::Relaxed)
}

unsafe extern "system" fn fake_create(_: usize, _: *const game::Descriptor) -> usize {
    CREATES.fetch_add(1, Ordering::Relaxed);
    0
}

unsafe extern "system" fn fake_destroy(_: usize, _: usize) {
    DESTROYS.fetch_add(1, Ordering::Relaxed);
}

/// Runs `members` for one player's machine 2, hosted by machine 1, at
/// `now` and again well past [`OPEN_AGAIN`].
fn members_twice(packet: u32) -> State {
    PACKET.store(packet, Ordering::Relaxed);
    for counter in [&LIMITS, &CREATES, &DESTROYS] {
        counter.store(0, Ordering::Relaxed);
    }
    let game = Game::for_test(fake_create, fake_destroy, fake_limit);
    let host: [u32; 2] = [1, 0];
    let session = host.as_ptr().expose_provenance();
    let mut slot = None;
    let state = fresh(&mut slot, session);
    let now = Instant::now();
    members(&game, session, state, &[(7, 2)], now);
    members(&game, session, state, &[(7, 2)], now + 2 * OPEN_AGAIN);
    slot.expect("members keeps the state")
}

#[test]
fn a_session_whose_packets_hold_no_message_gets_no_socket_and_no_retry() {
    let _alone = alone();
    let state = members_twice(0);
    assert!(state.off);
    assert_eq!(LIMITS.load(Ordering::Relaxed), 3, "asked once, each mode");
    assert_eq!(CREATES.load(Ordering::Relaxed), 0);
    assert_eq!(DESTROYS.load(Ordering::Relaxed), 0);
}

#[test]
fn a_socket_the_session_did_not_make_is_tried_again_later() {
    let _alone = alone();
    crate::log::capture();
    let state = members_twice(1200);
    let sized = crate::log::captured()
        .into_iter()
        .filter(|(_, _, line)| line.contains("bytes a packet"))
        .count();
    crate::log::end_capture();
    assert_eq!(sized, 0, "no packet size line for a socket never made");
    assert!(!state.off);
    assert_eq!(CREATES.load(Ordering::Relaxed), 2);
    assert_eq!(DESTROYS.load(Ordering::Relaxed), 0);
    assert!(state.peers.is_empty() && state.tried.contains_key(&7));
}

#[test]
fn a_session_turned_off_is_on_again_after_the_game_ends() {
    let _alone = alone();
    *CARRIER.server.relock() = Some(members_twice(0));
    forget();
    let server = CARRIER.server.relock();
    assert_eq!(server.as_ref().map(|s| s.off), Some(false));
}

#[test]
fn a_socket_of_the_last_game_is_closed_in_the_session_the_client_keeps() {
    let _alone = alone();
    DESTROYS.store(0, Ordering::Relaxed);
    let game = Game::for_test(fake_create, fake_destroy, fake_limit);
    // Channel 0 and peer 0, where the fake game's offsets read a socket.
    let slot = [0u32; 2];
    let socket = slot.as_ptr().expose_provenance();
    for (live, destroys) in [(1, 1), (2, 1)] {
        let mut guard = CARRIER.client.relock();
        fresh(&mut guard, 1).peers.insert(
            SERVER,
            Peer {
                socket,
                ..peer(true)
            },
        );
        drop(guard);
        forget();
        assert!(CARRIER.client.relock().is_none(), "the link is forgotten");
        close_retired(&game, live);
        assert!(CARRIER.retired.relock().is_empty(), "looked at once");
        assert_eq!(
            DESTROYS.load(Ordering::Relaxed),
            destroys,
            "closed in the session it was opened in, left to another"
        );
    }
}
