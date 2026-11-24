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

fn held() -> Vec<(String, String)> {
    let held = STATE.held.load();
    let held = held.as_deref().map_or_default(Vec::as_slice);
    held.iter()
        .map(|h| (h.mod_id.clone(), h.window.clone()))
        .collect()
}

#[test]
fn the_server_s_close_takes_only_its_own_window() {
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

#[test]
fn a_lost_link_forgets_the_windows_so_none_draws_over_the_menu() {
    heard("you.admin", "panel", true, "W\tPanel\n");
    assert_eq!(held().len(), 1);
    crate::base::net::link_lost();
    assert!(STATE.held.load().is_none());
}
