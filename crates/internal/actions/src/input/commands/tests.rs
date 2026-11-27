//! The commands' raw forms and their Luau reading.

use super::luau::LuauCommand;
use super::*;
use crate::controls::Key;

/// A Luau command of `kind` in session 1, every other field unset.
fn luau(kind: &str) -> LuauCommand {
    LuauCommand {
        kind: kind.to_owned(),
        session: 1,
        ..LuauCommand::default()
    }
}

#[test]
fn each_action_fills_its_own_fields_of_the_raw_command() {
    let open =
        ClientCommand::new(CommandAction::OpenScreen("Map".to_owned()), 2).on_screen("InGame");
    let raw = open.raw();
    assert_eq!((raw.kind, raw.session), (RawCommand::OPEN_SCREEN, 2));
    // SAFETY: the view points at `open`'s target, alive here.
    let target = unsafe { raw.target.as_str() };
    // SAFETY: the view points at `open`'s screen, alive here.
    let screen = unsafe { raw.expected_screen.as_str() };
    assert_eq!((target, screen), (Some("Map"), Some("InGame")));
    let walk = ClientCommand::new(
        CommandAction::Move {
            direction: [0.0, 1.0],
            hold_ms: 250,
        },
        2,
    )
    .raw();
    assert_eq!(
        (walk.kind, walk.direction, walk.hold_ms),
        (RawCommand::MOVE, [0.0, 1.0], 250)
    );
}

#[test]
fn a_luau_command_needs_what_its_kind_takes() {
    let open = |target: Option<&str>| LuauCommand {
        target: target.map(str::to_owned),
        ..luau("open_screen")
    };
    assert!(ClientCommand::try_from(open(Some("Map"))).is_ok());
    assert!(ClientCommand::try_from(open(None)).is_err());
    assert!(ClientCommand::try_from(luau("dance")).is_err());
    assert!(
        ClientCommand::try_from(luau("answer_dialog")).is_err(),
        "an answer needs its response"
    );
    let answer = |dialog| LuauCommand {
        answer: Some(Response::Ok),
        dialog,
        ..luau("answer_dialog")
    };
    assert!(
        ClientCommand::try_from(answer(None)).is_err(),
        "an answer needs its dialog"
    );
    let command = ClientCommand::try_from(answer(Some(4))).expect("a whole answer");
    let raw = command.raw();
    assert_eq!(
        (raw.kind, raw.dialog, raw.answer),
        (RawCommand::ANSWER_DIALOG, 4, Response::Ok.to_raw())
    );
    assert!(
        ClientCommand::try_from(luau("close_screen"))
            .is_ok_and(|c| c.action == CommandAction::CloseScreen)
    );
}

#[test]
fn a_press_carries_its_action_s_game_name_and_its_hold() {
    let press = ClientCommand::new(
        CommandAction::Press {
            action: GameAction::ContextualActionTap,
            hold_ms: 60,
        },
        3,
    )
    .raw();
    // SAFETY: the view points at the action's static name.
    let target = unsafe { press.target.as_str() };
    assert_eq!(
        (press.kind, target, press.hold_ms, press.session),
        (RawCommand::PRESS, Some("ContextualAction_Tap"), 60, 3)
    );
}

#[test]
fn item_commands_carry_their_slots_counts_and_entities_within_bounds() {
    let craft = ClientCommand::new(
        CommandAction::Craft {
            recipe: RecipeId(0x7cd8_0a04),
            count: 2,
            station: None,
        },
        1,
    )
    .raw();
    assert_eq!(
        (craft.kind, craft.amount, craft.entity),
        (RawCommand::CRAFT, 2, Entity::from_engine(0))
    );
    assert!(craft.in_bounds());
    assert!(
        !enshwick_abi::headed!(RawCommand { amount: 0, ..craft }).in_bounds(),
        "no craft"
    );
    let (from, to) = (
        SlotRef::new(Entity::from_engine(0x41), 2),
        SlotRef::new(Entity::from_engine(0x42), 0),
    );
    let split = ClientCommand::new(
        CommandAction::MoveItem {
            from,
            to,
            count: NonZeroU16::new(5),
        },
        1,
    )
    .raw();
    assert_eq!(
        (split.slot, split.target_slot, split.amount),
        (from.0, to.0, 5)
    );
    assert!(split.in_bounds());
    assert!(
        !enshwick_abi::headed!(RawCommand {
            target_slot: from.0,
            ..split
        })
        .in_bounds(),
        "a move onto its own slot"
    );
    let whole = ClientCommand::new(
        CommandAction::MoveItem {
            from,
            to,
            count: None,
        },
        1,
    )
    .raw();
    assert_eq!(whole.amount, 0, "the whole stack");
    let take = ClientCommand::new(CommandAction::TakeAll(Entity::from_engine(500)), 1).raw();
    assert_eq!(
        (take.kind, take.entity),
        (RawCommand::TAKE_ALL, Entity::from_engine(500))
    );
    assert!(
        !enshwick_abi::headed!(RawCommand {
            entity: Entity::from_engine(0),
            ..take
        })
        .in_bounds()
    );
    let trash = ClientCommand::new(CommandAction::Trash(from), 1).raw();
    assert_eq!((trash.kind, trash.slot), (RawCommand::TRASH, from.0));
}

#[test]
fn a_luau_move_item_takes_a_count_above_zero_or_none_for_the_whole_stack() {
    let (from, to) = (
        SlotRef::new(Entity::from_engine(9), 0),
        SlotRef::new(Entity::from_engine(9), 1),
    );
    let moved = |count: Option<u16>| {
        ClientCommand::try_from(LuauCommand {
            count,
            slot: Some(from),
            target_slot: Some(to),
            ..luau("move_item")
        })
        .map(|c| c.action)
        .ok()
    };
    assert_eq!(
        moved(Some(4)),
        Some(CommandAction::MoveItem {
            from,
            to,
            count: NonZeroU16::new(4),
        })
    );
    assert_eq!(
        moved(None),
        Some(CommandAction::MoveItem {
            from,
            to,
            count: None,
        })
    );
    assert_eq!(moved(Some(0)), None, "a count of 0 moves nothing");
}

#[test]
fn a_luau_press_needs_a_game_action_by_its_game_name_and_a_hold() {
    let press = |action: Option<&str>, hold_ms: Option<u32>| LuauCommand {
        hold_ms,
        action: action.map(str::to_owned),
        ..luau("press")
    };
    assert!(
        ClientCommand::try_from(press(Some("MainhandAction_Tap"), Some(16))).is_ok_and(|c| {
            c.action
                == CommandAction::Press {
                    action: GameAction::MainhandActionTap,
                    hold_ms: 16,
                }
        })
    );
    assert!(ClientCommand::try_from(press(Some("Dance"), Some(16))).is_err());
    assert!(ClientCommand::try_from(press(None, Some(16))).is_err());
    assert!(ClientCommand::try_from(press(Some("Jump"), None)).is_err());
}

#[test]
fn a_rebind_carries_its_mod_control_slot_and_key_bits_and_none_clears() {
    let key = Key::parse("Alt+G").expect("a key");
    let rebind = |second, key| {
        ClientCommand::new(
            CommandAction::Rebind {
                mod_id: "you.lights".to_owned(),
                control: "toggle".to_owned(),
                second,
                key,
            },
            1,
        )
    };
    let bound = rebind(true, Some(key));
    let raw = bound.raw();
    // SAFETY: the view points at `bound`'s mod id, alive here.
    let target = unsafe { raw.target.as_str() };
    // SAFETY: the view points at `bound`'s control, alive here.
    let item = unsafe { raw.item.as_str() };
    assert_eq!(
        (raw.kind, target, item, raw.key_slot, raw.key),
        (
            RawCommand::REBIND,
            Some("you.lights"),
            Some("toggle"),
            1,
            key.bits()
        )
    );
    assert!(raw.in_bounds());
    let cleared = rebind(false, None).raw();
    assert_eq!((cleared.key_slot, cleared.key), (0, 0), "no key clears it");
    assert!(
        !enshwick_abi::headed!(RawCommand {
            key_slot: 2,
            ..cleared
        })
        .in_bounds(),
        "a control has two keys"
    );
}

#[test]
fn a_setting_carries_its_mod_key_and_toml_value_within_its_bound() {
    let set = |value: String| {
        ClientCommand::new(
            CommandAction::SetSetting {
                mod_id: "you.lights".to_owned(),
                key: "range".to_owned(),
                value,
            },
            1,
        )
    };
    let command = set("12".to_owned());
    let raw = command.raw();
    // SAFETY: the view points at `command`'s key, alive here.
    let item = unsafe { raw.item.as_str() };
    // SAFETY: the view points at `command`'s value, alive here.
    let text = unsafe { raw.text.as_str() };
    assert_eq!(
        (raw.kind, item, text),
        (RawCommand::SET_SETTING, Some("range"), Some("12"))
    );
    assert!(raw.in_bounds());
    for refused in [String::new(), "1".repeat(CommandAction::MAX_SETTING + 1)] {
        let command = set(refused);
        assert!(!command.raw().in_bounds());
    }
    assert!(
        set("1".repeat(CommandAction::MAX_SETTING))
            .raw()
            .in_bounds()
    );
}

#[test]
fn a_quit_runs_only_in_session_zero() {
    let quit = ClientCommand::new(CommandAction::Quit, 0).raw();
    assert_eq!(quit.kind, RawCommand::QUIT);
    assert!(quit.in_bounds());
    assert!(!ClientCommand::new(CommandAction::Quit, 3).raw().in_bounds());
}

#[test]
fn luau_rebinds_settings_and_quits_read_as_their_actions() {
    let key = Key::parse("F5").expect("a key");
    let rebind = LuauCommand {
        target: Some("you.lights".to_owned()),
        control: Some("toggle".to_owned()),
        key: Some(key),
        ..luau("rebind")
    };
    assert_eq!(
        ClientCommand::try_from(rebind.clone())
            .map(|c| c.action)
            .ok(),
        Some(CommandAction::Rebind {
            mod_id: "you.lights".to_owned(),
            control: "toggle".to_owned(),
            second: false,
            key: Some(key),
        }),
        "the first key, without `second`"
    );
    assert!(
        ClientCommand::try_from(LuauCommand {
            control: None,
            ..rebind
        })
        .is_err(),
        "a rebind needs its control"
    );
    let set = LuauCommand {
        target: Some("you.lights".to_owned()),
        setting: Some("range".to_owned()),
        value: Some("12".to_owned()),
        ..luau("set_setting")
    };
    assert_eq!(
        ClientCommand::try_from(set.clone()).map(|c| c.action).ok(),
        Some(CommandAction::SetSetting {
            mod_id: "you.lights".to_owned(),
            key: "range".to_owned(),
            value: "12".to_owned(),
        })
    );
    assert!(
        ClientCommand::try_from(LuauCommand { value: None, ..set }).is_err(),
        "a setting needs its value"
    );
    assert!(ClientCommand::try_from(luau("quit")).is_ok_and(|c| c.action == CommandAction::Quit));
}

#[test]
fn a_cancelled_status_names_its_reason() {
    let raw = enshwick_abi::headed!(Raw {
        state: Raw::CANCELLED,
        reason: Raw::PLAYER_INPUT,
        ..Raw::EMPTY
    });
    assert_eq!(
        CommandState::from_raw(&raw),
        CommandState::Cancelled(CancelReason::PlayerInput)
    );
    assert_eq!(CancelReason::PlayerInput.name(), "player_input");
    let refused = enshwick_abi::headed!(Raw {
        reason: Raw::CHANGE_REFUSED,
        ..raw
    });
    assert_eq!(
        CommandState::from_raw(&refused),
        CommandState::Cancelled(CancelReason::ChangeRefused)
    );
    assert_eq!(CancelReason::ChangeRefused.name(), "change_refused");
}

#[test]
fn a_luau_join_never_shows_its_password_in_a_debug_print() {
    let join = LuauCommand {
        session: 0,
        target: Some("127.0.0.1:15637".to_owned()),
        password: Some("hunter2".to_owned()),
        ..luau("join")
    };
    let shown = format!("{join:?}");
    assert!(!shown.contains("hunter2"), "{shown}");
    assert!(shown.contains("127.0.0.1:15637"));
}

#[test]
fn a_join_carries_its_server_and_password_and_a_failed_one_names_why() {
    let join = ClientCommand::new(
        CommandAction::Join {
            server: "127.0.0.1:15637".to_owned(),
            password: Some(Password::new("hunter2")),
        },
        0,
    );
    let raw = join.raw();
    assert_eq!(raw.kind, RawCommand::JOIN);
    // SAFETY: the view points at `join`'s server, alive here.
    let target = unsafe { raw.target.as_str() };
    // SAFETY: the view points at `join`'s password, alive here.
    let password = unsafe { raw.password.as_str() };
    assert_eq!(
        (target, password),
        (Some("127.0.0.1:15637"), Some("hunter2"))
    );
    assert!(
        !format!("{join:?}").contains("hunter2"),
        "the password never shows"
    );
    let failed = enshwick_abi::headed!(Raw {
        state: Raw::FAILED,
        failure: Raw::FULL,
        phase: Raw::PHASE_MENU,
        ..Raw::EMPTY
    });
    assert_eq!(
        CommandState::from_raw(&failed),
        CommandState::Failed(JoinFailure::Full)
    );
    assert_eq!(JoinPhase::from_raw(failed.phase), Some(JoinPhase::Menu));
    assert_eq!(JoinPhase::from_raw(0), None, "no phase for other kinds");
    assert_eq!(JoinFailure::from_raw(99), JoinFailure::Other(99));
}

#[test]
fn a_reason_a_newer_loader_adds_reads_as_other_not_as_a_known_one() {
    let raw = enshwick_abi::headed!(Raw {
        state: Raw::CANCELLED,
        reason: 99,
        ..Raw::EMPTY
    });
    assert_eq!(
        CommandState::from_raw(&raw),
        CommandState::Cancelled(CancelReason::Other(99))
    );
    assert_eq!(CancelReason::Other(99).name(), "other");
}
