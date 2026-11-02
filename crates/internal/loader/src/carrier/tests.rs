use super::*;

/// Serialises the tests, which share the carrier's state.
static SHARED: Mutex<()> = Mutex::new(());

fn alone() -> std::sync::MutexGuard<'static, ()> {
    let guard = SHARED.relock();
    forget();
    *CARRIER.server.relock() = None;
    guard
}

#[test]
fn a_hosted_message_waits_until_its_half_takes_it_whole() {
    let _alone = alone();
    assert!(loop_back(true, 5, b"to the server").is_ok());
    assert!(loop_back(false, 6, b"to the player").is_ok());
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
        assert!(loop_back(true, 5, b"x").is_ok());
    }
    assert_eq!(loop_back(true, 5, b"x"), Err(Status::REFUSED));
    CARRIER.host_key.store(9, Ordering::Release);
    CARRIER.hosted_hello.store(true, Ordering::Release);
    assert!(loop_back(false, 6, b"y").is_ok());
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
