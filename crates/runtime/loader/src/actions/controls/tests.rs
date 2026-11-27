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
fn threads_declaring_one_control_at_once_keep_exactly_one() {
    static DECLARED: Registry<Declared> = Registry::new();
    const THREADS: usize = 8;
    let start = std::sync::Barrier::new(THREADS);
    let answers: Vec<_> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..THREADS)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    declare(&DECLARED, declared(""))
                })
            })
            .collect();
        threads.into_iter().filter_map(|t| t.join().ok()).collect()
    });
    let kept = answers.iter().filter(|answer| answer.is_ok()).count();
    let twice = answers
        .iter()
        .filter(|answer| **answer == Err(DeclareControlError::Twice))
        .count();
    assert_eq!((kept, twice), (1, THREADS - 1), "{answers:?}");
    assert_eq!(DECLARED.seal().len(), 1);
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
    crate::engine::log::capture();
    press("you.weather", "day");
    let lines = crate::engine::log::captured();
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

#[test]
fn a_given_up_key_file_leaves_the_keys_in_force_equal_to_the_file() {
    let dir = std::env::temp_dir().join(format!("enshwick-controls-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(bindings::FILE);
    let kept: toml::Table = "\"you.server:dash\" = [3]\n".parse().unwrap();
    let entries = [("you.test:jump".to_owned(), [Some(7), None])];
    std::fs::write(&path, bindings::render(&entries, &kept)).unwrap();
    let jump = [declared("")];
    let dash = Heard {
        mod_id: "you.server".to_owned(),
        mod_name: "Server".to_owned(),
        control: Carried {
            name: "dash".to_owned(),
            label: "Dash".to_owned(),
            action: "dash".to_owned(),
            keys: [5, 0],
            fires: 1,
            hold_ms: 0,
        },
    };
    STATE.keys.store(Some(Arc::new(Bound {
        keys: vec![[9, 0], [6, 0]],
        carried: vec![dash.clone()],
        carried_at: 4,
    })));
    *STATE.kept.relock() = Some(toml::Table::new());

    key_file::reread(&jump, &path);
    let bound = STATE.keys.load_full().unwrap();
    assert_eq!(
        bound.keys.first(),
        Some(&[7, 0]),
        "the rebind kept in memory is gone"
    );
    assert_eq!(bound.carried, [dash], "the server's controls stay");
    assert_eq!(bound.carried_at, 4, "no carried control restarts");
    assert_eq!(
        STATE.kept.relock().as_ref(),
        Some(&kept),
        "a later rebind writes the file's other entries"
    );

    // A folder does not read as a file, as one a program holds unshared.
    key_file::reread(&jump, &dir);
    assert_eq!(
        STATE.keys.load_full().unwrap().keys.first(),
        Some(&[7, 0]),
        "a file that does not read keeps the keys in force"
    );
    assert_eq!(
        STATE.kept.relock().as_ref(),
        Some(&kept),
        "and the entries a rebind writes"
    );
    let _gone = std::fs::remove_dir_all(&dir);
}
