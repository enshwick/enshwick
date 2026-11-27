//! Changes on the loader's Mods screen and the main menu's quit through
//! the commands' frame.

use super::*;

/// A spec of `kind` naming mod `target` and its `item`, with `text`.
fn spec(kind: u32, target: &'static str, item: &'static str, text: &'static str) -> Spec {
    enshwick_abi::headed!(Spec {
        kind,
        target: Str::new(target),
        item: Str::new(item),
        text: Str::new(text),
        ..Spec::EMPTY
    })
}

/// What the menu frame shows on the Mods screen's `page` of mod 0.
fn mods_page(page: u32) -> Seen {
    Seen::menu(
        3,
        LoaderScreen {
            page,
            mod_index: Some(0),
        },
    )
}

#[test]
fn a_rebind_or_a_setting_is_read_with_its_names_and_a_bad_one_refused() {
    let loader = crate::start::tests::loader_running(&["you.lights"]);
    assert_eq!(
        kind(
            &loader,
            &spec(Spec::SET_SETTING, "you.lights", "range", "12")
        ),
        Ok(Kind::Edit(edits::Edit::Setting))
    );
    assert_eq!(
        edit_ask(
            &loader,
            &spec(Spec::SET_SETTING, "you.lights", "range", "12"),
            edits::Edit::Setting
        ),
        Ok(edits::Ask::Setting {
            mod_id: "you.lights".to_owned(),
            key: "range".to_owned(),
            value: "12".to_owned(),
        })
    );
    assert_eq!(
        edit_ask(
            &loader,
            &spec(Spec::SET_SETTING, "you.other", "range", "12"),
            edits::Edit::Setting
        ),
        Err(CommandError::BadArgs),
        "a mod no one loaded"
    );
    for (refused, why) in [
        (
            spec(Spec::SET_SETTING, "you.lights", "range", ""),
            "no value",
        ),
        (spec(Spec::SET_SETTING, "you.lights", "", "12"), "no key"),
        (spec(Spec::REBIND, "", "toggle", ""), "no mod"),
        (
            enshwick_abi::headed!(Spec {
                key_slot: 2,
                ..spec(Spec::REBIND, "you.lights", "toggle", "")
            }),
            "a third key",
        ),
    ] {
        assert_eq!(kind(&loader, &refused), Err(CommandError::BadArgs), "{why}");
    }
    let rebind = |key| {
        enshwick_abi::headed!(Spec {
            key,
            ..spec(Spec::REBIND, "you.lights", "toggle", "")
        })
    };
    assert_eq!(
        edit_ask(&loader, &rebind(0xffff_0000), edits::Edit::Rebind),
        Err(CommandError::BadArgs),
        "bits no key holds"
    );
    assert_eq!(
        edit_ask(&loader, &rebind(0), edits::Edit::Rebind),
        Err(CommandError::BadArgs),
        "a control no mod declared"
    );
}

#[test]
fn a_change_goes_to_the_draw_only_while_a_mod_s_page_shows() {
    let setting = Command {
        expected: None,
        ..command(Kind::Edit(edits::Edit::Setting))
    };
    for page in [Snapshot::MOD_PAGE, Snapshot::MOD_BINDINGS] {
        assert_eq!(next(&setting, &mods_page(page), None, 150), Next::Apply);
    }
    for (seen, why) in [
        (mods_page(Snapshot::MOD_LIST), "the list of mods"),
        (Seen::menu(3, LoaderScreen::default()), "no Mods screen"),
    ] {
        assert_eq!(
            next(&setting, &seen, None, 150),
            Next::End(Status::UNAVAILABLE, 0),
            "{why}"
        );
    }
    let typing = Seen {
        held: Some(1 << 4),
        ..mods_page(Snapshot::MOD_PAGE)
    };
    assert_eq!(
        next(&setting, &typing, None, 150),
        Next::Apply,
        "the player's keys do not cancel it"
    );
}

#[test]
fn a_handed_change_ends_as_the_draw_answered() {
    let handed = Command {
        expected: None,
        handed: Some(150),
        ..command(Kind::Edit(edits::Edit::Rebind))
    };
    let page = mods_page(Snapshot::MOD_PAGE);
    for (progress, end) in [
        (Progress::Done, Next::End(Status::OBSERVED, 0)),
        (Progress::Unavailable, Next::End(Status::UNAVAILABLE, 0)),
        (
            Progress::Cancelled(Status::CHANGE_REFUSED),
            Next::End(Status::CANCELLED, Status::CHANGE_REFUSED),
        ),
    ] {
        assert_eq!(next(&handed, &page, Some(progress), 160), end);
    }
    assert_eq!(
        next(&handed, &page, Some(Progress::Waiting), 160),
        Next::Wait,
        "the draw has not answered"
    );
    assert_eq!(
        next(
            &handed,
            &page,
            Some(Progress::Waiting),
            150 + TIMEOUT_MS + 1
        ),
        Next::End(Status::CANCELLED, Status::TIMED_OUT)
    );
}

#[test]
fn a_handed_change_ends_unavailable_once_its_page_closes_before_the_draw_answers() {
    let handed = Command {
        expected: None,
        handed: Some(150),
        ..command(Kind::Edit(edits::Edit::Setting))
    };
    for (seen, why) in [
        (mods_page(Snapshot::MOD_LIST), "back to the list of mods"),
        (Seen::menu(3, LoaderScreen::default()), "the screen closed"),
    ] {
        assert_eq!(
            next(&handed, &seen, Some(Progress::Waiting), 160),
            Next::End(Status::UNAVAILABLE, 0),
            "{why}: no draw answers it"
        );
    }
}

/// A quit queued at the main menu, in session 0.
fn quit() -> Commands {
    kept(Command {
        session: 0,
        expected: None,
        ..command(Kind::Quit)
    })
}

/// The main menu with nothing running, where a quit posts.
fn idle_menu() -> Seen {
    Seen {
        menu_idle: true,
        ..Seen::menu(3, LoaderScreen::default())
    }
}

#[test]
fn a_queued_quit_is_cancelled_once_the_menu_is_no_longer_idle_before_it_posts() {
    let mut commands = quit();
    assert!(
        run(&mut commands, None, Some(idle_menu()), 150, |_| {
            Applied::Busy
        }),
        "a full queue: it waits"
    );
    let busy = Seen::menu(3, LoaderScreen::default());
    assert!(!run(&mut commands, None, Some(busy), 160, |_| {
        panic!("play started, or a join or host runs: no quit posts")
    }));
    let ended = commands.kept[0];
    assert_eq!(
        (ended.state, ended.reason),
        (Status::CANCELLED, Status::OTHER_SCREEN)
    );
}

#[test]
fn a_quit_is_posted_at_the_main_menu_and_observed_once_the_queue_takes_it() {
    let menu = idle_menu();
    let mut commands = quit();
    let mut posted = Vec::new();
    assert!(
        run(&mut commands, None, Some(menu), 150, |change| {
            posted.push(change == FrameChange::Quit);
            Applied::Busy
        }),
        "a full queue: it waits"
    );
    assert!(!run(&mut commands, None, Some(menu), 160, |change| {
        posted.push(change == FrameChange::Quit);
        Applied::Taken
    }));
    assert_eq!(posted, [true, true]);
    assert_eq!(commands.kept[0].state, Status::OBSERVED);
    let mut in_world = quit();
    assert!(!run(&mut in_world, Some(world()), None, 150, |_| {
        panic!("a world frame posts no quit")
    }));
    let ended = in_world.kept[0];
    assert_eq!(
        (ended.state, ended.reason),
        (Status::CANCELLED, Status::OTHER_SCREEN)
    );
}

#[test]
fn a_quit_runs_in_session_zero_with_no_screen_to_expect() {
    let loader = crate::start::tests::bare_loader();
    let quit = spec(Spec::QUIT, "", "", "");
    assert_eq!(kind(&loader, &quit), Ok(Kind::Quit));
    assert_eq!(
        kind(&loader, &enshwick_abi::headed!(Spec { session: 4, ..quit })),
        Err(CommandError::BadArgs)
    );
    assert_eq!(
        expected_screen(Kind::Quit, "InGame"),
        Err(CommandError::BadArgs)
    );
    assert_eq!(expected_screen(Kind::Quit, ""), Ok(None));
}
