//! Whether values fit an action's arguments, and whether its optional
//! arguments are well declared.

use super::super::Action;
use super::super::request::{Arg, Value};
use super::super::sheet::ArgView;

/// What is wrong with the optional arguments among `args`: one before an
/// argument that is not optional, an optional player, or a default its
/// argument does not take; `None` when they are well declared.
pub(in crate::actions) fn optional_fault(args: &[Arg]) -> Option<String> {
    let required = args
        .iter()
        .rposition(|arg| arg.default.is_none())
        .map_or(0, |last| last + 1);
    if args[..required].iter().any(|arg| arg.default.is_some()) {
        return Some("only its last arguments may be optional".to_owned());
    }
    let last = args.len().saturating_sub(1);
    let early_text = |(i, arg): (usize, &Arg)| i < last && takes_line(arg) && arg.default.is_some();
    if args.iter().enumerate().any(early_text) {
        return Some("an optional text or item must be its last argument".to_owned());
    }
    let at = args.iter().position(|arg| match &arg.default {
        None => false,
        Some(_) if arg.kind == ArgView::PLAYER => true,
        Some(Value::Text(text)) if arg.kind == ArgView::TEXT && text.is_empty() => false,
        Some(default) => !takes(default, arg),
    })?;
    Some(format!(
        "its argument {} is a player, which is never optional, or does not take its default",
        at + 1
    ))
}

/// Whether `arg` reads words as a text does, so that, last on a typed line,
/// it takes the rest of the line: a text or an item.
pub(in crate::actions) fn takes_line(arg: &Arg) -> bool {
    matches!(arg.kind, ArgView::TEXT | ArgView::ITEM)
}

/// Whether `value` fits `arg`: its type and range, or the default the
/// argument takes when a typed line leaves it out.
pub(in crate::actions) fn fits(value: &Value, arg: &Arg) -> bool {
    arg.default.as_ref() == Some(value) || takes(value, arg)
}

/// Whether `value` is of `arg`'s type and within its range.
fn takes(value: &Value, arg: &Arg) -> bool {
    match (value, arg.kind) {
        (Value::Player { .. }, ArgView::PLAYER)
        | (Value::Toggle(_), ArgView::TOGGLE)
        | (Value::Item(_), ArgView::ITEM) => true,
        (Value::Choice(index), ArgView::CHOICE) => *index < arg.choices.len(),
        (Value::Number(n), ArgView::NUMBER) => {
            n.is_finite() && *n >= arg.range.0 && *n <= arg.range.1
        }
        (Value::Text(text), ArgView::TEXT) => arg.takes_text(text),
        _ => false,
    }
}

/// What is wrong with the `values` a window's button gives `action` first:
/// a list takes its player and on or off, any other action no more values
/// than its arguments, each fitting its own; `None` when they fit.
pub(in crate::actions) fn button_fault(action: &Action, values: &[Value]) -> Option<String> {
    let id = &action.id;
    if action.on.is_some() {
        return (!matches!(values, [Value::Player { .. }, Value::Toggle(_)]))
            .then(|| format!("a button gives the list {id} other than a player and on or off"));
    }
    if values.len() > action.args.len() {
        return Some(format!(
            "a button gives {} values to {id}, which takes {}",
            values.len(),
            action.args.len()
        ));
    }
    let at = values
        .iter()
        .zip(&action.args)
        .position(|(v, a)| !fits(v, a))?;
    Some(format!(
        "a button gives {id} a value its argument {} does not take",
        at + 1
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlayerKey;

    #[test]
    fn only_the_last_arguments_may_be_optional_and_each_takes_its_default() {
        let count = || Arg::number("Count", 1.0..=99.0, 1.0);
        let one = Value::Number(1.0);
        assert_eq!(optional_fault(&[]), None);
        assert_eq!(
            optional_fault(&[Arg::text("Item", 8), count().optional(one.clone())]),
            None
        );
        assert_eq!(
            optional_fault(&[Arg::text("Why", 8).optional(Value::Text(String::new()))]),
            None,
            "an empty text may stand for a text left out"
        );
        let first = optional_fault(&[count().optional(one.clone()), Arg::text("Item", 8)]);
        assert!(first.unwrap().contains("only its last arguments"));
        let note = || Arg::text("Note", 8).optional(Value::Text(String::new()));
        let mid = optional_fault(&[note(), count().optional(one.clone())]).unwrap();
        assert!(
            mid.contains("an optional text or item must be its last"),
            "{mid}"
        );
        let torch = Value::Item(crate::content::Guid([1; 16]));
        let early = optional_fault(&[
            Arg::item("Item").optional(torch),
            count().optional(one.clone()),
        ]);
        assert!(
            early.unwrap().contains("must be its last"),
            "an optional item too"
        );
        assert_eq!(
            optional_fault(&[count().optional(one.clone()), note()]),
            None
        );
        let far = optional_fault(&[count().optional(Value::Number(100.0))]).unwrap();
        assert!(far.contains("argument 1"), "{far}");
        let other = optional_fault(&[Arg::text("Item", 8), count().optional(Value::Toggle(true))]);
        assert!(other.unwrap().contains("argument 2"));
        let who = Value::Player {
            key: PlayerKey::new(7),
            name: "Ann".into(),
        };
        assert!(optional_fault(&[Arg::player("Player").optional(who)]).is_some());
        let blank = optional_fault(&[count().optional(Value::Text(String::new()))]);
        assert!(
            blank.unwrap().contains("argument 1"),
            "an empty text stands only for a text"
        );
    }

    #[test]
    fn a_button_s_values_fit_its_action_or_name_their_fault() {
        let ann = Value::Player {
            key: PlayerKey::new(7),
            name: "Ann".into(),
        };
        let list = Action::per_player("allowed", "Allowed", |_, _| Vec::new());
        let whole = [ann.clone(), Value::Toggle(true)];
        assert_eq!(button_fault(&list, &whole), None);
        for short in [&[][..], &whole[..1], &[Value::Toggle(true), ann.clone()]] {
            let fault = button_fault(&list, short).expect("a list takes a player and on or off");
            assert!(fault.contains("the list allowed"), "{fault}");
        }

        let ban = Action::new("ban", "Ban")
            .arg(Arg::player("Player"))
            .arg(Arg::text("Reason", 8));
        assert_eq!(button_fault(&ban, &[]), None);
        assert_eq!(button_fault(&ban, core::slice::from_ref(&ann)), None);
        let long = Value::Text("too long a reason".into());
        let fault = button_fault(&ban, &[ann.clone(), long]).unwrap();
        assert!(
            fault.contains("ban") && fault.contains("argument 2"),
            "{fault}"
        );
        let three = [ann.clone(), Value::Text("ok".into()), ann];
        let fault = button_fault(&ban, &three).unwrap();
        assert!(fault.contains("3 values to ban, which takes 2"), "{fault}");

        let spawn = Action::new("spawn", "Spawn").arg(Arg::item("Item"));
        let torch = Value::Item(crate::content::Guid([1; 16]));
        assert_eq!(button_fault(&spawn, &[torch]), None);
        let fault = button_fault(&spawn, &[Value::Text("Torch".into())]).unwrap();
        assert!(
            fault.contains("argument 1"),
            "a text in an item's place: {fault}"
        );
    }
}
