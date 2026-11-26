use super::*;
use enshwick_core::abi::Str;

mod chat_lines;
mod dialog_answers;
mod item_acts;
mod joins;

/// The frame's step while no command opens or closes a screen.
fn next(command: &Command, seen: &Seen, progress: Option<Progress>, now: u64) -> Next {
    super::next(command, seen, progress, now, false)
}

fn command(kind: Kind) -> Command {
    Command {
        id: 1,
        owner: ModIndex::new(0),
        kind,
        session: 3,
        expected: Some(1),
        held: Some(0),
        queued_ms: 100,
        state: Status::PENDING,
        reason: 0,
        handed: None,
        phase: 0,
        failure: 0,
    }
}

fn seen(screen: u8, tab: TabRead) -> Seen {
    Seen {
        session: 3,
        screen: Some(screen),
        tab,
        recipe: RecipeRead::Unread,
        held: Some(0),
        host: 0,
        loader: LoaderScreen::default(),
    }
}

#[test]
fn a_mods_command_is_observed_once_the_screen_shows_its_page() {
    let ask = |mod_index, gear| ModsAsk { mod_index, gear };
    let shown = |page, mod_index| Seen {
        loader: LoaderScreen { page, mod_index },
        ..seen(0, TabRead::NoMenu)
    };
    let open = Command {
        expected: None,
        ..command(Kind::Mods(ask(None, false)))
    };
    assert_eq!(next(&open, &shown(0, None), None, 150), Next::Apply);
    assert_eq!(
        next(&open, &shown(Snapshot::MOD_LIST, None), None, 160),
        Next::End(Status::OBSERVED, 0)
    );
    let page = ask(Some(2), false);
    assert!(page.shows(LoaderScreen {
        page: Snapshot::MOD_PAGE,
        mod_index: Some(2)
    }));
    assert!(!page.shows(LoaderScreen {
        page: Snapshot::MOD_SERVER_SETTINGS,
        mod_index: Some(2)
    }));
    assert!(
        ask(Some(2), true).shows(LoaderScreen {
            page: Snapshot::MOD_SERVER_SETTINGS,
            mod_index: Some(2)
        }),
        "a gear may open the joined server's settings"
    );
    assert!(!page.shows(LoaderScreen {
        page: Snapshot::MOD_PAGE,
        mod_index: Some(1)
    }));
}

/// The world's own screen, 1 on client 26430625.
fn world() -> Seen {
    seen(1, TabRead::NoMenu)
}

#[test]
fn a_look_is_handed_on_the_world_screen_and_observed_when_written() {
    let look = command(Kind::Look([0.5, 0.0]));
    assert_eq!(next(&look, &world(), None, 150), Next::Apply);
    let handed = Command {
        handed: Some(100),
        ..look
    };
    assert_eq!(
        next(&handed, &world(), Some(Progress::Waiting), 150),
        Next::Wait,
        "handed once"
    );
    assert_eq!(
        next(&handed, &world(), Some(Progress::Done), 160),
        Next::End(Status::OBSERVED, 0)
    );
}

#[test]
fn a_move_is_applied_while_it_holds_and_cancelled_by_a_press_or_another_screen() {
    let walk = Command {
        handed: Some(100),
        ..command(Kind::Move([0.0, 1.0], 250))
    };
    assert_eq!(
        next(&walk, &world(), Some(Progress::Taken), 150),
        Next::Applied
    );
    let moving = Command {
        state: Status::APPLIED,
        ..walk
    };
    let cancelled = |reason| Next::End(Status::CANCELLED, reason);
    let pressed = Seen {
        held: Some(1 << 3),
        ..world()
    };
    assert_eq!(
        next(&moving, &pressed, Some(Progress::Taken), 160),
        cancelled(Status::PLAYER_INPUT),
        "W pressed during the move"
    );
    assert_eq!(
        next(
            &moving,
            &seen(4, TabRead::NoMenu),
            Some(Progress::Taken),
            160
        ),
        cancelled(Status::OTHER_SCREEN),
        "a menu opened during the move"
    );
    assert_eq!(
        next(
            &moving,
            &world(),
            Some(Progress::Cancelled(Status::INPUT_BLOCKED)),
            160
        ),
        cancelled(Status::INPUT_BLOCKED)
    );
    assert_eq!(
        next(&moving, &world(), Some(Progress::Done), 400),
        Next::End(Status::OBSERVED, 0)
    );
}

#[test]
fn a_move_taken_late_keeps_its_hold_before_the_timeout() {
    let moving = Command {
        handed: Some(100),
        state: Status::APPLIED,
        ..command(Kind::Move([0.0, 1.0], 250))
    };
    let late = 100 + TIMEOUT_MS + 100;
    assert_eq!(
        next(&moving, &world(), Some(Progress::Taken), late),
        Next::Wait
    );
    assert_eq!(
        next(&moving, &world(), Some(Progress::Taken), late + 200),
        Next::End(Status::CANCELLED, Status::TIMED_OUT)
    );
}

#[test]
fn a_recipe_waits_for_the_browser_and_is_observed_once_it_is_highlighted() {
    let torch = RecipeId(0x7cd8_0a04);
    let highlight = Command {
        expected: None,
        ..command(Kind::Highlight(torch))
    };
    let browser = |recipe| Seen {
        recipe: RecipeRead::Shown(recipe),
        ..seen(8, TabRead::NoMenu)
    };
    assert_eq!(next(&highlight, &world(), None, 150), Next::Wait, "no menu");
    assert_eq!(next(&highlight, &browser(None), None, 150), Next::Apply);
    let handed = Command {
        handed: Some(100),
        ..highlight
    };
    assert_eq!(
        next(&handed, &browser(None), Some(Progress::Taken), 160),
        Next::Applied
    );
    let applied = Command {
        state: Status::APPLIED,
        ..handed
    };
    assert_eq!(
        next(&applied, &browser(Some(torch)), Some(Progress::Taken), 170),
        Next::End(Status::OBSERVED, 0)
    );
    assert_eq!(
        next(&handed, &browser(None), Some(Progress::Unavailable), 160),
        Next::End(Status::UNAVAILABLE, 0),
        "a recipe the open menu does not list"
    );
}

#[test]
fn a_look_or_move_past_its_bounds_and_on_another_screen_is_refused() {
    let loader = crate::start::tests::bare_loader();
    let kind = |spec: &Spec| kind(&loader, spec);
    let spec = |kind, look, direction, hold_ms| {
        enshwick_abi::headed!(Spec {
            kind,
            look,
            direction,
            hold_ms,
            ..Spec::EMPTY
        })
    };
    assert_eq!(
        kind(&spec(Spec::LOOK, [Spec::MAX_LOOK, -0.5], [0.0; 2], 0)),
        Ok(Kind::Look([Spec::MAX_LOOK, -0.5]))
    );
    for refused in [
        spec(Spec::LOOK, [Spec::MAX_LOOK + 0.01, 0.0], [0.0; 2], 0),
        spec(Spec::LOOK, [f32::NAN, 0.0], [0.0; 2], 0),
        spec(Spec::MOVE, [0.0; 2], [0.0, 1.0], 251),
        spec(Spec::MOVE, [0.0; 2], [0.0, 1.0], 0),
        spec(Spec::MOVE, [0.0; 2], [1.0, 1.0], 100),
        spec(Spec::MOVE, [0.0; 2], [0.0, 0.0], 100),
        enshwick_abi::headed!(Spec {
            kind: Spec::HIGHLIGHT_RECIPE,
            ..Spec::EMPTY
        }),
    ] {
        assert_eq!(kind(&refused), Err(CommandError::BadArgs), "{refused:?}");
    }
    assert_eq!(
        expected_screen(Kind::Move([0.0, 1.0], 100), "Backpack"),
        Err(CommandError::BadArgs)
    );
}

#[test]
fn a_press_needs_a_game_action_a_bounded_hold_and_the_build_s_bit() {
    let loader = crate::start::tests::bare_loader();
    let press = |name: &str, hold_ms| {
        enshwick_abi::headed!(Spec {
            kind: Spec::PRESS,
            target: Str::new(name),
            hold_ms,
            ..Spec::EMPTY
        })
    };
    for (spec, why) in [
        (press("Jump", 0), "no hold"),
        (press("Jump", Spec::MAX_PRESS_MS + 1), "past the bound"),
        (press("Dance", 16), "no game action"),
    ] {
        assert_eq!(kind(&loader, &spec), Err(CommandError::BadArgs), "{why}");
    }
    assert_eq!(
        kind(&loader, &press("Jump", Spec::MAX_PRESS_MS)),
        Err(CommandError::Unavailable),
        "a build that does not reflect the action"
    );
    assert_eq!(
        expected_screen(Kind::Press(32, 16, false), "Backpack"),
        Err(CommandError::BadArgs),
        "a press runs only on the world's screen"
    );
}

#[test]
fn a_mod_holds_one_press_per_action_beside_its_move() {
    let (jump, sprint) = (Kind::Press(32, 16, false), Kind::Press(53, 2000, false));
    assert!(jump.waits_beside(sprint));
    assert!(
        !jump.waits_beside(Kind::Press(32, 500, false)),
        "the same action"
    );
    assert!(sprint.waits_beside(Kind::Move([0.0, 1.0], 250)));
    assert!(!Kind::Move([0.0, 1.0], 250).waits_beside(Kind::Move([1.0, 0.0], 10)));
}

#[test]
fn a_mod_holds_its_share_of_presses_and_leaves_the_other_slots_to_other_mods() {
    let (me, other) = (ModIndex::new(0), ModIndex::new(1));
    let mut kept: VecDeque<Command> = (0..rules::PRESSES_PER_MOD)
        .map(|bit| command(Kind::Press(u8::try_from(bit).unwrap(), 2000, false)))
        .collect();
    let next = Kind::Press(40, 16, false);
    assert!(!may_queue(&kept, me, next), "its share is held");
    assert!(may_queue(&kept, other, next), "another mod's presses");
    assert!(may_queue(&kept, me, Kind::Move([0.0, 1.0], 250)));
    kept[0].state = Status::OBSERVED;
    assert!(may_queue(&kept, me, next), "one ended");
    assert!(
        !may_queue(&kept, me, Kind::Press(1, 16, false)),
        "one press of each action"
    );
}

#[test]
fn a_press_holds_until_done_and_ends_on_the_player_s_key_or_another_screen() {
    let press = Command {
        handed: Some(100),
        ..command(Kind::Press(32, 2000, false))
    };
    assert_eq!(next(&press, &world(), None, 150), Next::Apply);
    assert_eq!(
        next(&press, &world(), Some(Progress::Taken), 150),
        Next::Applied
    );
    let pressing = Command {
        state: Status::APPLIED,
        ..press
    };
    let cancelled = |reason| Next::End(Status::CANCELLED, reason);
    let keyed = Seen {
        held: Some(1 << 14),
        ..world()
    };
    assert_eq!(
        next(&pressing, &keyed, Some(Progress::Taken), 160),
        cancelled(Status::PLAYER_INPUT)
    );
    assert_eq!(
        super::next(
            &pressing,
            &seen(4, TabRead::NoMenu),
            Some(Progress::Taken),
            160,
            true
        ),
        cancelled(Status::OTHER_SCREEN),
        "a screen a command opened is not the press's"
    );
    assert_eq!(
        next(
            &press,
            &seen(4, TabRead::NoMenu),
            Some(Progress::Waiting),
            160
        ),
        cancelled(Status::OTHER_SCREEN),
        "a screen before the game took the press"
    );
    assert_eq!(
        next(
            &pressing,
            &world(),
            Some(Progress::Cancelled(Status::MOD_OFF)),
            160
        ),
        cancelled(Status::MOD_OFF)
    );
    let late = 100 + TIMEOUT_MS + 1000;
    assert_eq!(
        next(&pressing, &world(), Some(Progress::Taken), late),
        Next::Wait,
        "a press taken late keeps its whole hold"
    );
    assert_eq!(
        next(&pressing, &world(), Some(Progress::Done), late),
        Next::End(Status::OBSERVED, 0)
    );
}

#[test]
fn a_press_whose_held_action_opens_a_screen_mid_hold_ends_observed() {
    // An interaction held on a storage chest opens the chest's screen
    // before its 50 ms hold ends (client 26430625).
    let pressing = Command {
        handed: Some(100),
        state: Status::APPLIED,
        ..command(Kind::Press(14, 50, true))
    };
    let chest = Seen {
        host: 77,
        ..seen(0x15, TabRead::NoMenu)
    };
    assert_eq!(
        next(&pressing, &chest, Some(Progress::Taken), 110),
        Next::End(Status::OBSERVED, 0)
    );
    let map = seen(4, TabRead::Shown(MenuTab::Map));
    assert_eq!(
        next(&pressing, &map, Some(Progress::Taken), 110),
        Next::End(Status::CANCELLED, Status::OTHER_SCREEN),
        "the player's map key, outside the input, opens a screen with no host"
    );
    let jumping = Command {
        kind: Kind::Press(32, 2000, false),
        ..pressing
    };
    assert_eq!(
        next(&jumping, &chest, Some(Progress::Taken), 110),
        Next::End(Status::CANCELLED, Status::OTHER_SCREEN),
        "a jump opens no screen, so another's chest is not its effect"
    );
    let keyed = Seen {
        held: Some(1 << 9),
        ..chest
    };
    assert_eq!(
        next(&pressing, &keyed, Some(Progress::Taken), 110),
        Next::End(Status::CANCELLED, Status::PLAYER_INPUT),
        "the player's own key opened it"
    );
    let walking = Command {
        handed: Some(100),
        state: Status::APPLIED,
        ..command(Kind::Move([0.0, 1.0], 250))
    };
    assert_eq!(
        next(&walking, &chest, Some(Progress::Taken), 110),
        Next::End(Status::CANCELLED, Status::OTHER_SCREEN),
        "a move opens no screen of its own"
    );
}

#[test]
fn a_frame_marks_a_screen_a_command_applied_as_commanded_for_the_presses() {
    let mut commands = kept(Command {
        state: Status::APPLIED,
        expected: None,
        ..command(Kind::Open(4))
    });
    commands.kept.push_back(Command {
        id: 2,
        handed: Some(100),
        state: Status::APPLIED,
        ..command(Kind::Press(14, 50, true))
    });
    assert!(commanded(&commands));
    commands.kept[0].state = Status::OBSERVED;
    assert!(!commanded(&commands), "only while the open runs");
}

#[test]
fn a_press_that_waits_past_the_timeout_for_a_slot_ends_timed_out() {
    let waiting = command(Kind::Press(53, 2000, false));
    assert_eq!(
        next(&waiting, &world(), None, 100 + TIMEOUT_MS),
        Next::Apply,
        "every slot held, it asks again"
    );
    assert_eq!(
        next(&waiting, &world(), None, 100 + TIMEOUT_MS + 1),
        Next::End(Status::CANCELLED, Status::TIMED_OUT)
    );
}

#[test]
fn a_press_handed_late_holds_its_whole_hold_from_its_handing() {
    let handed_at = 100 + TIMEOUT_MS - 10;
    let pressing = Command {
        handed: Some(handed_at),
        state: Status::APPLIED,
        ..command(Kind::Press(53, 2000, false))
    };
    let held_out = handed_at + 16 + 2000;
    assert_eq!(
        next(&pressing, &world(), Some(Progress::Taken), held_out - 1),
        Next::Wait,
        "past the queue's timeout and hold, not the handing's"
    );
    assert_eq!(
        next(&pressing, &world(), Some(Progress::Done), held_out),
        Next::End(Status::OBSERVED, 0)
    );
    assert_eq!(
        next(
            &pressing,
            &world(),
            Some(Progress::Taken),
            handed_at + TIMEOUT_MS + u64::from(Spec::MAX_PRESS_MS) + 1
        ),
        Next::End(Status::CANCELLED, Status::TIMED_OUT),
        "a detour that never ends it"
    );
}

#[test]
fn a_command_applies_on_its_screen_and_is_observed_once_the_screen_shows() {
    let mut open = command(Kind::Open(9));
    assert_eq!(
        next(&open, &seen(1, TabRead::NoMenu), None, 150),
        Next::Apply
    );
    open.state = Status::APPLIED;
    assert_eq!(
        next(&open, &seen(1, TabRead::NoMenu), None, 160),
        Next::Wait
    );
    assert_eq!(
        next(
            &open,
            &seen(9, TabRead::Shown(MenuTab::Backpack)),
            None,
            170
        ),
        Next::End(Status::OBSERVED, 0)
    );
}

#[test]
fn each_cancel_reason_ends_a_pending_command() {
    let open = command(Kind::Open(9));
    let ended = |seen: Seen, now| next(&open, &seen, None, now);
    let cancelled = |reason| Next::End(Status::CANCELLED, reason);
    assert_eq!(
        ended(
            Seen {
                session: 4,
                ..seen(1, TabRead::NoMenu)
            },
            150
        ),
        cancelled(Status::SESSION_CHANGED)
    );
    assert_eq!(
        ended(seen(4, TabRead::NoMenu), 150),
        cancelled(Status::OTHER_SCREEN)
    );
    assert_eq!(
        ended(
            Seen {
                held: Some(1 << 3),
                ..seen(1, TabRead::NoMenu)
            },
            150
        ),
        cancelled(Status::PLAYER_INPUT)
    );
    let applied = Command {
        state: Status::APPLIED,
        ..open
    };
    assert_eq!(
        next(
            &applied,
            &seen(1, TabRead::NoMenu),
            None,
            100 + TIMEOUT_MS + 1
        ),
        cancelled(Status::TIMED_OUT)
    );
}

#[test]
fn a_control_held_since_the_queue_is_not_the_player_s_new_input() {
    let held = Command {
        held: Some(1 << 3),
        ..command(Kind::Open(9))
    };
    let seen = Seen {
        held: Some(1 << 3),
        ..seen(1, TabRead::NoMenu)
    };
    assert_eq!(next(&held, &seen, None, 150), Next::Apply);
}

#[test]
fn a_tab_applies_while_the_menu_shows_a_tab_the_build_adds() {
    let tab = Command {
        expected: None,
        ..command(Kind::Tab(MenuTab::Map))
    };
    assert_eq!(next(&tab, &seen(4, TabRead::Other), None, 150), Next::Apply);
}

#[test]
fn a_tab_waits_for_the_menu_and_is_observed_when_it_shows() {
    let mut tab = Command {
        expected: None,
        ..command(Kind::Tab(MenuTab::Character))
    };
    assert_eq!(next(&tab, &seen(1, TabRead::NoMenu), None, 150), Next::Wait);
    assert_eq!(
        next(&tab, &seen(4, TabRead::Shown(MenuTab::Backpack)), None, 150),
        Next::Apply
    );
    tab.state = Status::APPLIED;
    assert_eq!(
        next(
            &tab,
            &seen(4, TabRead::Shown(MenuTab::Character)),
            None,
            160
        ),
        Next::End(Status::OBSERVED, 0)
    );
}

#[test]
fn a_command_whose_effect_already_shows_is_observed_without_applying() {
    let open = command(Kind::Open(9));
    assert_eq!(
        next(&open, &seen(9, TabRead::NoMenu), None, 150),
        Next::End(Status::OBSERVED, 0),
        "an open of the screen shown is not toggled shut"
    );
    let tab = Command {
        expected: None,
        ..command(Kind::Tab(MenuTab::Map))
    };
    assert_eq!(
        next(&tab, &seen(4, TabRead::Shown(MenuTab::Map)), None, 150),
        Next::End(Status::OBSERVED, 0)
    );
}

#[test]
fn a_control_held_before_the_first_fresh_read_is_not_a_press() {
    let unread = Command {
        held: None,
        ..command(Kind::Look([0.5, 0.0]))
    };
    let holding = Seen {
        held: Some(1 << 3),
        ..world()
    };
    assert_eq!(next(&unread, &holding, None, 150), Next::Apply);
}

#[test]
fn a_window_command_needs_a_window_an_item_and_a_pick_s_option() {
    let loader = crate::start::tests::bare_loader();
    let spec = |kind, window: &'static str, item: &'static str, text: &'static str| {
        enshwick_abi::headed!(Spec {
            kind,
            target: Str::new(window),
            item: Str::new(item),
            text: Str::new(text),
            ..Spec::EMPTY
        })
    };
    assert_eq!(
        kind(
            &loader,
            &spec(Spec::WINDOW_PRESS, "you.admin:panel", "ban#0", "")
        ),
        Ok(Kind::Window(windows::Act::Press))
    );
    assert_eq!(
        kind(
            &loader,
            &spec(Spec::WINDOW_TEXT, "you.admin:panel", "say/0", "hi")
        ),
        Ok(Kind::Window(windows::Act::Text))
    );
    for (of, window, item, text, why) in [
        (Spec::WINDOW_PRESS, "panel", "ban#0", "", "no mod"),
        (
            Spec::WINDOW_PRESS,
            "you.admin:panel",
            "Ban",
            "",
            "no item id",
        ),
        (
            Spec::WINDOW_PICK,
            "you.admin:panel",
            "ban/0",
            "",
            "no option",
        ),
    ] {
        let read = window_ask(&spec(of, window, item, text), window);
        assert_eq!(read, Err(CommandError::BadArgs), "{why}");
    }
    let long = "x".repeat(Spec::MAX_TEXT + 1);
    let past = enshwick_abi::headed!(Spec {
        text: Str::new(&long),
        ..spec(Spec::WINDOW_TEXT, "you.admin:panel", "say/0", "")
    });
    assert_eq!(
        kind(&loader, &past),
        Err(CommandError::BadArgs),
        "past MAX_TEXT"
    );
}

#[test]
fn a_handed_window_command_ends_as_the_carried_draw_answered() {
    let _state = windows::tests::TEST_STATE.relock();
    let ask = windows::Ask::new("you.admin:panel", "@close", windows::Act::Press, "")
        .expect("ids a window can hold");
    let press = Command {
        id: 77,
        expected: None,
        ..command(Kind::Window(windows::Act::Press))
    };
    windows::register(|| true);
    windows::park(press.id, ask);
    let mut commands = kept(press);
    run(&mut commands, Some(world()), None, 150, |_| Applied::Taken);
    assert!(commands.kept[0].handed.is_some(), "handed to the draw");
    assert_eq!(windows::asked().map(|(id, _)| id), Some(77));
    assert!(windows::claim(77));
    windows::answer(77, windows::Answer::Shown);
    run(&mut commands, Some(world()), None, 160, |_| Applied::Taken);
    assert_eq!(commands.kept[0].state, Status::OBSERVED);
    assert_eq!(windows::asked(), None, "released at its end");
}

/// Commands of their own for a test, apart from the process's, holding
/// `command` as the loader's own, which reads as on without a loader.
fn kept(command: Command) -> Commands {
    Commands {
        next_id: 2,
        kept: VecDeque::from([Command {
            owner: ModIndex::LOADER,
            ..command
        }]),
    }
}

#[test]
fn a_frame_that_reads_no_game_still_ends_a_command_past_its_timeout() {
    let mut commands = kept(command(Kind::Tab(MenuTab::Map)));
    assert!(
        run(&mut commands, None, None, 150, |_| Applied::Taken),
        "within its timeout"
    );
    let late = 100 + TIMEOUT_MS + 1;
    assert!(!run(&mut commands, None, None, late, |_| Applied::Taken));
    let ended = commands.kept[0];
    assert_eq!(
        (ended.state, ended.reason),
        (Status::CANCELLED, Status::TIMED_OUT)
    );
}

#[test]
fn a_command_of_session_0_takes_the_session_and_held_baseline_of_its_first_frame() {
    let mut commands = kept(Command {
        session: 0,
        held: None,
        ..command(Kind::Open(9))
    });
    let shown = Seen {
        session: 5,
        held: Some(1 << 3),
        ..seen(1, TabRead::NoMenu)
    };
    run(&mut commands, Some(shown), None, 150, |_| Applied::Taken);
    let running = commands.kept[0];
    assert_eq!((running.session, running.held), (5, Some(1 << 3)));
    assert_eq!(running.state, Status::APPLIED, "not cancelled");
}

#[test]
fn no_kind_runs_while_the_screen_reads_are_off() {
    crate::engine::screens::READS_FUSE.blow("a test");
    assert!(!runs(Kind::Open(9)));
    assert!(!runs(Kind::Tab(MenuTab::Map)));
}

#[test]
fn a_mods_screen_command_runs_only_while_the_registered_screen_can_open() {
    // The registered check alone: other tests of one process turn the
    // commands' fuses off, which `runs` reads first.
    static OPENS: AtomicBool = AtomicBool::new(true);
    assert!(!mods_screen_opens(), "no interface registered a screen");
    register_mods_screen(|| OPENS.load(Ordering::Relaxed));
    assert!(mods_screen_opens());
    OPENS.store(false, Ordering::Relaxed);
    assert!(
        !mods_screen_opens(),
        "queued as unavailable, not left to time out"
    );
}

#[test]
fn no_kind_runs_once_the_commands_panicked() {
    FUSE.blow("a test");
    assert!(!runs(Kind::Open(9)), "queueing answers unavailable");
}

#[test]
fn the_frame_runs_nothing_while_no_command_is_open() {
    let asked = core::cell::Cell::new(false);
    frame(
        || {
            asked.set(true);
            None
        },
        || {
            asked.set(true);
            Seen::menu(0, LoaderScreen::default())
        },
        |_| Applied::Taken,
    );
    assert!(!asked.get());
}

#[test]
fn at_the_main_menu_a_mods_command_runs_on_the_loader_s_screen_alone() {
    let open = Command {
        expected: None,
        ..command(Kind::Mods(ModsAsk {
            mod_index: None,
            gear: false,
        }))
    };
    let mut commands = kept(open);
    commands.kept.push_back(Command {
        id: 2,
        owner: ModIndex::LOADER,
        expected: None,
        ..command(Kind::Tab(MenuTab::Map))
    });
    let menu = |page| {
        Seen::menu(
            3,
            LoaderScreen {
                page,
                mod_index: None,
            },
        )
    };
    let mut asked = Vec::new();
    assert!(run(
        &mut commands,
        None,
        Some(menu(Snapshot::NO_LOADER_SCREEN)),
        150,
        |change| {
            asked.push(format!("{change:?}"));
            Applied::Taken
        }
    ));
    let mods = FrameChange::Mods(ModsAsk {
        mod_index: None,
        gear: false,
    });
    assert_eq!(
        asked,
        [format!("{mods:?}")],
        "the Mods screen only: no game screen reads at the menu"
    );
    assert_eq!(commands.kept[0].state, Status::APPLIED);
    assert_eq!(commands.kept[1].state, Status::PENDING, "the tab waits");
    run(
        &mut commands,
        None,
        Some(menu(Snapshot::MOD_LIST)),
        160,
        |_| Applied::Taken,
    );
    assert_eq!(commands.kept[0].state, Status::OBSERVED);
    let named = Command {
        expected: Some(1),
        ..open
    };
    assert_eq!(
        next(&named, &menu(Snapshot::NO_LOADER_SCREEN), None, 150),
        Next::End(Status::CANCELLED, Status::OTHER_SCREEN),
        "a HUD screen named, which the menu is not"
    );
}

#[test]
fn a_waypoint_waiting_for_its_turn_is_not_cancelled_by_the_player_s_keys() {
    let mark = Command {
        expected: None,
        ..command(Kind::Mark(Mark::Ping([1.0; 3])))
    };
    let sprinting = Seen {
        held: Some(1 << 12),
        ..world()
    };
    assert_eq!(next(&mark, &sprinting, None, 150), Next::Apply);
    let open = command(Kind::Open(9));
    assert_eq!(
        next(&open, &sprinting, None, 150),
        Next::End(Status::CANCELLED, Status::PLAYER_INPUT),
        "other kinds still give way"
    );
}

#[test]
fn a_server_queues_nothing() {
    let loader = crate::start::tests::bare_loader();
    assert_eq!(
        queue(&loader, ModIndex::new(0), &Spec::EMPTY),
        Err(CommandError::NotClient)
    );
}

#[test]
fn a_waypoint_or_ping_needs_a_finite_position_and_clear_needs_none() {
    let loader = crate::start::tests::bare_loader();
    let spec = |kind, position| {
        enshwick_abi::headed!(Spec {
            kind,
            position,
            ..Spec::EMPTY
        })
    };
    let at = [100.0, 20.0, -50.0];
    assert_eq!(
        kind(&loader, &spec(Spec::SET_WAYPOINT, at)),
        Ok(Kind::Mark(Mark::Set(at)))
    );
    assert_eq!(
        kind(&loader, &spec(Spec::PING, at)),
        Ok(Kind::Mark(Mark::Ping(at)))
    );
    assert_eq!(
        kind(&loader, &spec(Spec::CLEAR_WAYPOINT, [f64::NAN; 3])),
        Ok(Kind::Mark(Mark::Clear))
    );
    for refused in [Spec::SET_WAYPOINT, Spec::PING] {
        assert_eq!(
            kind(&loader, &spec(refused, [f64::INFINITY, 0.0, 0.0])),
            Err(CommandError::BadArgs)
        );
    }
}

#[test]
fn a_position_outside_the_read_map_frame_is_refused_and_any_passes_without_one() {
    let frame = enshwick_abi::headed!(enshwick_core::abi::MapFrameInfo {
        origin: [0.0, 0.0],
        size: [10240.0, 10240.0],
        ..enshwick_core::abi::MapFrameInfo::EMPTY
    });
    assert!(inside_frame(Some(frame), [5000.0, 80.0, 5000.0]));
    assert!(!inside_frame(Some(frame), [-1.0, 80.0, 5000.0]));
    assert!(!inside_frame(Some(frame), [5000.0, 80.0, 10241.0]));
    assert!(inside_frame(None, [-1.0, 0.0, -1.0]), "a joined client");
}

#[test]
fn a_mod_marks_one_waypoint_or_ping_at_a_time_on_any_screen() {
    let set = Kind::Mark(Mark::Set([1.0; 3]));
    assert!(!set.waits_beside(Kind::Mark(Mark::Clear)));
    assert!(set.waits_beside(Kind::Press(32, 16, false)));
    assert!(set.handed() && !set.drives());
    let mark = Command {
        expected: None,
        ..command(set)
    };
    let menu = seen(4, TabRead::NoMenu);
    assert_eq!(next(&mark, &menu, None, 150), Next::Apply);
    let handed = Command {
        handed: Some(150),
        ..mark
    };
    assert_eq!(
        next(&handed, &menu, Some(Progress::Waiting), 160),
        Next::Wait
    );
    assert_eq!(
        next(&handed, &menu, Some(Progress::Done), 170),
        Next::End(Status::OBSERVED, 0),
        "observed once the step's input carries it"
    );
}
