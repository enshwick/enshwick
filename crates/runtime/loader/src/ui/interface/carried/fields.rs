//! The field of one action argument in a carried window: what the player
//! set in it, how it draws, and the value it gives a run.
//!
//! A frame draws from the field's state and asks only whether it holds a
//! value; the value itself is built when the player runs the action.

use enshwick_core::PlayerKey;
use enshwick_core::abi::UiWidget;
use enshwick_core::actions::Value;
use enshwick_core::actions::sheet::ArgView;
use enshwick_core::content::Guid;
use enshwick_core::ui::MAX_OPTIONS;

use super::super::engine::TextBuffer;
use super::super::{Widgets, options};
use crate::engine::texts;

/// The most steps a number's slider takes.
const MAX_STEPS: f64 = 1_000.0;
/// A player field's pick while no player is picked: past every row.
const NO_PICK: u32 = u32::MAX;

/// What the player set in one field.
pub(super) struct Field {
    pub(super) pick: u32,
    /// The account `pick` named when it was last drawn.
    picked: Option<PlayerKey>,
    number: f32,
    on: bool,
    pub(super) text: Box<TextBuffer>,
    /// The item picked, for an item argument.
    item: Option<Guid>,
    /// The search the items in `found` match, and the language of their
    /// list; `None` before the first.
    searched: Option<(String, u32)>,
    /// The items of the list that match the search, at most
    /// [`MAX_OPTIONS`].
    found: Vec<usize>,
}

impl Field {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a slider takes an f32; the server checks the value it gets"
    )]
    pub(super) fn new(arg: &ArgView) -> Self {
        let mut field = Self {
            pick: if arg.kind == ArgView::PLAYER {
                NO_PICK
            } else {
                0
            },
            picked: None,
            number: arg.min as f32,
            on: false,
            text: TextBuffer::new(),
            item: None,
            searched: None,
            found: Vec::new(),
        };
        match &arg.default {
            Some(Value::Choice(at)) => field.pick = u32::try_from(*at).unwrap_or(0),
            Some(Value::Number(number)) => field.number = *number as f32,
            Some(Value::Toggle(on)) => field.on = *on,
            Some(Value::Text(text)) => field.text.set(text),
            Some(Value::Item(item)) => field.item = Some(*item),
            _ => {}
        }
        field
    }
}

/// Draws one argument's field with `state`; whether it holds a value.
pub(super) fn field(paint: &impl Widgets, arg: &ArgView, state: &mut Field) -> bool {
    let label = arg.label.as_bytes();
    match arg.kind {
        ArgView::PLAYER => {
            state.pick = followed(&arg.players, state.picked, state.pick);
            let (names, count) = options(arg.players.iter().map(|(_, name)| name.as_str()));
            if count == 0 {
                paint.label(label, UiWidget::STYLE_SECONDARY, false, false);
            } else if let Some(pick) = paint.dropdown(label, &names[..count], state.pick, false) {
                state.pick = pick;
                state.text.set("");
            }
            state.picked = picked(arg, state.pick);
            let _typed = paint.text_field(
                &mut state.text,
                texts::text("windows.name").as_bytes(),
                false,
            );
        }
        ArgView::CHOICE => {
            let (choices, count) = options(arg.choices.iter().map(String::as_str));
            if let Some(pick) = paint.dropdown(label, &choices[..count], state.pick, false) {
                state.pick = pick;
            }
        }
        ArgView::NUMBER => {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a slider takes an f32; the server checks the value it gets"
            )]
            let range = (arg.min as f32, arg.max as f32);
            if let Some(number) = paint.slider(label, state.number, range, steps(arg), false) {
                state.number = number;
            }
        }
        ArgView::TOGGLE => {
            if let Some(on) = paint.toggle(label, state.on, false) {
                state.on = on;
            }
        }
        ArgView::TEXT => {
            paint.label(label, UiWidget::STYLE_SECONDARY, false, false);
            let _typed = paint.text_field(
                &mut state.text,
                texts::text("windows.text").as_bytes(),
                false,
            );
        }
        ArgView::ITEM => item_field(paint, label, state),
        _ => {}
    }
    is_set(arg, state)
}

/// A field's value borrowed from its state, so a frame learns whether the
/// field holds one without building it.
#[derive(Debug, PartialEq)]
enum Given<'a> {
    /// Typed text, trimmed: a text argument's, or a player's name.
    Typed(&'a str),
    /// A value that owns no text.
    Value(Value),
}

impl Given<'_> {
    /// The value an action runs with.
    fn into_value(self) -> Value {
        match self {
            Given::Typed(text) => Value::Text(text.to_owned()),
            Given::Value(value) => value,
        }
    }
}

/// The value `state` holds for `arg`, borrowed; `None` while it holds none.
fn given<'a>(arg: &ArgView, state: &'a Field) -> Option<Given<'a>> {
    let owned = |value| Some(Given::Value(value));
    match arg.kind {
        ArgView::PLAYER => player_given(arg, state.pick, state.text.text()),
        ArgView::CHOICE => owned(Value::Choice(chosen(arg, state.pick)?)),
        ArgView::NUMBER => owned(Value::Number(snapped(arg, f64::from(state.number)))),
        ArgView::TOGGLE => owned(Value::Toggle(state.on)),
        ArgView::TEXT => text_given(arg, state.text.text()),
        ArgView::ITEM => owned(Value::Item(state.item?)),
        _ => None,
    }
}

/// Whether `state` holds a value for `arg`.
fn is_set(arg: &ArgView, state: &Field) -> bool {
    given(arg, state).is_some()
}

/// The value `state` holds for `arg`; `None` while it holds none.
pub(super) fn value(arg: &ArgView, state: &Field) -> Option<Value> {
    given(arg, state).map(Given::into_value)
}

/// The choice row `pick` of a choice argument, `None` past its choices.
fn chosen(arg: &ArgView, pick: u32) -> Option<usize> {
    usize::try_from(pick)
        .ok()
        .filter(|&pick| pick < arg.choices.len())
}

/// The account of row `pick` of a player argument's list, `None` past it.
fn picked(arg: &ArgView, pick: u32) -> Option<PlayerKey> {
    let (key, _) = arg.players.get(usize::try_from(pick).ok()?)?;
    Some(*key)
}

/// Draws an item argument's field: a search over the items this client's
/// game names in its language, and a list of those that match.
fn item_field(paint: &impl Widgets, label: &[u8], state: &mut Field) {
    let search_field = |state: &mut Field| {
        let _typed = paint.text_field(
            &mut state.text,
            texts::text("windows.item").as_bytes(),
            false,
        );
    };
    let Some(list) = crate::inventory::item_names::items(texts::language()) else {
        paint.label(label, UiWidget::STYLE_SECONDARY, false, false);
        search_field(state);
        return;
    };
    let search = (state.text.text(), list.language);
    if state
        .searched
        .as_ref()
        .is_none_or(|(text, language)| (text.as_str(), *language) != search)
    {
        state.found = crate::inventory::item_names::matching(&list.items, search.0, MAX_OPTIONS);
        crate::inventory::item_names::keep_pick(
            &mut state.found,
            &list.items,
            state.item,
            MAX_OPTIONS,
        );
        state.searched = Some((search.0.to_owned(), search.1));
    }
    let (names, count) = options(state.found.iter().map(|&at| list.items[at].name.as_str()));
    let shown = |item: Guid| {
        state
            .found
            .iter()
            .position(|&at| list.items[at].item == item)
    };
    let pick = state.item.and_then(shown);
    let index = pick
        .and_then(|at| u32::try_from(at).ok())
        .unwrap_or(NO_PICK);
    let chosen = if count == 0 {
        paint.label(label, UiWidget::STYLE_SECONDARY, false, false);
        None
    } else {
        paint.dropdown(label, &names[..count], index, false)
    };
    // The pick stays until the player picks another, whatever the search shows.
    if let Some(&at) = chosen
        .and_then(|at| usize::try_from(at).ok())
        .and_then(|at| state.found.get(at))
    {
        state.item = Some(list.items[at].item);
    }
    search_field(state);
}

/// The row of the account `picked` in `players` as an update left them: a
/// join or a leave moves the rows under the player's pick, and an account
/// that is gone leaves no pick rather than the player now in its row. With
/// no account picked yet, `pick`.
fn followed(players: &[(PlayerKey, String)], picked: Option<PlayerKey>, pick: u32) -> u32 {
    let Some(key) = picked else {
        return pick;
    };
    players
        .iter()
        .position(|(k, _)| *k == key)
        .and_then(|at| u32::try_from(at).ok())
        .unwrap_or(NO_PICK)
}

/// A player argument's value: the typed name when there is one, which the
/// server looks up, else the player picked from the list.
fn player_given<'a>(arg: &ArgView, pick: u32, typed: &'a str) -> Option<Given<'a>> {
    let typed = typed.trim();
    if !typed.is_empty() {
        return Some(Given::Typed(typed));
    }
    Some(Given::Value(Value::Player {
        key: picked(arg, pick)?,
        name: String::new(),
    }))
}

/// A text argument's value: the typed text within its bytes.
fn text_given<'a>(arg: &ArgView, typed: &'a str) -> Option<Given<'a>> {
    text_fits(arg, typed).then(|| Given::Typed(typed.trim()))
}

/// Whether `typed`, trimmed, is a text argument's value: not empty and
/// within its bytes.
fn text_fits(arg: &ArgView, typed: &str) -> bool {
    u16::try_from(typed.trim().len()).is_ok_and(|len| len > 0 && f64::from(len) <= arg.max)
}

/// The steps of a number's slider: one per step of its range, 0 for any
/// value or more than [`MAX_STEPS`].
fn steps(arg: &ArgView) -> u32 {
    let count = ((arg.max - arg.min) / arg.step).round() + 1.0;
    if arg.step > 0.0 && count.is_finite() && (1.0..=MAX_STEPS).contains(&count) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked above: a whole number from 1 to MAX_STEPS"
        )]
        return count as u32;
    }
    0
}

/// `value` on its argument's step and within its range.
fn snapped(arg: &ArgView, value: f64) -> f64 {
    let stepped = if arg.step > 0.0 {
        arg.min + ((value - arg.min) / arg.step).round() * arg.step
    } else {
        value
    };
    // Not `clamp`: the range comes from the server's text, and `clamp` panics
    // on a reversed or NaN one.
    stepped.max(arg.min).min(arg.max)
}

#[cfg(test)]
mod tests {
    use super::super::super::recorder::{Answer, Drew, Recorder};
    use super::*;

    fn ann_and_bob() -> ArgView {
        ArgView {
            kind: ArgView::PLAYER,
            label: "Player".into(),
            players: vec![
                (PlayerKey::new(7), "Ann".into()),
                (PlayerKey::new(8), "Bob".into()),
            ],
            ..ArgView::default()
        }
    }

    #[test]
    fn a_player_field_lists_the_players_and_gives_the_one_picked() {
        let arg = ann_and_bob();
        let mut state = Field::new(&arg);
        let untouched = Recorder::default();
        assert!(!field(&untouched, &arg, &mut state), "no one is picked yet");
        assert_eq!(
            untouched.drew(),
            [
                Drew::Dropdown {
                    label: "Player".into(),
                    options: vec!["Ann".into(), "Bob".into()],
                    index: NO_PICK,
                },
                Drew::TextField {
                    hint: texts::text("windows.name").into()
                },
            ]
        );
        let picks = Recorder::default().answer("Player", Answer::Pick(1));
        assert!(field(&picks, &arg, &mut state));
        assert_eq!(state.picked, Some(PlayerKey::new(8)));
        assert_eq!(
            value(&arg, &state),
            Some(Value::Player {
                key: PlayerKey::new(8),
                name: String::new()
            })
        );
    }

    #[test]
    fn a_name_typed_in_a_player_field_wins_over_the_pick() {
        let arg = ann_and_bob();
        let mut state = Field::new(&arg);
        let types =
            Recorder::default().answer(texts::text("windows.name"), Answer::Type(" Zed ".into()));
        assert!(field(&types, &arg, &mut state), "a typed name is a value");
        assert_eq!(value(&arg, &state), Some(Value::Text("Zed".into())));
    }

    #[test]
    fn a_player_field_without_players_shows_its_label_and_the_name_field() {
        let arg = ArgView {
            players: Vec::new(),
            ..ann_and_bob()
        };
        let paint = Recorder::default();
        assert!(!field(&paint, &arg, &mut Field::new(&arg)));
        assert_eq!(
            paint.drew(),
            [
                Drew::Label("Player".into()),
                Drew::TextField {
                    hint: texts::text("windows.name").into()
                },
            ]
        );
    }

    #[test]
    fn a_text_field_holds_a_value_only_once_text_within_its_bytes_is_typed() {
        let arg = ArgView {
            kind: ArgView::TEXT,
            label: "Reason".into(),
            max: 4.0,
            ..ArgView::default()
        };
        let hint = texts::text("windows.text");
        let mut state = Field::new(&arg);
        assert!(!field(&Recorder::default(), &arg, &mut state));
        let long = Recorder::default().answer(hint, Answer::Type("griefing".into()));
        assert!(!field(&long, &arg, &mut state), "past its bytes");
        let fits = Recorder::default().answer(hint, Answer::Type("spam".into()));
        assert!(field(&fits, &arg, &mut state));
        assert_eq!(value(&arg, &state), Some(Value::Text("spam".into())));
    }

    #[test]
    fn choice_number_and_toggle_fields_take_what_the_player_set() {
        let choice = ArgView {
            kind: ArgView::CHOICE,
            label: "Kind".into(),
            choices: vec!["Basic".into(), "Pro".into()],
            ..ArgView::default()
        };
        let mut state = Field::new(&choice);
        let picks = Recorder::default().answer("Kind", Answer::Pick(1));
        assert!(field(&picks, &choice, &mut state));
        assert_eq!(value(&choice, &state), Some(Value::Choice(1)));
        let past = Recorder::default().answer("Kind", Answer::Pick(5));
        assert!(!field(&past, &choice, &mut state), "a row past the choices");

        let hours = ArgView {
            kind: ArgView::NUMBER,
            label: "Hours".into(),
            min: 1.0,
            max: 48.0,
            step: 1.0,
            ..ArgView::default()
        };
        let mut state = Field::new(&hours);
        let slides = Recorder::default().answer("Hours", Answer::Slide(3.4));
        assert!(field(&slides, &hours, &mut state));
        assert_eq!(value(&hours, &state), Some(Value::Number(3.0)));

        let bound = ArgView {
            kind: ArgView::TOGGLE,
            label: "Bound".into(),
            ..ArgView::default()
        };
        let mut state = Field::new(&bound);
        let flips = Recorder::default().answer("Bound", Answer::Flip(true));
        assert!(field(&flips, &bound, &mut state));
        assert_eq!(value(&bound, &state), Some(Value::Toggle(true)));
    }

    #[test]
    fn fields_give_values_within_their_arguments() {
        let players = ArgView {
            kind: ArgView::PLAYER,
            players: vec![(PlayerKey::new(7), "Ann".into())],
            ..ArgView::default()
        };
        assert_eq!(
            player_given(&players, 0, ""),
            Some(Given::Value(Value::Player {
                key: PlayerKey::new(7),
                name: String::new()
            }))
        );
        assert_eq!(
            player_given(&players, 0, " Bob "),
            Some(Given::Typed("Bob"))
        );
        assert_eq!(player_given(&players, 3, ""), None);

        let text = ArgView {
            kind: ArgView::TEXT,
            min: 1.0,
            max: 4.0,
            ..ArgView::default()
        };
        assert_eq!(text_given(&text, " hi "), Some(Given::Typed("hi")));
        assert_eq!(text_given(&text, "toolong"), None);
        assert_eq!(text_given(&text, "  "), None);

        let hours = ArgView {
            kind: ArgView::NUMBER,
            min: 1.0,
            max: 48.0,
            step: 1.0,
            ..ArgView::default()
        };
        assert_eq!(steps(&hours), 48);
        assert!((snapped(&hours, 3.4) - 3.0).abs() < f64::EPSILON);
        assert!((snapped(&hours, 99.0) - 48.0).abs() < f64::EPSILON);
        let any = ArgView { step: 0.0, ..hours };
        assert_eq!(steps(&any), 0);
        let fine = ArgView {
            step: 0.001,
            ..any.clone()
        };
        assert_eq!(steps(&fine), 0, "past MAX_STEPS the slider takes any value");
    }

    #[test]
    fn a_number_snaps_to_its_step_from_the_least_value_and_any_value_stays() {
        let even = ArgView {
            kind: ArgView::NUMBER,
            min: 1.0,
            max: 9.0,
            step: 2.0,
            ..ArgView::default()
        };
        assert!((snapped(&even, 4.2) - 5.0).abs() < f64::EPSILON);
        let any = ArgView { step: 0.0, ..even };
        assert!((snapped(&any, 4.2) - 4.2).abs() < f64::EPSILON);
    }

    #[test]
    fn a_reversed_or_unreadable_range_from_the_server_does_not_panic() {
        let reversed = ArgView {
            kind: ArgView::NUMBER,
            min: 9.0,
            max: 1.0,
            ..ArgView::default()
        };
        assert!((snapped(&reversed, 4.0) - 1.0).abs() < f64::EPSILON);
        let unreadable = ArgView {
            min: f64::NAN,
            max: f64::NAN,
            ..reversed
        };
        assert!((snapped(&unreadable, 4.0) - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_player_pick_follows_its_account_and_goes_when_the_account_does() {
        let (ann, bob) = (PlayerKey::new(7), PlayerKey::new(8));
        let both = [(ann, "Ann".to_owned()), (bob, "Bob".to_owned())];
        assert_eq!(followed(&both, None, 1), 1, "nothing drawn yet");
        assert_eq!(followed(&both, Some(bob), 1), 1);
        assert_eq!(followed(&both[1..], Some(bob), 1), 0, "Ann left");
        let carl = [
            (ann, "Ann".to_owned()),
            (PlayerKey::new(9), "Carl".to_owned()),
        ];
        let pick = followed(&carl, Some(bob), 1);
        assert_eq!(
            pick, NO_PICK,
            "Bob left: Carl, now in his row, is not picked"
        );
        let arg = ArgView {
            kind: ArgView::PLAYER,
            players: carl.to_vec(),
            ..ArgView::default()
        };
        assert_eq!(player_given(&arg, pick, ""), None, "nothing to run on");
        assert_eq!(
            Field::new(&arg).pick,
            NO_PICK,
            "a player is picked, never assumed"
        );
        let choice = ArgView {
            kind: ArgView::CHOICE,
            ..ArgView::default()
        };
        assert_eq!(Field::new(&choice).pick, 0);
    }

    #[test]
    fn a_field_opens_with_its_argument_s_default() {
        let with = |kind: u8, default: Value| ArgView {
            kind,
            min: 1.0,
            max: 99.0,
            default: Some(default),
            ..ArgView::default()
        };
        let count = Field::new(&with(ArgView::NUMBER, Value::Number(5.0)));
        assert!((count.number - 5.0).abs() < f32::EPSILON);
        assert!(Field::new(&with(ArgView::TOGGLE, Value::Toggle(true))).on);
        assert_eq!(Field::new(&with(ArgView::CHOICE, Value::Choice(2))).pick, 2);
        let note = Field::new(&with(ArgView::TEXT, Value::Text("spam".into())));
        assert_eq!(note.text.text(), "spam");
        let torch = Guid([7; 16]);
        assert_eq!(
            Field::new(&with(ArgView::ITEM, Value::Item(torch))).item,
            Some(torch)
        );
        let plain = Field::new(&ArgView {
            kind: ArgView::NUMBER,
            min: 1.0,
            ..ArgView::default()
        });
        assert!(
            (plain.number - 1.0).abs() < f32::EPSILON,
            "no default: the least value"
        );
    }
}
