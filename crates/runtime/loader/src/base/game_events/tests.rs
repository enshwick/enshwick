use super::*;

unsafe extern "system" fn unused(_event: *const GameEventData, _user: *mut c_void) {}

fn subscriber(whose: Whose, type_hash: u32) -> Subscriber {
    Subscriber {
        whose,
        owner: ModIndex::new(0),
        type_hash,
        callback: unused,
        user: 0,
    }
}

fn sealed_with(subscribers: &[Subscriber]) -> Sealed {
    let (server, client) = by_world(server_first(subscribers.to_vec()).leak());
    Sealed {
        server,
        client,
        equipment: None,
        screened: false,
    }
}

fn copied(type_hash: u32, bytes: &[u8], into: &mut Batch) {
    into.events.push(Copied {
        type_hash,
        size: u32::try_from(bytes.len()).unwrap(),
        start: into.bytes.len(),
    });
    into.bytes.extend_from_slice(bytes);
}

fn payloads(batch: &Batch) -> Vec<(u32, Vec<u8>)> {
    batch
        .events
        .iter()
        .map(|e| {
            let end = e.start + e.size as usize;
            (e.type_hash, batch.bytes[e.start..end].to_vec())
        })
        .collect()
}

#[test]
fn a_dedicated_server_answers_a_client_handler_without_keeping_it() {
    let registry = Registry::new();
    let client = subscriber(Whose::Client, 7);
    assert_eq!(keep(&registry, Side::SERVER, true, client), Ok(()));
    assert_eq!(
        keep(&registry, Side::SERVER, true, subscriber(Whose::Server, 7)),
        Ok(())
    );
    assert_eq!(keep(&registry, Side::CLIENT, true, client), Ok(()));
    let kept: Vec<Whose> = registry.seal().iter().map(|s| s.whose).collect();
    assert_eq!(kept, [Whose::Server, Whose::Client]);
}

#[test]
fn a_handler_registered_outside_its_mod_s_init_is_too_late_on_either_world() {
    let registry = Registry::new();
    for side in [Side::CLIENT, Side::SERVER] {
        for whose in [Whose::Server, Whose::Client] {
            assert_eq!(
                keep(&registry, side, false, subscriber(whose, 7)),
                Err(JoinFeatureError::TooLate),
                "{whose:?}"
            );
        }
    }
    assert!(registry.seal().is_empty());
}

#[test]
fn a_handler_registered_in_init_after_the_seal_is_too_late_and_not_kept() {
    let registry = Registry::new();
    let _sealed = registry.seal();
    for side in [Side::CLIENT, Side::SERVER] {
        assert_eq!(
            keep(&registry, side, true, subscriber(Whose::Server, 7)),
            Err(JoinFeatureError::TooLate),
            "{side:?}"
        );
    }
    assert!(registry.seal().is_empty());
}

#[test]
fn each_world_keeps_its_handlers_in_the_order_mods_registered_them() {
    let sealed = sealed_with(&[
        subscriber(Whose::Client, 1),
        subscriber(Whose::Server, 2),
        subscriber(Whose::Client, 3),
        subscriber(Whose::Server, 4),
    ]);
    let hashes =
        |wanted: &Wanted| -> Vec<u32> { wanted.subscribers.iter().map(|s| s.type_hash).collect() };
    assert_eq!(hashes(&sealed.server), [2, 4]);
    assert_eq!(hashes(&sealed.client), [1, 3]);
}

#[test]
fn each_world_copies_only_what_its_own_handlers_want() {
    let sealed = sealed_with(&[subscriber(Whose::Server, 42), subscriber(Whose::Client, 7)]);
    assert!(ServerScope::copies(&sealed, 42));
    assert!(!ServerScope::copies(&sealed, 7));
    assert!(ClientScope::copies(&sealed, 7));
    assert!(!ClientScope::copies(&sealed, 42));
    assert!(!ClientScope::idle(&sealed));
    let every = sealed_with(&[subscriber(Whose::Client, 0)]);
    assert!(ClientScope::copies(&every, 99));
    assert!(!ServerScope::copies(&every, 99));
    let hashes = |wanted: &Wanted| {
        wanted
            .subscribers
            .iter()
            .map(|s| s.type_hash)
            .collect::<Vec<_>>()
    };
    assert_eq!(hashes(ServerScope::wanted(&sealed)), [42]);
    assert_eq!(hashes(ClientScope::wanted(&sealed)), [7]);
}

#[test]
fn a_server_world_nothing_wants_is_idle() {
    let client_only = sealed_with(&[subscriber(Whose::Client, 7)]);
    assert!(
        ServerScope::idle(&client_only),
        "no handler, stats, filter or item action"
    );
    assert!(!ServerScope::idle(&sealed_with(&[subscriber(
        Whose::Server,
        42
    )])));
    let equipment = Sealed {
        equipment: Some(43),
        ..sealed_with(&[])
    };
    assert!(!ServerScope::idle(&equipment));
    let screened = Sealed {
        screened: true,
        ..sealed_with(&[])
    };
    assert!(
        !ServerScope::idle(&screened),
        "a craft filter or item action reads them"
    );
}

#[test]
fn a_client_with_only_server_handlers_copies_nothing_of_its_own() {
    let sealed = Sealed {
        equipment: Some(43),
        ..sealed_with(&[subscriber(Whose::Server, 0)])
    };
    assert!(ClientScope::idle(&sealed));
    assert!(!ClientScope::copies(&sealed, 43));
    assert!(ServerScope::copies(&sealed, 99));
}

#[test]
fn the_worlds_batches_are_apart_and_events_copied_meanwhile_stay_bounded() {
    let mut server = ServerScope::batch().take();
    copied(1, b"server", &mut server);
    give_back::<ServerScope>(server);
    let mut client = ClientScope::batch().take();
    copied(2, b"client", &mut client);
    give_back::<ClientScope>(client);
    assert_eq!(
        payloads(&ServerScope::batch().take()),
        [(1, b"server".to_vec())]
    );
    assert_eq!(
        payloads(&ClientScope::batch().take()),
        [(2, b"client".to_vec())]
    );
    let mut outer = Batch::default();
    for _ in 0..CLIENT_EVENTS {
        copied(3, b"x", &mut outer);
    }
    CLIENT_BATCH.with_borrow_mut(|held| copied(4, b"y", held));
    give_back::<ClientScope>(outer);
    let back = ClientScope::batch().take();
    assert_eq!(back.events.len(), CLIENT_EVENTS);
    assert!(back.bytes.len() <= CLIENT_BYTES);
}

#[test]
fn a_batch_taken_out_leaves_the_cell_free_and_keeps_events_copied_meanwhile() {
    let mut first = ServerScope::batch().take();
    copied(1, b"ab", &mut first);
    BATCH.with_borrow_mut(|held| copied(2, b"cde", held));
    give_back::<ServerScope>(first);
    let back = ServerScope::batch().take();
    assert_eq!(payloads(&back), [(1, b"ab".to_vec()), (2, b"cde".to_vec())]);
    give_back::<ServerScope>(Batch::default());
    BATCH.with_borrow(|held| assert!(held.is_empty()));
}

#[test]
fn a_build_without_the_stream_layout_lists_only_the_delivery_off_by_the_entry_it_lacks() {
    let loader = crate::start::tests::bare_loader();
    assert!(stream_layout(&loader, true).is_none());
    assert!(DELIVERY_FUSE.is_off());
    assert!(!EVENTS_FUSE.is_off(), "the move still links for the rest");
    let reason = crate::base::off_reason(&DELIVERY_FUSE).unwrap_or_default();
    assert!(
        reason.starts_with(pinned::ecs::EVENT_STREAMS.id()),
        "{reason}"
    );
}

#[test]
fn a_client_build_without_the_stream_layout_lists_its_client_events_off_too() {
    let loader = crate::start::tests::pinning_loader(Side::CLIENT, &[], &[]);
    assert!(stream_layout(&loader, false).is_none());
    assert!(
        !CLIENT_FUSE.is_off(),
        "no mod asked for client events, so none is listed off"
    );
    assert!(stream_layout(&loader, true).is_none());
    assert!(DELIVERY_FUSE.is_off());
    assert!(
        CLIENT_FUSE.is_off(),
        "a client mod's handler would never run"
    );
    let reason = crate::base::off_reason(&CLIENT_FUSE).unwrap_or_default();
    assert!(
        reason.starts_with(pinned::ecs::EVENT_STREAMS.id()),
        "{reason}"
    );
}
