use super::*;

fn join_op(step: Step) -> Op {
    Op {
        id: 1,
        step,
        started_ms: 0,
        info: Some(ServerInfo::default()),
        password_given: true,
        refusals: 0,
        report: report(Progress::Waiting, Status::PHASE_QUEUED, 0),
    }
}

fn seen(stage: Stage, game: bool) -> Seen {
    Seen {
        stage,
        game,
        refusals: 0,
    }
}

/// Runs `op` one frame, collecting what it posts; the queue takes all.
fn frame(op: &mut Op, seen: Seen, now: u64) -> (bool, Vec<String>) {
    let mut posted = Vec::new();
    let done = step(op, seen, now, &mut |p| {
        posted.push(match p {
            Post::Password(bytes) => format!("password {}", bytes.len()),
            Post::Join(bytes) => format!("join {}", bytes.len()),
            Post::StartPlay => "start".to_owned(),
            Post::Back => "back".to_owned(),
            Post::Character(slot) => format!("character {slot}"),
            Post::World(slot) => format!("world {slot}"),
            Post::Session { online } => format!("session online {online}"),
            Post::PlayWorld => "play world".to_owned(),
        });
        true
    });
    (done, posted)
}

fn posting() -> Step {
    Step::Post {
        password: Some(Secret(b"hunter2".to_vec())),
        address: vec![3; 0x21].into(),
    }
}

#[test]
fn a_join_walks_the_phases_from_the_menu_into_the_world() {
    let mut op = join_op(posting());
    assert_eq!(
        frame(&mut op, seen(Stage::Menu, false), 10),
        (false, vec!["password 7".to_owned(), "join 33".to_owned()])
    );
    assert!(
        matches!(op.step, Step::Starting { .. }),
        "the password is gone once posted"
    );
    assert_eq!(op.report.phase, Status::PHASE_MENU);
    assert_eq!(frame(&mut op, seen(Stage::Menu, false), 300).1, ["start"]);
    assert_eq!(
        frame(&mut op, seen(Stage::Menu, false), 400).1,
        Vec::<String>::new(),
        "not again within the repost time"
    );
    let (done, posted) = frame(&mut op, seen(Stage::Loading, false), 600);
    assert!(!done && posted.is_empty());
    assert_eq!(op.report.phase, Status::PHASE_CONNECTING);
    frame(&mut op, seen(Stage::Loading, true), 700);
    assert_eq!(op.report.phase, Status::PHASE_LOADING);
    assert!(frame(&mut op, seen(Stage::InGame, true), 900).0);
    assert_eq!(op.report, report(Progress::Done, Status::PHASE_IN_WORLD, 0));
}

#[test]
fn a_join_that_falls_back_to_the_menu_names_what_the_client_knows() {
    let failed = |info: ServerInfo, password_given: bool, refusals: u64| {
        let mut op = Op {
            info: Some(info),
            password_given,
            ..join_op(Step::Playing)
        };
        let seen = Seen {
            refusals,
            ..seen(Stage::Menu, false)
        };
        assert!(frame(&mut op, seen, 10).0);
        op.report
    };
    let full = ServerInfo {
        players: 16,
        max_players: 16,
        ..ServerInfo::default()
    };
    let locked = ServerInfo {
        password: true,
        ..ServerInfo::default()
    };
    assert_eq!(failed(full, true, 0).failure, Status::FULL);
    assert_eq!(
        failed(locked.clone(), false, 0).failure,
        Status::WRONG_PASSWORD
    );
    assert_eq!(
        failed(locked, true, 0).failure,
        Status::UNKNOWN,
        "a given password may still be the wrong one, which the client does not show"
    );
    assert_eq!(
        failed(ServerInfo::default(), true, 1).failure,
        Status::REFUSED
    );
    assert_eq!(
        failed(ServerInfo::default(), true, 0).progress,
        Progress::Failed(Status::UNKNOWN)
    );
}

#[test]
fn a_join_the_game_never_starts_times_out_and_goes_back_until_the_menu_settles() {
    let mut op = join_op(Step::Starting {
        posted_ms: 0,
        last_ms: 0,
    });
    assert!(!frame(&mut op, seen(Stage::Menu, false), START_MS).0);
    let (done, posted) = frame(&mut op, seen(Stage::Menu, false), START_MS + 1);
    assert!(!done, "a start posted just before may still be taken");
    assert_eq!(posted, ["back"]);
    assert_eq!(
        op.report,
        report(
            Progress::Failed(Status::JOIN_TIMED_OUT),
            Status::PHASE_MENU,
            Status::JOIN_TIMED_OUT
        )
    );
    assert_eq!(
        frame(&mut op, seen(Stage::Loading, false), START_MS + 400),
        (false, vec!["back".to_owned()]),
        "play that started late is backed out of"
    );
    assert!(
        frame(
            &mut op,
            seen(Stage::Menu, false),
            START_MS + 1 + BACK_SETTLE_MS
        )
        .0
    );
}

#[test]
fn a_join_whose_posts_the_queue_never_takes_times_out_at_the_menu() {
    let mut op = join_op(posting());
    let refused = |_: Post<'_>| false;
    assert!(!step(&mut op, seen(Stage::Menu, false), 10, &mut {
        refused
    }));
    assert!(step(
        &mut op,
        seen(Stage::Menu, false),
        START_MS + 1,
        &mut { refused }
    ));
    assert_eq!(op.report.failure, Status::JOIN_TIMED_OUT);
    let Step::Post { password, .. } = &op.step else {
        panic!("still posting: {:?}", op.step);
    };
    assert!(password.is_none(), "the password is wiped at the time-out");
}

#[test]
fn a_join_the_join_check_refuses_before_play_starts_fails_as_refused() {
    let mut op = join_op(Step::Starting {
        posted_ms: 0,
        last_ms: 0,
    });
    let refused = Seen {
        refusals: 1,
        ..seen(Stage::Menu, false)
    };
    assert_eq!(frame(&mut op, refused, 300), (true, Vec::new()));
    assert_eq!(
        op.report,
        report(
            Progress::Failed(Status::REFUSED),
            Status::PHASE_MENU,
            Status::REFUSED
        )
    );
}

#[test]
fn only_a_mod_s_cancel_backs_a_taken_join_out_of_play() {
    let mut op = join_op(Step::Playing);
    assert!(
        !let_go(&mut op, 10, false),
        "a mod or feature off lets it be"
    );
    assert!(matches!(op.step, Step::Playing));
    assert!(let_go(&mut op, 10, true));
    assert!(matches!(op.step, Step::Backing { since_ms: 10, .. }));
    let mut asking = join_op(Step::Asking {
        password: Secret(Vec::new()),
    });
    assert!(!let_go(&mut asking, 10, true), "nothing posted: it stops");
}

#[test]
fn a_leave_that_times_out_in_a_world_keeps_the_joined_server() {
    let mut op = join_op(Step::Leaving { last_ms: None });
    assert!(frame(&mut op, seen(Stage::InGame, true), LEAVE_MS + 1).0);
    assert_eq!(op.report.progress, Progress::Cancelled(Status::TIMED_OUT));
    assert!(!forgets_server(&op, Stage::InGame));
    assert!(forgets_server(&op, Stage::Menu));
    op.report = report(
        Progress::Failed(Status::UNKNOWN),
        Status::PHASE_MENU,
        Status::UNKNOWN,
    );
    assert!(forgets_server(&op, Stage::Loading));
}

#[test]
fn an_answer_for_an_op_that_no_longer_asks_changes_nothing() {
    let mut op = join_op(Step::Leaving { last_ms: Some(5) });
    take_answer(&mut op, Answer::Unreachable, 0);
    assert!(matches!(op.step, Step::Leaving { last_ms: Some(5) }));
    assert_eq!(op.report.progress, Progress::Waiting);
}

#[test]
fn a_join_runs_only_where_the_address_fits_the_join_command() {
    assert!(fits(Some(0x21), Some(0x21)));
    assert!(!fits(Some(0x20), Some(0x21)));
    assert!(!fits(None, Some(0x21)), "no menu posters");
    assert!(!fits(Some(0x21), None), "no join target layout");
}

#[test]
fn an_idle_frame_reads_no_play_data_and_posts_nothing() {
    super::frame(
        Stage::Menu,
        10,
        || panic!("read the play data"),
        |_| panic!("posted"),
    );
}

#[test]
fn a_loading_join_waits_at_the_game_s_pace() {
    let mut op = join_op(Step::Playing);
    assert!(
        !frame(&mut op, seen(Stage::Loading, true), 10 * START_MS).0,
        "no two-second or start limit in play"
    );
}

#[test]
fn an_unreachable_server_fails_the_join_before_it_is_posted() {
    let mut op = join_op(Step::Asking {
        password: Secret(Vec::new()),
    });
    take_answer(&mut op, Answer::Unreachable, 0);
    assert_eq!(
        op.report,
        report(
            Progress::Failed(Status::UNREACHABLE),
            Status::PHASE_QUEUED,
            Status::UNREACHABLE
        )
    );
    assert_eq!(
        frame(&mut op, seen(Stage::Menu, false), 10),
        (true, Vec::new()),
        "nothing posted"
    );
}

#[test]
fn a_cancelled_join_goes_back_until_the_menu_shows_a_while() {
    let mut op = join_op(Step::Backing {
        since_ms: 0,
        last_ms: None,
    });
    op.report = report(
        Progress::Cancelled(Status::BY_MOD),
        Status::PHASE_LOADING,
        0,
    );
    assert_eq!(
        frame(&mut op, seen(Stage::Loading, true), 10),
        (false, vec!["back".to_owned()])
    );
    assert_eq!(
        frame(&mut op, seen(Stage::Menu, false), 500),
        (false, vec!["back".to_owned()]),
        "the start-play state reads as a menu and takes back; the main menu ignores it"
    );
    assert!(frame(&mut op, seen(Stage::Menu, false), BACK_SETTLE_MS).0);
}

#[test]
fn a_leave_posts_back_in_play_and_ends_when_the_menu_shows() {
    let mut op = join_op(Step::Leaving { last_ms: None });
    assert_eq!(
        frame(&mut op, seen(Stage::InGame, true), 10),
        (false, vec!["back".to_owned()])
    );
    assert_eq!(op.report.progress, Progress::Taken);
    assert!(!frame(&mut op, seen(Stage::Loading, false), 100).0);
    assert!(frame(&mut op, seen(Stage::Menu, false), 2_000).0);
    assert_eq!(op.report, report(Progress::Done, Status::PHASE_LEFT, 0));
}

#[test]
fn a_full_queue_keeps_the_join_for_the_next_frame() {
    let mut op = join_op(posting());
    let mut taken = 0;
    step(&mut op, seen(Stage::Menu, false), 10, &mut |p| {
        taken += 1;
        matches!(p, Post::Password(_))
    });
    assert_eq!(taken, 2);
    let Step::Post { password, .. } = &op.step else {
        panic!("still posting: {:?}", op.step);
    };
    assert!(
        password.is_none(),
        "the password went once; the address goes next frame"
    );
}

#[test]
fn the_stage_byte_names_the_menu_loading_and_the_world() {
    assert_eq!(
        [0, 1, 3, 4, 6, 7, 8].map(Stage::of),
        [
            Stage::Menu,
            Stage::Menu,
            Stage::Menu,
            Stage::Loading,
            Stage::Loading,
            Stage::InGame,
            Stage::Menu
        ],
        "as the interface frame names its screens"
    );
}

#[test]
fn the_password_never_shows_in_a_debug_print() {
    let op = join_op(posting());
    assert!(!format!("{op:?}").contains("hunter2"));
}

#[test]
fn a_host_posts_its_character_world_and_session_then_plays_the_world() {
    let host = Host {
        world: 2,
        character: 0,
        online: true,
    };
    let mut op = Op {
        info: None,
        password_given: false,
        ..join_op(Step::Hosting { host, posted: 0 })
    };
    assert_eq!(
        frame(&mut op, seen(Stage::Loading, false), 10).1,
        Vec::<String>::new(),
        "only at the menu"
    );
    assert_eq!(
        frame(&mut op, seen(Stage::Menu, false), 20),
        (
            false,
            vec![
                "character 1".to_owned(),
                "character 257".to_owned(),
                "world 3".to_owned(),
                "world 259".to_owned(),
                "session online true".to_owned(),
                "play world".to_owned(),
            ]
        ),
        "each place in both storages, the place plus 1 in the low byte and the storage in the high"
    );
    assert!(matches!(op.step, Step::Starting { .. }));
    assert_eq!(op.report, report(Progress::Taken, Status::PHASE_MENU, 0));
    assert_eq!(frame(&mut op, seen(Stage::Menu, false), 300).1, ["start"]);
    frame(&mut op, seen(Stage::Loading, true), 600);
    assert_eq!(op.report.phase, Status::PHASE_LOADING);
    assert!(frame(&mut op, seen(Stage::InGame, true), 900).0);
    assert_eq!(op.report, report(Progress::Done, Status::PHASE_IN_WORLD, 0));
}

#[test]
fn a_host_whose_posts_the_queue_refuses_goes_on_from_where_it_stopped() {
    let host = Host {
        world: 0,
        character: 4,
        online: false,
    };
    let mut op = join_op(Step::Hosting { host, posted: 0 });
    let mut taken = 4;
    let mut posted = Vec::new();
    let done = step(&mut op, seen(Stage::Menu, false), 10, &mut |p| {
        posted.push(format!("{p:?}"));
        taken -= 1;
        taken >= 0
    });
    assert!(!done);
    assert!(
        matches!(op.step, Step::Hosting { posted: 4, .. }),
        "{:?}",
        op.step
    );
    let (_, next) = frame(&mut op, seen(Stage::Menu, false), 20);
    assert_eq!(next, ["session online false", "play world"]);
    let mut late = join_op(Step::Hosting { host, posted: 0 });
    let (done, _) = frame(&mut late, seen(Stage::Menu, false), START_MS + 1);
    assert!(done);
    assert_eq!(
        late.report.progress,
        Progress::Failed(Status::JOIN_TIMED_OUT)
    );
}

#[test]
fn a_host_whose_posts_land_late_still_times_out_twenty_seconds_after_its_queue() {
    let host = Host {
        world: 0,
        character: 0,
        online: false,
    };
    let mut op = join_op(Step::Hosting { host, posted: 0 });
    let late = START_MS - 1000;
    assert!(!frame(&mut op, seen(Stage::Menu, false), late).0);
    assert!(matches!(op.step, Step::Starting { .. }));
    let (_, posted) = frame(&mut op, seen(Stage::Menu, false), START_MS + 1);
    assert_eq!(
        op.report.progress,
        Progress::Failed(Status::JOIN_TIMED_OUT),
        "not a second limit from the last post"
    );
    assert_eq!(posted, ["back"], "a start posted just before may be taken");
}

#[test]
fn a_host_starts_only_at_the_menu_and_never_beside_a_join() {
    let host = Host {
        world: 0,
        character: 0,
        online: false,
    };
    note_stage(7, 100);
    assert_eq!(
        super::host(40, host, 100),
        Err(StartError::NotNow),
        "in a world"
    );
    note_stage(2, 200);
    assert_eq!(super::host(41, host, 200), Ok(()));
    assert_eq!(
        super::host(42, host, 200),
        Err(StartError::NotNow),
        "one runs"
    );
    release(41, 200, false);
    assert_eq!(report_of(41), None);
}
