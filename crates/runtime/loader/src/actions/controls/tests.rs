use super::*;

unsafe extern "system" fn fired(_: *const ControlEvent, _: *mut c_void) {}

fn declared(link: &str) -> Declared {
    Declared {
        owner: ModIndex::new(0),
        id: "you.test:jump".to_owned(),
        name: "jump".to_owned(),
        label: "Jump".to_owned(),
        defaults: [0; 2],
        link: link.to_owned(),
        action: String::new(),
        fires: 1,
        hold_ms: 0,
        callback: fired,
        user: 0,
    }
}

#[test]
fn a_game_s_end_leaves_no_firing_waiting_for_the_next() {
    let state = State::new();
    state.firing.store(true, Ordering::Release);
    state.forget();
    assert!(!state.firing.load(Ordering::Acquire));
    assert!(!state.players.relock().waiting());
}

#[test]
fn controls_that_follow_game_actions_are_listed_off_when_the_input_system_did_not_link() {
    let loader = crate::start::tests::bare_loader();
    link_actions(&loader, &[declared("")]);
    assert!(!LINKED_FUSE.is_off(), "no control follows a game action");
    link_actions(&loader, &[declared(""), declared("Jump")]);
    assert!(LINKED_FUSE.is_off());
    assert!(!linked());
    let listed = crate::engine::diag::parts_off();
    let off = listed.iter().find(|p| p.part == LINKED_FUSE.feature());
    assert!(
        off.is_some_and(|p| p
            .reason
            .starts_with(crate::actions::interactions::ACTIONS_SYSTEM)),
        "{listed:?}"
    );
}

#[test]
fn a_press_the_server_cannot_take_leaves_a_warning_in_the_log() {
    let turn = crate::engine::log::capture_turn();
    press("you.weather", "day");
    let lines = crate::engine::log::captured();
    drop(turn);
    assert!(
        lines
            .iter()
            .any(|(level, _, line)| *level == enshwick_core::abi::Level::WARN
                && line.contains("you.weather day was not sent")),
        "{lines:?}"
    );
}

#[test]
fn controls_follow_game_actions_only_while_every_link_they_need_is_on() {
    let state = State::new();
    let fuse = Fuse::new("test controls that follow game actions");
    let reads = || true;
    assert!(
        !state.follows(&fuse, || panic!("asked before the cheap checks")),
        "no control follows a game action"
    );
    let _first = state.linked.set(Linked {
        digital: 0,
        follows: Vec::new(),
    });
    assert!(
        !state.follows(&fuse, || false),
        "the input system never linked"
    );
    assert!(state.follows(&fuse, reads));
    fuse.blow("a test");
    assert!(!state.follows(&fuse, reads), "their own fuse is off");
}
