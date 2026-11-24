use enshwick_core::PlayerKey;

use super::super::recorder::{Answer, Drew, Recorder};
use super::*;

fn view(title: &str) -> View {
    View {
        title: title.into(),
        ..View::default()
    }
}

#[test]
fn a_window_opens_updates_in_place_and_an_update_of_a_closed_one_changes_nothing() {
    let mut held = Vec::new();
    hold(&mut held, "you.admin", "panel", true, view("one"));
    hold(&mut held, "you.admin", "bans", true, view("bans"));
    hold(&mut held, "you.admin", "panel", false, view("two"));
    hold(&mut held, "you.admin", "gone", false, view("gone"));
    let titles: Vec<&str> = held.iter().map(|h| h.view.title.as_str()).collect();
    assert_eq!(titles, ["two", "bans"]);
    for n in 0..MAX_HELD {
        hold(&mut held, "you.admin", &format!("w{n}"), true, view("w"));
    }
    assert_eq!(held.len(), MAX_HELD);
    assert!(held.iter().all(|h| h.window != "panel"), "the oldest goes");
}

#[test]
fn back_ends_the_typing_then_leaves_the_form_then_closes_the_window() {
    let mut drawn = Drawn::new(("you.admin".into(), "panel".into()));
    drawn.form = Some(Form {
        label: "Ban".into(),
        action: "ban".into(),
        given: Vec::new(),
    });
    let text = ArgView {
        kind: ArgView::TEXT,
        ..ArgView::default()
    };
    drawn.field("ban", 0, &text).text.start_typing();
    assert_eq!(drawn.back(), Click::Nothing, "the typing ends first");
    assert_eq!(drawn.back(), Click::Cancel, "then the form closes");
    drawn.after(&Click::Cancel);
    assert_eq!(drawn.back(), Click::Close, "then the window");
}

#[test]
fn opening_or_leaving_a_form_starts_its_scroll_area_at_the_top() {
    let mut drawn = Drawn::new(("you.admin".into(), "panel".into()));
    let list = drawn.scroll.frame_id();
    let form = Form {
        label: "Ban".into(),
        action: "ban".into(),
        given: Vec::new(),
    };
    drawn.after(&Click::Form(form));
    let opened = drawn.scroll.frame_id();
    assert_ne!(opened, list, "the form opens at its top");
    drawn.after(&Click::Nothing);
    assert_eq!(
        drawn.scroll.frame_id(),
        opened,
        "nothing done keeps the place"
    );
    drawn.after(&Click::Cancel);
    assert_ne!(
        drawn.scroll.frame_id(),
        opened,
        "the list comes back at its top"
    );
}

#[test]
fn a_press_runs_or_opens_a_form_for_the_values_its_action_still_takes() {
    let ban = Entry {
        id: "ban".into(),
        args: vec![
            ArgView {
                kind: ArgView::PLAYER,
                ..ArgView::default()
            },
            ArgView {
                kind: ArgView::TEXT,
                max: 64.0,
                ..ArgView::default()
            },
        ],
        ..Entry::default()
    };
    let view = View {
        entries: vec![ban],
        ..View::default()
    };
    let ann = Value::Player {
        key: PlayerKey::new(7),
        name: String::new(),
    };
    let button = |args: Vec<Value>| ButtonView {
        label: "Ban".into(),
        action: "ban".into(),
        args,
    };
    assert_eq!(
        press(&view, &button(vec![ann.clone()])),
        Click::Form(Form {
            label: "Ban".into(),
            action: "ban".into(),
            given: vec![ann.clone()],
        })
    );
    let full = vec![ann, Value::Text("spam".into())];
    assert_eq!(
        press(&view, &button(full.clone())),
        Click::Run("ban".into(), full)
    );
}

fn say() -> Entry {
    Entry {
        id: "say".into(),
        label: "Announce".into(),
        offered: true,
        args: vec![ArgView {
            kind: ArgView::TEXT,
            label: "Message".into(),
            max: 64.0,
            ..ArgView::default()
        }],
        ..Entry::default()
    }
}

fn drawn() -> Drawn {
    Drawn::new(("you.admin".into(), "panel".into()))
}

#[test]
fn a_window_stepping_aside_for_the_map_keeps_its_form_and_fields() {
    let mut drawn = drawn();
    let form = Form {
        label: "Kick".into(),
        action: "kick".into(),
        given: Vec::new(),
    };
    drawn.form = Some(form.clone());
    let reason = ArgView {
        kind: ArgView::TEXT,
        ..ArgView::default()
    };
    drawn.field("kick", 0, &reason);
    drawn.step_aside();
    assert_eq!(drawn.form, Some(form));
    assert_eq!(drawn.fields.len(), 1);
}

fn run_button(drew: &[Drew]) -> Option<bool> {
    drew.iter().find_map(|widget| match widget {
        Drew::Button { text, enabled } if text == texts::text("windows.run") => Some(*enabled),
        _ => None,
    })
}

#[test]
fn an_action_s_run_stays_drawn_but_off_until_its_fields_are_set() {
    let paint = Recorder::default().answer(texts::text("windows.run"), Answer::Press);
    assert_eq!(action(&paint, &say(), &mut drawn()), Click::Nothing);
    let drew = paint.drew();
    assert_eq!(run_button(&drew), Some(false), "drawn, off: {drew:?}");
    assert_eq!(
        drew.last(),
        Some(&Drew::Button {
            text: texts::text("windows.run").into(),
            enabled: false
        })
    );
}

#[test]
fn an_action_runs_with_the_values_its_fields_hold() {
    let paint = Recorder::default()
        .answer(
            texts::text("windows.text"),
            Answer::Type("Restart soon".into()),
        )
        .answer(texts::text("windows.run"), Answer::Press);
    assert_eq!(
        action(&paint, &say(), &mut drawn()),
        Click::Run("say".into(), vec![Value::Text("Restart soon".into())])
    );
    let refused = Entry {
        offered: false,
        ..say()
    };
    assert_eq!(
        action(&paint, &refused, &mut drawn()),
        Click::Nothing,
        "an action the player may not run now"
    );
}

#[test]
fn an_action_without_fields_is_one_button_under_its_label() {
    let wave = Entry {
        id: "wave".into(),
        label: "Wave".into(),
        offered: true,
        ..Entry::default()
    };
    let paint = Recorder::default().answer("Wave", Answer::Press);
    assert_eq!(
        action(&paint, &wave, &mut drawn()),
        Click::Run("wave".into(), Vec::new())
    );
    assert_eq!(
        paint.drew(),
        [Drew::Button {
            text: "Wave".into(),
            enabled: true
        }]
    );
}

#[test]
fn a_list_s_switch_runs_its_action_on_that_player_and_reads_only_when_not_offered() {
    let list = Entry {
        id: "allowed".into(),
        label: "Allowed".into(),
        per_player: true,
        offered: true,
        players: vec![(PlayerKey::new(7), "Ann".into(), false)],
        ..Entry::default()
    };
    let paint = Recorder::default().answer("Ann", Answer::Flip(true));
    let ann = Value::Player {
        key: PlayerKey::new(7),
        name: String::new(),
    };
    assert_eq!(
        action(&paint, &list, &mut drawn()),
        Click::Run("allowed".into(), vec![ann, Value::Toggle(true)])
    );
    let shut = Entry {
        offered: false,
        ..list
    };
    assert_eq!(action(&paint, &shut, &mut drawn()), Click::Nothing);
}

#[test]
fn a_form_runs_with_the_button_s_values_and_the_rest_or_cancels() {
    let ann = Value::Player {
        key: PlayerKey::new(7),
        name: String::new(),
    };
    let ban = Entry {
        id: "ban".into(),
        offered: true,
        args: vec![
            ArgView {
                kind: ArgView::PLAYER,
                ..ArgView::default()
            },
            ArgView {
                kind: ArgView::TEXT,
                max: 64.0,
                ..ArgView::default()
            },
        ],
        ..Entry::default()
    };
    let view = View {
        entries: vec![ban],
        ..View::default()
    };
    let form = Form {
        label: "Ban".into(),
        action: "ban".into(),
        given: vec![ann.clone()],
    };
    let empty = Recorder::default().answer(texts::text("windows.run"), Answer::Press);
    assert_eq!(
        form_widgets(&empty, &view, &form, &mut drawn()),
        Click::Nothing
    );
    assert_eq!(run_button(&empty.drew()), Some(false));

    let typed = Recorder::default()
        .answer(texts::text("windows.text"), Answer::Type("spam".into()))
        .answer(texts::text("windows.run"), Answer::Press);
    assert_eq!(
        form_widgets(&typed, &view, &form, &mut drawn()),
        Click::Run("ban".into(), vec![ann, Value::Text("spam".into())])
    );
    let cancels = Recorder::default().answer(texts::text("windows.cancel"), Answer::Press);
    assert_eq!(
        form_widgets(&cancels, &view, &form, &mut drawn()),
        Click::Cancel
    );
    let gone = Form {
        action: "kick".into(),
        ..form
    };
    assert_eq!(
        form_widgets(&typed, &view, &gone, &mut drawn()),
        Click::Cancel,
        "the action left the window"
    );
}

#[test]
fn a_form_a_run_or_a_cancel_starts_the_action_s_fields_afresh() {
    let arg = ArgView {
        kind: ArgView::TEXT,
        max: 64.0,
        ..ArgView::default()
    };
    let form = Form {
        label: "Ban".into(),
        action: "ban".into(),
        given: Vec::new(),
    };
    let typed =
        |drawn: &mut Drawn, action: &str| drawn.field(action, 1, &arg).text.text().to_owned();
    let mut drawn = Drawn::new(("you.admin".into(), "panel".into()));
    drawn.field("ban", 1, &arg).text.set("griefing");
    drawn.field("say", 1, &arg).text.set("hello");

    drawn.after(&Click::Nothing);
    assert_eq!(typed(&mut drawn, "ban"), "griefing");
    drawn.after(&Click::Run("ban".into(), Vec::new()));
    assert_eq!(
        typed(&mut drawn, "ban"),
        "",
        "the reason of the last ban is gone"
    );
    assert_eq!(
        typed(&mut drawn, "say"),
        "hello",
        "another action's field stays"
    );

    drawn.field("ban", 1, &arg).text.set("spam");
    drawn.after(&Click::Form(form.clone()));
    assert_eq!(drawn.form, Some(form));
    assert_eq!(typed(&mut drawn, "ban"), "", "a form opens empty");
    drawn
        .field("ban", 1, &arg)
        .text
        .set("typed, then cancelled");
    drawn.after(&Click::Cancel);
    assert_eq!(drawn.form, None);
    assert_eq!(typed(&mut drawn, "ban"), "");
    assert_eq!(typed(&mut drawn, "say"), "hello");
}

#[test]
fn each_action_s_argument_keeps_a_field_of_its_own() {
    let arg = ArgView {
        kind: ArgView::CHOICE,
        ..ArgView::default()
    };
    let mut drawn = Drawn::new(("you.admin".into(), "panel".into()));
    drawn.field("ban", 1, &arg).pick = 3;
    assert_eq!(drawn.field("say", 1, &arg).pick, 0, "another action");
    assert_eq!(drawn.field("ban", 0, &arg).pick, 0, "another argument");
    assert_eq!(drawn.field("ban", 1, &arg).pick, 3, "the same field");
    assert_eq!(drawn.fields.len(), 3);
}

fn map_view() -> View {
    View {
        items: vec![ItemView::Map(MapView::default())],
        ..View::default()
    }
}

#[test]
fn the_window_steps_aside_on_the_map_screen_and_on_the_in_game_menu_s_map_tab() {
    // Client 26430625: the Map screen is 5, the in-game menu 4, its
    // Backpack tab 9; the menu on its Map tab is seen as the Map screen.
    let mut drawn = drawn();
    drawn.map_screen = Some(5);
    assert!(
        drawn.aside(Some(5)),
        "the map, by its key or the menu's tab"
    );
    assert!(!drawn.aside(Some(9)), "the menu on its Backpack tab");
    assert!(!drawn.aside(Some(4)), "the menu without the tab read");
    assert!(!drawn.aside(None), "a tab the build adds");
    drawn.map_screen = None;
    assert!(!drawn.aside(None), "a build without the Map screen");
}

#[test]
fn the_map_key_passes_only_under_a_window_with_a_map_and_no_typing() {
    let mut drawn = drawn();
    assert!(drawn.passes_map(&map_view()));
    assert!(!drawn.passes_map(&view("Kick")), "no map in the window");
    let text = ArgView {
        kind: ArgView::TEXT,
        ..ArgView::default()
    };
    drawn.field("say", 0, &text).text.start_typing();
    assert!(!drawn.passes_map(&map_view()), "M is typed into the field");
    drawn.back();
    assert!(drawn.passes_map(&map_view()), "the typing ended");
}

#[test]
fn the_map_key_passes_only_in_a_frame_whose_windows_drew_and_where_nothing_types() {
    assert!(map_wanted(true, true, false));
    assert!(!map_wanted(false, true, false), "no carried draw asked");
    assert!(
        !map_wanted(true, false, false),
        "the game's own popup is up"
    );
    assert!(
        !map_wanted(true, true, true),
        "a field of another window types"
    );
}

#[test]
fn a_map_draws_its_line_and_a_button_that_opens_the_map() {
    let paint = Recorder::default();
    let mut drawn = drawn();
    drawn.map_opens = Some(5);
    assert_eq!(items(&paint, &map_view(), &mut drawn), Click::Nothing);
    assert_eq!(
        paint.drew(),
        [
            Drew::Label(texts::text("windows.map").into()),
            Drew::Button {
                text: texts::text("windows.map.open").into(),
                enabled: true
            }
        ]
    );
}

#[test]
fn pressing_the_map_button_opens_the_map_screen_the_key_cannot_reach() {
    let paint = Recorder::default().answer(texts::text("windows.map.open"), Answer::Press);
    let mut drawn = drawn();
    drawn.map_opens = Some(5);
    assert_eq!(items(&paint, &map_view(), &mut drawn), Click::OpenMap(5));
}

#[test]
fn the_map_button_turns_off_when_the_toggle_goes_while_the_window_shows() {
    let mut drawn = drawn();
    drawn.map_screen = Some(5);
    drawn.refresh(true);
    assert_eq!(drawn.map_opens, Some(5));
    drawn.refresh(false);
    assert_eq!(drawn.map_opens, None);
}

#[test]
fn a_refused_map_open_is_tried_again_until_taken() {
    let mut drawn = drawn();
    drawn.map_screen = Some(5);
    drawn.refresh(true);
    assert!(!drawn.opened(false));
    assert_eq!(drawn.retry(Click::Nothing), Click::OpenMap(5));
    assert_eq!(drawn.retry(Click::Close), Click::Close);
    assert!(!drawn.opened(true));
    assert_eq!(drawn.retry(Click::Nothing), Click::Nothing);
}

#[test]
fn a_map_open_refused_every_frame_gives_up_once() {
    let mut drawn = drawn();
    drawn.map_screen = Some(5);
    drawn.refresh(true);
    let gave_up = (0..=MAP_OPEN_TRIES).filter(|_| drawn.opened(false)).count();
    assert_eq!(gave_up, 1);
    assert_eq!(drawn.retry(Click::Nothing), Click::Nothing);
}

#[test]
fn opening_the_map_ends_the_retry() {
    let mut drawn = drawn();
    drawn.map_screen = Some(5);
    drawn.refresh(true);
    drawn.opened(false);
    drawn.step_aside();
    assert_eq!(drawn.retry(Click::Nothing), Click::Nothing);
}

#[test]
fn the_map_button_stays_off_on_a_build_that_cannot_open_the_map() {
    let paint = Recorder::default().answer(texts::text("windows.map.open"), Answer::Press);
    let mut drawn = drawn();
    drawn.map_opens = None;
    assert_eq!(items(&paint, &map_view(), &mut drawn), Click::Nothing);
    assert!(paint.drew().contains(&Drew::Button {
        text: texts::text("windows.map.open").into(),
        enabled: false
    }));
}

/// Held by every test that changes the windows the client holds, since
/// `cargo test` runs tests as threads of one process.
static HELD: Mutex<()> = Mutex::new(());

fn held() -> Vec<(String, String)> {
    let held = STATE.held.load();
    let held = held.as_deref().map_or_default(Vec::as_slice);
    held.iter()
        .map(|h| (h.mod_id.clone(), h.window.clone()))
        .collect()
}

#[test]
fn the_server_s_close_takes_only_its_own_window() {
    let _held = HELD.relock();
    heard("you.admin", "panel", true, "W\tPanel\n");
    heard("you.admin", "bans", true, "W\tBans\n");
    heard("you.other", "panel", true, "W\tOther\n");
    heard("you.admin", "broken", true, "no title");
    closed("you.admin", "panel");
    assert_eq!(
        held(),
        [
            ("you.admin".to_owned(), "bans".to_owned()),
            ("you.other".to_owned(), "panel".to_owned())
        ]
    );
    closed("you.admin", "bans");
    closed("you.other", "panel");
    assert!(STATE.held.load().is_none(), "nothing held");
}

/// A window with a Ban row per player, whose action takes a reason past the
/// player its button gives, and the Announce action drawn whole.
fn ban_and_say() -> View {
    let row = |key: u64, name: &str| ItemView::Row {
        text: name.into(),
        buttons: vec![ButtonView {
            label: "Ban".into(),
            action: "ban".into(),
            args: vec![Value::Player {
                key: PlayerKey::new(key),
                name: String::new(),
            }],
        }],
    };
    let reason = ArgView {
        kind: ArgView::TEXT,
        label: "Reason".into(),
        max: 64.0,
        ..ArgView::default()
    };
    let player = ArgView {
        kind: ArgView::PLAYER,
        label: "Player".into(),
        ..ArgView::default()
    };
    View {
        title: "Check".into(),
        items: vec![row(7, "Ann"), row(8, "Bob"), ItemView::Action("say".into())],
        entries: vec![
            Entry {
                id: "ban".into(),
                label: "Ban".into(),
                offered: true,
                args: vec![player, reason],
                ..Entry::default()
            },
            say(),
        ],
    }
}

fn ask(item: &str, act: Act, text: &str) -> Ask {
    Ask::new("you.admin:panel", item, act, text).expect("ids a window can hold")
}

#[test]
fn a_commanded_text_fills_the_field_and_a_commanded_run_sends_it() {
    let view = ban_and_say();
    let mut drawn = drawn();
    let typed = drawn.take(&view, &ask("say/0", Act::Text, "Restart soon"));
    assert_eq!(typed, Some(windows::Answer::Shown));
    assert_eq!(
        drawn.take(&view, &ask("say", Act::Press, "")),
        None,
        "the draw presses"
    );
    assert_eq!(
        items(&Recorder::default(), &view, &mut drawn),
        Click::Run("say".into(), vec![Value::Text("Restart soon".into())])
    );
}

#[test]
fn a_commanded_row_press_opens_the_form_of_that_row_s_player() {
    let view = ban_and_say();
    let mut drawn = drawn();
    assert_eq!(drawn.take(&view, &ask("ban#1", Act::Press, "")), None);
    let Click::Form(form) = items(&Recorder::default(), &view, &mut drawn) else {
        panic!("a Ban with a reason still to give opens its form");
    };
    let bob = Value::Player {
        key: PlayerKey::new(8),
        name: String::new(),
    };
    assert_eq!(form.given, [bob]);
}

#[test]
fn a_command_on_an_item_the_window_lacks_or_draws_off_says_so() {
    let view = ban_and_say();
    let mut drawn = drawn();
    let missing = drawn.take(&view, &ask("kick#0", Act::Press, ""));
    assert_eq!(missing, Some(windows::Answer::Missing));
    let text_on_button = drawn.take(&view, &ask("ban#0", Act::Text, "x"));
    assert_eq!(
        text_on_button,
        Some(windows::Answer::Off),
        "a button takes no text"
    );
    assert_eq!(
        drawn.take(&view, &ask("say", Act::Press, "")),
        Some(windows::Answer::Off),
        "Announce's Run is drawn off while its field is empty"
    );
    assert_eq!(drawn.pressing, None, "the draw presses nothing");
    let mut refused = ban_and_say();
    refused.entries[1].offered = false;
    let off = drawn.take(&refused, &ask("say", Act::Press, ""));
    assert_eq!(
        off,
        Some(windows::Answer::Off),
        "an action the player may not run"
    );
}

#[test]
fn a_commanded_pick_takes_the_option_by_the_name_the_window_shows() {
    let kind = ArgView {
        kind: ArgView::CHOICE,
        label: "Kind".into(),
        choices: vec!["Basic".into(), "Pro".into()],
        ..ArgView::default()
    };
    let mut view = ban_and_say();
    view.entries[1].args = vec![kind];
    let mut drawn = drawn();
    let pro = drawn.take(&view, &ask("say/0", Act::Pick, "Pro"));
    assert_eq!(pro, Some(windows::Answer::Shown));
    let none = drawn.take(&view, &ask("say/0", Act::Pick, "Gold"));
    assert_eq!(none, Some(windows::Answer::Off), "no option of that name");
    assert_eq!(drawn.take(&view, &ask("say", Act::Press, "")), None);
    assert_eq!(
        items(&Recorder::default(), &view, &mut drawn),
        Click::Run("say".into(), vec![Value::Choice(1)])
    );
}

#[test]
fn a_commanded_switch_flips_that_player_s_switch() {
    let list = Entry {
        id: "allowed".into(),
        label: "Allowed".into(),
        per_player: true,
        offered: true,
        players: vec![(PlayerKey::new(7), "Ann".into(), false)],
        ..Entry::default()
    };
    let view = View {
        title: "List".into(),
        items: vec![ItemView::Action("allowed".into())],
        entries: vec![list],
    };
    let mut drawn = drawn();
    assert_eq!(drawn.take(&view, &ask("allowed@7", Act::Press, "")), None);
    let ann = Value::Player {
        key: PlayerKey::new(7),
        name: String::new(),
    };
    assert_eq!(
        items(&Recorder::default(), &view, &mut drawn),
        Click::Run("allowed".into(), vec![ann, Value::Toggle(true)])
    );
}

#[test]
fn a_commanded_cancel_leaves_the_open_form_and_lists_follow_it() {
    let view = ban_and_say();
    let mut drawn = drawn();
    let form = Form {
        label: "Ban".into(),
        action: "ban".into(),
        given: vec![Value::Player {
            key: PlayerKey::new(7),
            name: String::new(),
        }],
    };
    drawn.after(&Click::Form(form.clone()));
    let open: Vec<String> = items::list(&view, drawn.form.as_ref(), true)
        .iter()
        .map(|item| item.target.id())
        .collect();
    assert_eq!(open, ["ban/1", "ban", "@cancel", "@close"]);
    assert_eq!(drawn.take(&view, &ask("@cancel", Act::Press, "")), None);
    assert_eq!(
        form_widgets(&Recorder::default(), &view, &form, &mut drawn),
        Click::Cancel
    );
    assert_eq!(
        drawn.take(&view, &ask("say", Act::Press, "")),
        Some(windows::Answer::Missing),
        "the form hides the window's own items"
    );
}

#[test]
fn a_lost_link_forgets_the_windows_so_none_draws_over_the_menu() {
    let _held = HELD.relock();
    heard("you.admin", "panel", true, "W\tPanel\n");
    assert_eq!(held().len(), 1);
    let _running = crate::engine::loader::install_for_test(crate::start::tests::bare_loader());
    crate::base::net::link_lost();
    assert!(STATE.held.load().is_none());
}

#[test]
fn a_listing_s_strings_stay_until_the_next_listing_on_its_thread() {
    let _held = HELD.relock();
    heard("you.admin", "panel", true, "W\tPanel\n");
    let lent = listed(<[ClientWindowItem]>::to_vec);
    let handed: Vec<String> = lent
        .iter()
        .map(|item| format!("{:?} {:?}", item.window, item.item))
        .collect();
    let kept: Vec<String> = LENT.with_borrow(|kept| {
        kept.iter()
            .map(|(window, id, _)| format!("{:?} {:?}", Str::new(window), Str::new(id)))
            .collect()
    });
    assert_eq!(
        handed, kept,
        "every string handed points into the kept listing"
    );
    // SAFETY: the listing stays until the next call on this thread.
    let read = |text: Str| unsafe { text.as_str() }.map(str::to_owned);
    assert_eq!(
        (read(lent[0].window), read(lent[0].item)),
        (Some("you.admin:panel".into()), Some("@close".into()))
    );
    assert_ne!(lent[0].flags & ClientWindowItem::SHOWN, 0);
    closed("you.admin", "panel");
}

#[test]
fn a_window_closed_or_forgotten_with_a_form_open_lists_no_form_when_it_reopens() {
    let _held = HELD.relock();
    let form = || {
        Some((
            "you.admin".to_owned(),
            "panel".to_owned(),
            Form {
                label: "Ban".into(),
                action: "ban".into(),
                given: Vec::new(),
            },
        ))
    };
    *STATE.form.relock() = form();
    closed("you.admin", "other");
    assert_eq!(*STATE.form.relock(), form(), "another window closed");
    closed("you.admin", "panel");
    assert_eq!(*STATE.form.relock(), None);
    *STATE.form.relock() = form();
    forget();
    assert_eq!(*STATE.form.relock(), None);
}

#[test]
fn a_commanded_press_counts_only_when_its_own_click_won_the_frame() {
    let view = ban_and_say();
    let mut drawn = drawn();
    assert_eq!(drawn.take(&view, &ask("ban#0", Act::Press, "")), None);
    // The player clicks a Ban too in the same frame, and Bob's row draws last.
    let paint = Recorder::default().answer("Ban", Answer::Press);
    let click = items(&paint, &view, &mut drawn);
    let row = |n| Target::RowButton("ban".into(), n);
    assert!(!made(&view, &row(0), &click), "Bob's row won the frame");
    assert!(made(&view, &row(1), &click));
    assert!(made(&view, &Target::Close, &Click::Close));
    assert!(
        !made(&view, &Target::Run("say".into()), &Click::Close),
        "the player closed the window"
    );
    assert!(
        !made(&view, &Target::Cancel, &Click::Nothing),
        "Back ended the typing"
    );
    let ann = Value::Player {
        key: PlayerKey::new(7),
        name: String::new(),
    };
    let flipped = Click::Run("allowed".into(), vec![ann, Value::Toggle(true)]);
    let switch = |key| Target::Switch("allowed".into(), PlayerKey::new(key));
    assert!(made(&view, &switch(7), &flipped));
    assert!(
        !made(&view, &switch(8), &flipped),
        "another player's switch"
    );
}

#[test]
fn a_commanded_map_press_is_off_on_a_build_that_cannot_open_the_map() {
    let mut drawn = drawn();
    let map = ask("@map", Act::Press, "");
    assert_eq!(drawn.take(&map_view(), &map), Some(windows::Answer::Off));
    drawn.map_opens = Some(5);
    assert_eq!(drawn.take(&map_view(), &map), None, "the draw presses");
}

#[test]
fn window_commands_are_refused_while_the_interface_frame_is_fused_off() {
    assert!(draws());
    super::super::INTERFACE_FUSE.blow("a test");
    assert!(!draws(), "no frame would answer the command");
}
