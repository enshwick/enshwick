use super::*;

#[test]
fn the_players_are_read_on_every_step_without_a_list_and_every_thirty_with_one() {
    assert!(read_due(1, false), "the first step after a world loads");
    assert!(read_due(7, false));
    assert!(!read_due(7, true));
    assert!(read_due(PLAYERS_EVERY, true));
    assert!(read_due(2 * PLAYERS_EVERY, true));
}

#[test]
fn the_filters_are_wholly_off_when_their_stock_link_fails() {
    use enshwick_base::Relock;

    let loader = crate::start::tests::bare_loader();
    let found = [
        (pinned::access::OFFER_TEST, 0x1000),
        (pinned::access::TRANSFER, 0x2000),
        (pinned::access::SORT, 0x3000),
        (pinned::access::DISMANTLE_METHOD, 0x4000),
    ];
    let at = |symbol: Symbol| {
        let address = found
            .iter()
            .find(|(s, _)| s.id() == symbol.id())
            .map(|&(_, address)| address);
        Target::Found(symbol.id(), address)
    };
    assert!(!link_filters(&loader, at, HookFlags::NONE));
    assert!(INTERACTIONS_FUSE.is_off());
    assert!(filter_off(), "a hold keeps no one out");
    {
        let mut chains = loader.chains.relock();
        chains.seal();
        assert!(
            chains.is_empty(),
            "the offer, transfer and sort links go with the stock's, never patched"
        );
    }
    let listed = crate::engine::diag::parts_off();
    let off = listed.iter().find(|p| p.part == "interaction filters");
    assert!(
        off.is_some_and(|p| p.reason.starts_with("access.base_stock ")),
        "{listed:?}"
    );
}

#[test]
fn a_build_without_the_component_reads_names_the_first_entry_it_lacks() {
    let loader = crate::start::tests::bare_loader();
    assert_eq!(
        Components::found(&loader).err(),
        Some(pinned::ecs::TYPE_INDEX.id())
    );
}

#[test]
fn a_build_without_the_layout_lists_the_filters_actions_and_holds_off() {
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
    let state = State::new();
    state.reads_inputs.store(true, Ordering::Release);
    assert!(!link_in(loader, &state), "no hold is ready");
    assert!(INTERACTIONS_FUSE.is_off() && ACTIONS_FUSE.is_off() && HOLDS_FUSE.is_off());
    assert!(!state.reads_inputs.load(Ordering::Acquire), "never linked");
    assert_eq!(
        crate::base::off_reason(&HOLDS_FUSE).as_deref(),
        Some("interaction filters is off")
    );
}

#[test]
fn props_open_to_players_name_the_link_that_is_off_without_a_build_code() {
    let turn = crate::engine::log::capture_turn();
    HOLDS_FUSE.turn_off(&holds_off(&ACTIONS_FUSE));
    let lines = crate::engine::log::captured();
    drop(turn);
    assert!(
        lines.iter().any(
            |(_, _, line)| line == "prop removal by transactions off: inventory actions is off"
        ),
        "{lines:?}"
    );
}

#[test]
fn a_link_made_at_link_time_counts_only_while_its_fuse_is_on() {
    let fuse = Fuse::new("test inventory actions");
    assert!(!still_linked(false, &fuse), "never made");
    assert!(still_linked(true, &fuse));
    fuse.blow("a test");
    assert!(!still_linked(true, &fuse), "turned off after it was made");
}

#[test]
fn the_players_input_is_read_only_while_its_link_is_on() {
    let state = State::new();
    let fuse = Fuse::new("test inventory actions");
    assert!(!state.reads_inputs(&fuse), "never linked");
    state.reads_inputs.store(true, Ordering::Release);
    assert!(state.reads_inputs(&fuse));
    fuse.blow("a test");
    assert!(
        !state.reads_inputs(&fuse),
        "a link turned off after it was made"
    );
}

#[test]
fn a_hosted_game_s_end_forgets_its_players_and_keeps_the_link() {
    let state = State::new();
    let fuse = Fuse::new("test inventory actions");
    state.reads_inputs.store(true, Ordering::Release);
    state.players.store(Some(std::sync::Arc::new(vec![7, 9])));
    state.forget();
    assert!(state.players.load().is_none(), "the next world reads anew");
    assert!(state.reads_inputs(&fuse), "the link outlives the game");
}
