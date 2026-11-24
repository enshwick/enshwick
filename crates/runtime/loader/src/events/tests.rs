use super::*;
use enshwick_base::Relock;

#[test]
fn a_hosted_game_s_end_forgets_the_joined_players_and_the_queued_building_events() {
    STATE.joined[3].store(42, Ordering::Relaxed);
    STATE.building.push(bytemuck::Zeroable::zeroed());
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
    Part::forget_in(&[&PART], "events", loader);
    assert!(
        STATE
            .joined
            .iter()
            .all(|joined| joined.load(Ordering::Relaxed) == 0)
    );
    assert!(STATE.building.take().is_empty());
}

#[test]
fn a_build_without_a_player_slot_constant_still_steps_and_one_without_the_world_does_not() {
    use super::offsets::{Found, WorldAt};
    let found = Found {
        entry_state: Ok(1),
        slots: Err(pinned::server::PLAYER_SLOT_STATE),
        world: Ok(WorldAt {
            server_world: 1,
            entities: 2,
        }),
        building: Err(pinned::building::PICKUP_MODE),
    };
    assert_eq!(found.step(), None, "only the joins need the slots");
    let found = Found {
        world: Err(pinned::world::ENTITIES),
        ..found
    };
    assert_eq!(found.step(), Some(pinned::world::ENTITIES));
}

#[test]
fn a_player_whose_slot_clears_or_is_reused_unseen_leaves_once_before_the_next_joins() {
    use SlotScan::{Joined, Left, Replaced, Same};
    let in_game = true;
    assert_eq!(scan_slot(0, 0, false), Same, "an empty slot");
    assert_eq!(scan_slot(0, 7, !in_game), Same, "a player on the way in");
    assert_eq!(scan_slot(0, 7, in_game), Joined);
    assert_eq!(scan_slot(7, 7, in_game), Same, "joined once");
    assert_eq!(scan_slot(7, 7, !in_game), Same, "the removal link ends it");
    assert_eq!(scan_slot(7, 0, false), Left, "the slot cleared unseen");
    assert_eq!(
        scan_slot(7, 9, !in_game),
        Left,
        "reused, the new one not in yet"
    );
    assert_eq!(scan_slot(7, 9, in_game), Replaced, "reused within one step");
    assert_eq!(
        scan_slot(0, 0, false),
        Same,
        "after the removal link zeroed the record, no second leave"
    );
}

#[test]
fn a_placed_piece_s_position_widens_the_game_s_three_floats_without_reading_past_them() {
    let floats: [f32; 3] = [1.5, -2.25, 1024.0];
    // SAFETY: `floats` is a live three-float array, as the game passes.
    let placed = unsafe { placed_at(&raw const floats) };
    assert_eq!(placed, [1.5, -2.25, 1024.0]);
    // SAFETY: a null position is allowed by `placed_at`.
    let origin = unsafe { placed_at(core::ptr::null()) };
    assert_eq!(origin, [0.0; 3]);
}

#[test]
fn each_link_is_a_feature_of_its_own_named_once() {
    let names = [
        &STEP_FUSE,
        &STATE_FUSE,
        &SAVE_FUSE,
        &REMOVE_FUSE,
        &PLACE_FUSE,
        &PICKUP_FUSE,
        &STEP_LINK,
        &STATE_LINK,
        &REMOVE_LINK,
        TRIES.fuse,
        REMEMBERED.fuse,
        TICK_EVENTS.fuse,
        PERIODIC_TICKS.fuse,
        JOINED_EVENTS.fuse,
    ]
    .map(Fuse::feature);
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "{names:?}");
}

#[test]
fn a_panic_in_an_event_s_work_leaves_the_link_that_carries_the_engine_s_work_wired() {
    let engine = [
        (pinned::server::POST_STEP, &STEP_LINK),
        (pinned::server::SET_STATE, &STATE_LINK),
        (pinned::server::PLAYER_REMOVE, &REMOVE_LINK),
    ];
    let symbols = engine.map(|(symbol, _)| symbol);
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::pinning_loader(
        Side::SERVER,
        &symbols,
        &[],
    )));
    assert!(
        link_server(loader, [true, true, false, true]),
        "the step links"
    );
    for events in [&STEP_FUSE, &STATE_FUSE, &REMOVE_FUSE] {
        events.blow("a test's event work");
    }
    let chains = loader.chains.relock();
    for (symbol, link) in engine {
        let at = chains.features_at(loader.address(symbol).unwrap());
        assert!(
            at.len() == 1 && core::ptr::eq(at[0], link) && !link.is_off(),
            "{symbol:?} stays linked under its own fuse"
        );
    }
}

#[test]
fn the_server_s_lifetime_work_runs_again_at_the_next_change_after_a_panic() {
    crate::engine::log::capture();
    let ran = core::cell::Cell::new(0);
    for _ in 0..2 {
        each_change("a test's state link", || {
            ran.set(ran.get() + 1);
            panic!("the teardown broke");
        });
    }
    assert_eq!(ran.get(), 2, "never turned off");
    let logged = crate::engine::log::captured();
    let panics = logged
        .iter()
        .filter(|(_, _, line)| line.contains("the teardown broke"))
        .count();
    assert_eq!(panics, 2, "{logged:?}");
}

#[test]
fn a_build_without_the_server_layout_turns_off_each_link_it_would_make_with_its_entry() {
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::pinning_loader(
        Side::SERVER,
        &[pinned::server::POST_STEP, pinned::server::SET_STATE],
        &[],
    )));
    link(loader);
    assert!(STEP_FUSE.is_off() && STATE_FUSE.is_off());
    assert!(
        !SAVE_FUSE.is_off() && !PLACE_FUSE.is_off(),
        "a link no mod asked for stays out of the list"
    );
    let listed = crate::engine::diag::parts_off();
    let reason = |fuse: &Fuse| {
        listed
            .iter()
            .find(|p| p.part == fuse.feature())
            .map_or_else(String::new, |p| p.reason.clone())
    };
    assert!(
        reason(&STEP_FUSE).starts_with(pinned::server::WORLD.id()),
        "the step names the world it captures: {listed:?}"
    );
    assert!(
        reason(&REMOVE_FUSE).starts_with(pinned::server::PLAYER_SLOTS.id()),
        "the slots, which only the joins and leaves read, under theirs: {listed:?}"
    );
    assert!(
        reason(&STATE_FUSE).starts_with(pinned::server::ENTRY_STATE.id()),
        "{listed:?}"
    );
}

/// Every server and world constant the links read, but the building ones.
const ALL_BUT_BUILDING: &[(Constant, u64)] = &[
    (pinned::server::ENTRY_STATE, 8),
    (pinned::server::PLAYER_SLOTS, 16),
    (pinned::server::PLAYER_SLOT_SIZE, 32),
    (pinned::server::PLAYER_SLOT_COUNT, 16),
    (pinned::server::PLAYER_SLOT_STATE, 4),
    (pinned::server::PLAYER_SLOT_ENTITY, 8),
    (pinned::server::PLAYER_SLOT_KEY, 16),
    (pinned::server::WORLD, 24),
    (pinned::world::ENTITIES, 32),
];

#[test]
fn a_client_without_a_building_constant_keeps_its_state_link_s_notices() {
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::pinning_loader(
        Side::CLIENT,
        &[],
        ALL_BUT_BUILDING,
    )));
    link(loader);
    let notices = listed_off("server notices").unwrap_or_default();
    assert!(
        !notices.starts_with(pinned::building::CTX_PLAYER.id()),
        "an unrelated building constant names nothing off: {notices}"
    );
    assert!(!STATE_FUSE.is_off(), "the state link's offsets are there");
}

#[test]
fn a_client_without_the_entry_state_lists_its_notices_off_naming_it() {
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::pinning_loader(
        Side::CLIENT,
        &[],
        &ALL_BUT_BUILDING[1..],
    )));
    link(loader);
    let notices = listed_off("server notices").unwrap_or_default();
    assert!(
        notices.starts_with(pinned::server::ENTRY_STATE.id()),
        "{notices}"
    );
    assert!(STATE_FUSE.is_off());
    assert!(!REMOVE_FUSE.is_off(), "the removal has its offsets");
}

#[test]
fn building_events_without_the_step_s_link_are_listed_off_for_the_step() {
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
    link_building(loader, false, true, false);
    assert!(PLACE_FUSE.is_off());
    assert!(!PICKUP_FUSE.is_off(), "no mod asked for the other");
    let listed = crate::engine::diag::parts_off();
    let place = listed.iter().find(|p| p.part == PLACE_FUSE.feature());
    assert!(
        place.is_some_and(|p| p.reason.starts_with(pinned::server::POST_STEP.id())),
        "{listed:?}"
    );
    assert!(loader.chains.relock().is_empty(), "nothing linked");
}

/// The reason the Mods screen lists for the loader feature named `feature`.
fn listed_off(feature: &str) -> Option<String> {
    crate::engine::diag::parts_off()
        .into_iter()
        .find(|part| part.part == feature)
        .map(|part| part.reason)
}

#[test]
fn a_client_without_the_state_link_lists_its_notices_off_for_the_state_link() {
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::pinning_loader(
        Side::CLIENT,
        &[],
        &[],
    )));
    let _stepped = link_server(loader, [false, true, false, false]);
    let notices = listed_off("server notices").unwrap_or_default();
    assert!(
        notices.starts_with(pinned::server::SET_STATE.id()),
        "{notices}"
    );
    assert_eq!(
        listed_off("client game events"),
        None,
        "no mod asked for client events, so none is listed off"
    );
}

#[test]
fn a_dedicated_server_without_the_state_link_keeps_its_notices() {
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::pinning_loader(
        Side::SERVER,
        &[],
        &[],
    )));
    let _stepped = link_server(loader, [false, true, false, false]);
    assert_eq!(listed_off("server notices"), None);
}

#[test]
fn a_server_leaving_the_game_s_list_frees_its_hosted_thread() {
    const CLIENT_WORLD: usize = 0x2000; // bound, not a game offset: a test's own world address
    crate::engine::world::server_starts();
    assert_eq!(
        crate::engine::world::whose(Side::CLIENT, CLIENT_WORLD),
        None
    );
    after_change("a test", STATE_TEAR_DOWN);
    assert_eq!(
        crate::engine::world::whose(Side::CLIENT, CLIENT_WORLD),
        None,
        "another state keeps it"
    );
    after_change("a test", STATE_NONE);
    assert_eq!(
        crate::engine::world::whose(Side::CLIENT, CLIENT_WORLD),
        Some(crate::engine::world::Whose::Client)
    );
}

#[test]
fn read_only_runs_found_after_the_shutdown_are_forgotten_at_the_teardown_and_the_next_start() {
    use crate::engine::pages::LASTING;
    use windows_sys::Win32::System::Memory::{
        MEM_RELEASE, PAGE_READONLY, PAGE_READWRITE, VirtualFree,
    };
    let page = crate::engine::pages::allocate(None, LASTING).expect("fresh pages");
    let start = page.as_ptr().expose_provenance();
    for state in [STATE_TEAR_DOWN, STATE_INITIALIZE] {
        // SAFETY: the pages are the test's own allocation, used by no one else.
        unsafe { crate::engine::pages::protect(start, LASTING, PAGE_READONLY) }.expect("read-only");
        assert_eq!(
            crate::engine::pages::read_only(start, 16),
            16,
            "found in the final save"
        );
        // SAFETY: the pages are the test's own allocation, used by no one else.
        unsafe { crate::engine::pages::protect(start, LASTING, PAGE_READWRITE) }
            .expect("read-write");
        forget_pages_entering(state);
        assert_eq!(
            crate::engine::pages::read_only(start, 16),
            0,
            "state {state}"
        );
    }
    // SAFETY: `page` is the test's own allocation, released once.
    unsafe { VirtualFree(page.as_ptr().cast(), 0, MEM_RELEASE) };
}

#[test]
fn a_world_loads_its_settings_on_a_server_whose_mods_subscribe_to_nothing() {
    assert!(own_link(pinned::server::SET_STATE, false));
    assert!(own_link(pinned::server::POST_STEP, false));
    assert!(!own_link(pinned::server::SAVE_STEP, true));
}

#[test]
fn a_player_leaving_reaches_the_parts_that_hear_it_with_no_mod_subscribed() {
    assert!(own_link(pinned::server::PLAYER_REMOVE, true));
    assert!(!own_link(pinned::server::PLAYER_REMOVE, false));
    let parts = crate::parts::parts();
    assert!(parts.iter().any(|part| part.left.is_some()));
}
