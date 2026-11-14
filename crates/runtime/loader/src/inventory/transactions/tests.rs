use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use super::checks::{fill, valid};
use super::*;

unsafe extern "system" fn nothing(_result: *const InventoryResult, _user: *mut c_void) {}

fn request(user: usize) -> Request {
    Request {
        owner: ModIndex::new(0),
        flags: 0,
        actor: 0,
        ops: Vec::new(),
        done: nothing,
        user,
        queued: Instant::now(),
    }
}

fn ok(remaining: u32) -> InventoryOpResult {
    InventoryOpResult {
        error: 0,
        remaining,
    }
}

#[test]
fn a_transaction_commits_only_when_every_op_ran_and_finished() {
    assert!(commits(&[ok(0), ok(0)], 2, false));
    assert!(!commits(&[ok(0), ok(3)], 2, false));
    assert!(commits(&[ok(0), ok(3)], 2, true));
    let failed = InventoryOpResult {
        error: 0x15,
        remaining: 0,
    };
    assert!(!commits(&[ok(0), failed], 2, true));
    assert!(!commits(&[ok(0)], 2, true));
}

#[test]
fn each_request_is_claimed_exactly_once_across_threads() {
    let queue = Arc::new(Queue::new());
    for user in 1..=1000 {
        queue.push(request(user));
    }
    let seen: Arc<Vec<AtomicU32>> = Arc::new((0..=1000).map(|_| AtomicU32::new(0)).collect());
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let (queue, seen) = (Arc::clone(&queue), Arc::clone(&seen));
            std::thread::spawn(move || {
                while let Some(request) = queue.claim() {
                    seen[request.user].fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert!(seen[1..].iter().all(|n| n.load(Ordering::Relaxed) == 1));
    assert!(!queue.pending.load(Ordering::Relaxed));
    assert!(queue.claim().is_none());
}

#[test]
fn every_table_offers_a_check_for_the_code_the_hook_relies_on() {
    for table in crate::engine::builds::load() {
        let resolved = table.resolve();
        for symbol in CHECKED {
            let bytes = format!("{symbol}.bytes");
            assert!(
                resolved.head(symbol.id()).is_some() || resolved.constant(&bytes).is_some(),
                "{}: {symbol}",
                table.name()
            );
        }
    }
}

#[test]
fn the_server_row_has_every_piece_the_executor_reads() {
    let Some(server) = crate::engine::builds::kept(Side::SERVER) else {
        return;
    };
    let symbols = [
        pinned::inventory::ACTIONS_ROW,
        pinned::inventory::VIEW_FILL,
        Symbol::new("inventory.skip_entity"),
        pinned::inventory::DESC_ZERO,
        pinned::inventory::ACTOR_FLAG,
        pinned::inventory::TXN_NEW,
        pinned::inventory::TXN_INIT,
        pinned::inventory::TXN_COMMIT,
        pinned::inventory::TXN_ROLLBACK,
        pinned::inventory::TXN_DROP,
        pinned::inventory::GIVE,
        pinned::inventory::TAKE,
        pinned::inventory::DESTROY_SLOT,
        pinned::inventory::MOVE_SLOT,
        pinned::inventory::MOVE_AMOUNT,
        pinned::inventory::MOVE_ALL,
        pinned::inventory::RESEND,
        pinned::item::INFO,
        pinned::ecs::SLOT_ROOT,
        pinned::ecs::ROW_ENTITY,
    ];
    for name in symbols {
        assert!(server.symbol(name.id()).is_some(), "{name}");
    }
    let constants = [
        "inventory.actions_row.bytes",
        "inventory.view_fill.bytes",
        pinned::inventory::VIEW_FRAME.id(),
        pinned::inventory::VIEW_OWNERS.id(),
        pinned::inventory::VIEW_ACTORS.id(),
        pinned::inventory::VIEW_REPLICATION.id(),
        pinned::inventory::VIEW_ITEM_REGISTRY.id(),
        pinned::inventory::DESC_SIZE.id(),
        pinned::inventory::DESC_CONTEXT.id(),
        pinned::inventory::DESC_ACTOR.id(),
        pinned::inventory::DESC_OTHER.id(),
        pinned::inventory::TXN_SIZE.id(),
    ];
    for name in constants {
        assert!(server.constant(name).is_some(), "{name}");
    }
    let copies = server
        .constants()
        .iter()
        .filter(|(name, _)| name.starts_with("inventory.copy_"))
        .count();
    assert_eq!(
        copies, 30,
        "a view and a descriptor offset for each of 15 fields"
    );
    assert!(server.constant(pinned::inventory::DESC_SIZE.id()).unwrap() <= DESC_AREA as u64);
}

#[test]
fn one_turn_at_a_time() {
    let busy = AtomicBool::new(false);
    let turn = Turn::take(&busy).unwrap();
    assert!(Turn::take(&busy).is_none());
    drop(turn);
    assert!(Turn::take(&busy).is_some());
}

#[test]
fn late_requests_drain_and_the_rest_stay_in_order() {
    let queue = Queue::new();
    for user in 1..=4 {
        queue.push(request(user));
    }
    let gone = queue.drain(|r| r.user % 2 == 0);
    assert_eq!(gone.iter().map(|r| r.user).collect::<Vec<_>>(), [2, 4]);
    assert_eq!(queue.claim().unwrap().user, 1);
    assert_eq!(queue.claim().unwrap().user, 3);
    assert!(queue.claim().is_none());
}

/// The outcome `state`'s results hold for the request with `user`, taking
/// them as the step's delivery would.
fn outcome_of(state: &State, user: usize) -> Option<u32> {
    state
        .finished
        .take()
        .iter()
        .find(|f| f.user == user)
        .map(|f| f.outcome)
}

#[test]
fn requests_queued_when_the_feature_turns_off_are_answered_unavailable_at_the_next_step() {
    let state = State::new();
    state.queue.push(request(7));
    INVENTORY_FUSE.blow("a test");
    state.expire(Instant::now());
    assert!(
        !state.queue.pending.load(Ordering::Acquire),
        "no row runs it now"
    );
    assert_eq!(outcome_of(&state, 7), Some(InventoryResult::UNAVAILABLE));
}

#[test]
fn a_wanted_row_hook_that_did_not_arm_turns_the_requests_off_and_answers_the_queue() {
    let state = State::new();
    state.wanted.store(true, Ordering::Release);
    state.queue.push(request(9));
    state.settle(&crate::start::tests::bare_loader(), []);
    assert!(
        INVENTORY_FUSE.is_off(),
        "off by its fuse, not only in the log"
    );
    assert!(!state.queue.pending.load(Ordering::Acquire));
    assert_eq!(outcome_of(&state, 9), Some(InventoryResult::UNAVAILABLE));
}

#[test]
fn an_unwanted_row_hook_leaves_the_requests_on_at_settling() {
    let state = State::new();
    state.settle(&crate::start::tests::bare_loader(), []);
    assert!(!INVENTORY_FUSE.is_off());
    assert!(state.settled.load(Ordering::Acquire));
    assert!(!state.armed.load(Ordering::Acquire));
}

#[test]
fn a_hosted_game_s_end_answers_its_requests_and_drops_its_registry() {
    let state = State::new();
    state.wanted.store(true, Ordering::Release);
    state.registry.store(0x1000, Ordering::Release);
    state.queue.push(request(5));
    state.end();
    assert_eq!(outcome_of(&state, 5), Some(InventoryResult::UNAVAILABLE));
    assert_eq!(
        state.registry.load(Ordering::Acquire),
        0,
        "the next world's registry is read anew"
    );
    assert!(state.wanted(), "what a mod asked for at init stays");
}

#[test]
fn a_build_without_the_checked_code_names_it_absent_not_moved() {
    let loader = crate::start::tests::bare_loader();
    let (symbol, why) = unchecked(&loader).unwrap();
    assert_eq!(symbol.id(), CHECKED[0].id());
    assert_eq!(why, crate::engine::builds::Unhooked::Absent);
}

#[test]
fn operations_must_name_what_they_need() {
    let give = InventoryOp {
        kind: InventoryOp::GIVE,
        target: 2,
        item: enshwick_core::ItemId::new(7),
        count: 1,
        ..InventoryOp::default()
    };
    assert!(valid(&give));
    assert!(!valid(&InventoryOp { count: 0, ..give }));
    assert!(!valid(&InventoryOp {
        count: u32::MAX,
        ..give
    }));
    assert!(!valid(&InventoryOp { kind: 99, ..give }));
    let filled = InventoryOp {
        flags: InventoryOp::FILLED,
        other: u64::from(0.5f32.to_bits()),
        ..give
    };
    assert!(valid(&filled));
    assert_eq!(
        fill(&filled).map(|f| (f[..4].to_vec(), f[4])),
        Some((0.5f32.to_le_bytes().to_vec(), 1))
    );
    assert!(!valid(&InventoryOp {
        other: u64::from(1.5f32.to_bits()),
        ..filled
    }));
    assert!(!valid(&InventoryOp {
        other: u64::from(f32::NAN.to_bits()),
        ..filled
    }));
    let all = InventoryOp {
        kind: InventoryOp::MOVE_ALL,
        target: 40,
        other: 2,
        ..InventoryOp::default()
    };
    assert!(valid(&all));
    assert!(!valid(&InventoryOp { other: 40, ..all }));
    let slot = InventoryOp {
        kind: InventoryOp::MOVE_SLOT,
        target: 0x29 | 3 << 32,
        other: 2,
        count: 70_000,
        ..InventoryOp::default()
    };
    assert!(!valid(&slot));
    assert!(valid(&InventoryOp { count: 5, ..slot }));
}

#[test]
fn the_holder_s_registry_stands_in_until_the_inventory_system_runs() {
    assert_eq!(pick_registry(0, 0x2000), (0x2000, false));
    assert_eq!(pick_registry(0x2000, 0x2000), (0x2000, false));
    assert_eq!(pick_registry(0x1000, 0), (0x1000, false));
    assert_eq!(pick_registry(0, 0), (0, false));
}

#[test]
fn registries_that_differ_keep_the_captured_one_and_say_so() {
    assert_eq!(pick_registry(0x1000, 0x2000), (0x1000, true));
}
