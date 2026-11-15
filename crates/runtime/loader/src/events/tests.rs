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
    let _turn = crate::engine::log::capture_turn();
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
    let step = listed.iter().find(|p| p.part == STEP_FUSE.feature());
    assert!(
        step.is_some_and(|p| p.reason.starts_with(pinned::server::ENTRY_STATE.id())),
        "{listed:?}"
    );
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
