use super::*;

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
        handed: false,
        phase: 0,
        failure: 0,
    }
}

fn seen(screen: u8, tab: TabRead) -> Seen {
    Seen {
        session: 3,
        screen,
        tab,
        recipe: RecipeRead::Unread,
        held: Some(0),
    }
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
        handed: true,
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
        handed: true,
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
        handed: true,
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
        handed: true,
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
    let spec = |kind, look, direction, hold_ms| Spec {
        kind,
        look,
        direction,
        hold_ms,
        ..Spec::EMPTY
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
        Spec {
            kind: Spec::HIGHLIGHT_RECIPE,
            ..Spec::EMPTY
        },
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
    let press = |name: &str, hold_ms| Spec {
        kind: Spec::PRESS,
        target: Str::new(name),
        hold_ms,
        ..Spec::EMPTY
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
        expected_screen(Kind::Press(32, 16), "Backpack"),
        Err(CommandError::BadArgs),
        "a press runs only on the world's screen"
    );
}

#[test]
fn a_mod_holds_one_press_per_action_beside_its_move() {
    let (jump, sprint) = (Kind::Press(32, 16), Kind::Press(53, 2000));
    assert!(jump.waits_beside(sprint));
    assert!(!jump.waits_beside(Kind::Press(32, 500)), "the same action");
    assert!(sprint.waits_beside(Kind::Move([0.0, 1.0], 250)));
    assert!(!Kind::Move([0.0, 1.0], 250).waits_beside(Kind::Move([1.0, 0.0], 10)));
}

#[test]
fn a_press_holds_until_done_and_ends_on_the_player_s_key_or_another_screen() {
    let press = Command {
        handed: true,
        ..command(Kind::Press(32, 2000))
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
        next(
            &pressing,
            &seen(4, TabRead::NoMenu),
            Some(Progress::Taken),
            160
        ),
        cancelled(Status::OTHER_SCREEN)
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
        run(&mut commands, None, 150, |_| Applied::Taken),
        "within its timeout"
    );
    let late = 100 + TIMEOUT_MS + 1;
    assert!(!run(&mut commands, None, late, |_| Applied::Taken));
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
    run(&mut commands, Some(shown), 150, |_| Applied::Taken);
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
fn no_kind_runs_once_the_commands_panicked() {
    FUSE.blow("a test");
    assert!(!runs(Kind::Open(9)), "queueing answers unavailable");
}

#[test]
fn the_frame_runs_nothing_while_no_command_is_open() {
    let mut asked = false;
    frame(
        || {
            asked = true;
            None
        },
        |_| Applied::Taken,
    );
    assert!(!asked);
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
fn a_panic_in_a_join_or_leave_turns_off_only_the_commands() {
    play_frame(|| panic!("a test"), || false, |_| false);
    assert!(FUSE.is_off(), "the commands, not the interface frame");
}

#[test]
fn a_join_and_a_leave_report_a_phase_from_the_queue_on() {
    assert_eq!(Kind::Join.queued_phase(), Status::PHASE_QUEUED);
    assert_eq!(Kind::Leave.queued_phase(), Status::PHASE_IN_WORLD);
    assert_eq!(Kind::Open(9).queued_phase(), 0);
}

#[test]
fn the_stage_tells_once_when_play_starts_and_once_when_it_ends() {
    let _menu = play::note_stage(2, 1);
    assert_eq!(play::note_stage(0, 2), None, "a menu after a menu");
    assert_eq!(play::note_stage(4, 3), Some(true), "play starts loading");
    assert_eq!(play::note_stage(7, 4), None, "loading into the world");
    assert_eq!(play::note_stage(7, 5), None, "still in the world");
    assert_eq!(play::note_stage(2, 6), Some(false), "back at a menu");
    assert_eq!(play::note_stage(2, 7), None, "the menu again");
}

#[test]
fn a_leave_starts_only_in_play_and_only_one_at_a_time() {
    play::note_stage(2, 10);
    assert_eq!(
        play::can_leave(10),
        Err(play::StartError::NotNow),
        "the menu"
    );
    play::note_stage(7, 20);
    assert_eq!(play::can_leave(20), Ok(()));
    assert_eq!(play::leave(5, 20), Ok(()));
    assert_eq!(
        play::can_leave(20),
        Err(play::StartError::NotNow),
        "a leave runs"
    );
}
