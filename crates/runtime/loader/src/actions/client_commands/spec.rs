//! A mod's command spec read into the kind it asks for, its arguments
//! checked against the running build.

use enshwick_core::__private::rules;
use enshwick_core::abi::{ClientCommand as Spec, Str};
use enshwick_core::controls::GameAction;
use enshwick_gamedata::catalogue;

use super::play::Host;
use super::{CommandError, Kind, ModsAsk, dialogs, windows};
use crate::actions::client_control::{Act, Mark, MenuTab};
use crate::engine::loader::Loader;

/// The screen byte named `name` on this build.
pub(super) fn screen_byte(name: &str) -> Option<u8> {
    crate::engine::screens::byte(name)
}

/// The text of a spec's name, empty for none, for the call: the command
/// keeps only the screen bytes and tabs found from it.
///
/// # Safety
///
/// - `name` is empty or points at text valid for `'a`, as the mod's call
///   borrows it.
pub(super) unsafe fn text<'a>(name: Str) -> Result<&'a str, CommandError> {
    // SAFETY: the caller's contract.
    unsafe { name.as_str() }.ok_or(CommandError::BadArgs)
}

/// `at` when it lies inside the map's frame the loader last read; with no
/// frame read, as on a client joined to a server, any `at` within the
/// fixed-point range, which the ABI's bounds checked.
///
/// # Errors
///
/// [`CommandError::BadArgs`] outside the frame.
fn on_map(at: [f64; 3]) -> Result<[f64; 3], CommandError> {
    inside_frame(crate::world::scene::last_frame(), at)
        .then_some(at)
        .ok_or(CommandError::BadArgs)
}

/// Whether `at` lies inside `frame`, or there is none to check.
pub(super) fn inside_frame(frame: Option<enshwick_core::abi::MapFrameInfo>, at: [f64; 3]) -> bool {
    frame.is_none_or(|frame| {
        enshwick_base::__internal::waypoints::inside(frame.origin, frame.size, at)
    })
}

/// The kind `spec` asks for on `loader`'s build, its arguments checked.
pub(super) fn kind(loader: &Loader, spec: &Spec) -> Result<Kind, CommandError> {
    // SAFETY: the mod's spec, its text borrowed for the call.
    let target = unsafe { text(spec.target) }?;
    match spec.kind {
        Spec::OPEN_SCREEN => {
            if !rules::CLIENT_OPENS.contains(&target) {
                return Err(CommandError::BadArgs);
            }
            screen_byte(target)
                .map(Kind::Open)
                .ok_or(CommandError::Unavailable)
        }
        Spec::CLOSE_SCREEN => screen_byte(rules::WORLD_SCREEN)
            .map(Kind::Close)
            .ok_or(CommandError::Unavailable),
        Spec::CHOOSE_TAB => MenuTab::ALL
            .into_iter()
            .find(|tab| tab.name() == target)
            .map(Kind::Tab)
            .ok_or(CommandError::BadArgs),
        _ if !spec.in_bounds() => Err(CommandError::BadArgs),
        Spec::LOOK => Ok(Kind::Look(spec.look)),
        Spec::MOVE => Ok(Kind::Move(spec.direction, spec.hold_ms)),
        Spec::HIGHLIGHT_RECIPE => Ok(Kind::Highlight(spec.recipe)),
        Spec::ANSWER_DIALOG => Ok(Kind::Answer(dialogs::Ask {
            answer: spec.answer,
            dialog: spec.dialog,
        })),
        Spec::JOIN => Ok(Kind::Join),
        Spec::HOST => Ok(Kind::Host(Host {
            world: spec.world,
            character: spec.character,
            online: spec.online != 0,
        })),
        Spec::LEAVE => Ok(Kind::Leave),
        // Its line is copied once, by `queue`, which parks it.
        Spec::CHAT => Ok(Kind::Chat),
        Spec::SET_WAYPOINT => on_map(spec.position).map(|at| Kind::Mark(Mark::Set(at))),
        Spec::CLEAR_WAYPOINT => Ok(Kind::Mark(Mark::Clear)),
        Spec::OPEN_MODS => Ok(Kind::Mods(ModsAsk {
            mod_index: None,
            gear: false,
        })),
        Spec::OPEN_MOD | Spec::OPEN_MOD_SETTINGS => loader
            .mods
            .iter()
            .position(|m| m.id == target)
            .map(|index| {
                Kind::Mods(ModsAsk {
                    mod_index: Some(index),
                    gear: spec.kind == Spec::OPEN_MOD_SETTINGS,
                })
            })
            .ok_or(CommandError::BadArgs),
        Spec::PING => on_map(spec.position).map(|at| Kind::Mark(Mark::Ping(at))),
        // Its ids are read once, by `window_ask`, when it is queued.
        Spec::WINDOW_PRESS | Spec::WINDOW_PICK | Spec::WINDOW_TEXT => {
            Ok(Kind::Window(window_act(spec.kind)))
        }
        Spec::PRESS => press_bit(loader, target).map(|bit| {
            let interacts = INTERACTS.iter().any(|action| action.name() == target);
            Kind::Press(bit, spec.hold_ms, interacts)
        }),
        Spec::CRAFT => Ok(Kind::Act(Act::Craft {
            station: spec.entity.get(),
            recipe: spec.recipe.0,
            count: amount(spec.amount)?,
        })),
        Spec::MOVE_ITEM => Ok(Kind::Act(Act::Move {
            from: spec.slot,
            to: spec.target_slot,
            count: amount(spec.amount)?,
        })),
        Spec::TAKE_ALL => Ok(Kind::Act(Act::MoveAll {
            from: spec.entity.get(),
            to: 0,
        })),
        Spec::DEPOSIT_ALL => Ok(Kind::Act(Act::MoveAll {
            from: 0,
            to: spec.entity.get(),
        })),
        Spec::EQUIP => equipment_slot(loader, target).map(|equipment| {
            Kind::Act(Act::Equip {
                slot: spec.slot,
                equipment,
            })
        }),
        Spec::TRASH => Ok(Kind::Act(Act::Trash { slot: spec.slot })),
        _ => Err(CommandError::BadArgs),
    }
}

/// A command's amount as the requests' `u16`, which the bounds checked.
fn amount(amount: u32) -> Result<u16, CommandError> {
    u16::try_from(amount).map_err(|_| CommandError::BadArgs)
}

/// The `EquipmentSlot` member `name` on `loader`'s build, as the equipment
/// request's byte.
///
/// # Errors
///
/// [`CommandError::BadArgs`] for a name the build's enum lacks.
fn equipment_slot(loader: &Loader, name: &str) -> Result<u8, CommandError> {
    let ty = catalogue::type_name(catalogue::ids::EQUIPMENT_SLOT);
    loader
        .reflection
        .enum_value(ty, name)
        .ok()
        .and_then(|value| u8::try_from(value).ok())
        .ok_or(CommandError::BadArgs)
}

/// The window command `spec` gives on window `window`, its ids read.
///
/// # Errors
///
/// [`CommandError::BadArgs`] for ids no window can hold, and for a pick or
/// a text without one.
pub(super) fn window_ask(spec: &Spec, window: &str) -> Result<windows::Ask, CommandError> {
    let act = window_act(spec.kind);
    // SAFETY: the mod's spec, its item borrowed for the call; the ask
    // copies it.
    let item = unsafe { text(spec.item) }?;
    // SAFETY: the mod's spec, its text borrowed for the call; the ask
    // copies it.
    let text = unsafe { text(spec.text) }?;
    if act == windows::Act::Pick && text.is_empty() {
        return Err(CommandError::BadArgs);
    }
    windows::Ask::new(window, item, act, text).ok_or(CommandError::BadArgs)
}

/// What the window command of spec kind `kind` does to its item.
const fn window_act(kind: u32) -> windows::Act {
    match kind {
        Spec::WINDOW_PICK => windows::Act::Pick,
        Spec::WINDOW_TEXT => windows::Act::Text,
        _ => windows::Act::Press,
    }
}

/// The forms of the interact key, whose press opens the screen of what the
/// player faces, as a storage chest's (client 26430625).
pub(super) const INTERACTS: [GameAction; 3] = [
    GameAction::ContextualAction,
    GameAction::ContextualActionTap,
    GameAction::ContextualActionHold,
];

/// The digital input's bit of the game action `name` on `loader`'s build:
/// its `PlayerInputType` member's value.
///
/// # Errors
///
/// [`CommandError::BadArgs`] for a name no [`GameAction`] has,
/// [`CommandError::Unavailable`] for one the build lacks or places past the
/// mask.
pub(super) fn press_bit(loader: &Loader, name: &str) -> Result<u8, CommandError> {
    if !GameAction::ALL.iter().any(|action| action.name() == name) {
        return Err(CommandError::BadArgs);
    }
    let input_type = catalogue::type_name(catalogue::ids::PLAYER_INPUT_TYPE);
    loader
        .reflection
        .enum_value(input_type, name)
        .ok()
        .and_then(|bit| u8::try_from(bit).ok())
        .filter(|bit| u32::from(*bit) < u64::BITS)
        .ok_or(CommandError::Unavailable)
}

/// The screen byte `name` names, `None` for empty: a look or a move runs
/// only on the world's own screen.
pub(super) fn expected_screen(kind: Kind, name: &str) -> Result<Option<u8>, CommandError> {
    if kind.drives() {
        if !name.is_empty() && name != rules::WORLD_SCREEN {
            return Err(CommandError::BadArgs);
        }
        return screen_byte(rules::WORLD_SCREEN)
            .map(Some)
            .ok_or(CommandError::Unavailable);
    }
    if matches!(kind, Kind::Join | Kind::Host(_)) {
        // A join or a host starts at the main menu, which no HUD screen
        // names.
        return if name.is_empty() {
            Ok(None)
        } else {
            Err(CommandError::BadArgs)
        };
    }
    if name.is_empty() {
        return Ok(None);
    }
    screen_byte(name).map(Some).ok_or(CommandError::BadArgs)
}
