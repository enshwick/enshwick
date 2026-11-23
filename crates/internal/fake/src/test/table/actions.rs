//! The fake's action entries: chat filters, controls, commands, and the
//! filters on interactions, crafting and building.

use core::ffi::c_void;

use crate::test::table::{Registered, put, text, with, with_open};
use enshwick_core::abi::{
    ActionTarget, BuildFilterFn, ChatFilterFn, CraftFilterFn, InteractionFilterFn,
    InteractionPrompt, ModHandle, Status, Str,
};
#[cfg(not(feature = "unstable"))]
use enshwick_core::abi::{ControlFn, ControlSpec};

pub(crate) mod commands;

/// What the installed fake's actions answer and have recorded.
#[derive(Default)]
pub(crate) struct ActionsState {
    /// The mod's player actions callback, once registered.
    pub(crate) callback: Option<Registered<enshwick_core::abi::ActionFn>>,
    /// The lines the mod ran on the console, in order.
    pub(crate) console: Vec<String>,
    pub(crate) craft_filters: Vec<Registered<CraftFilterFn>>,
    pub(crate) build_filters: Vec<Registered<BuildFilterFn>>,
    /// The mod's controls and the action runs it sent.
    #[cfg(feature = "unstable")]
    pub(crate) controls: crate::test::actions::controls::FakeControls,
    pub(crate) interaction_filters: Vec<Registered<InteractionFilterFn>>,
    /// The prompts mods gave the objects placed from their items.
    pub(crate) interaction_prompts: Vec<InteractionPrompt>,
    pub(crate) chat_filters: Vec<Registered<ChatFilterFn>>,
    /// Each account's chat rate: the start of its window in fake server
    /// time and the lines heard in it.
    pub(crate) chat_rate: Vec<(enshwick_core::PlayerKey, u64, u32)>,
    /// The command name and alias the mod took, and how often it said
    /// its rules changed.
    pub(crate) commands: commands::FakeCommands,
    /// The answer `run_console` last handed out, alive until the next call.
    run_out: Option<Box<str>>,
    /// The client's keyboard and mouse bindings of the game's controls.
    pub(crate) bindings: GameBindings,
}

#[cfg(feature = "unstable")]
pub(crate) use crate::test::actions::controls::{control, run_action};

pub(crate) extern "system" fn actions(
    me: ModHandle,
    callback: Option<enshwick_core::abi::ActionFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with_open(|s| {
        if s.actions.callback.is_some() {
            return Status::REFUSED;
        }
        s.actions.callback = Some(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn run_console(_me: ModHandle, line: Str, out: *mut Str) -> Status {
    let line = text(line);
    let (word, rest) = line.split_once(' ').unwrap_or((&line, ""));
    let (word, rest) = (word.to_owned(), rest.trim().to_owned());
    with((), |s| s.actions.console.push(line.clone()));
    let answer = match with(None, |s| s.host.actions.console_answers.get(&word).cloned()) {
        Some(answer) => answer,
        // The mod's own actions, asked with the table unlocked: they may call
        // back.
        None => match with(None, |s| s.actions.callback) {
            Some(actions) => {
                let reply = crate::test::ask(
                    actions,
                    enshwick_core::abi::ActionCall::CONSOLE,
                    &crate::test::console(),
                    0,
                    &rest,
                    &word.to_lowercase(),
                    ActionTarget::NONE,
                );
                reply.get(1..).unwrap_or_default().to_owned()
            }
            None => return Status::MISSING,
        },
    };
    with(Status::MISSING, |s| {
        let kept = s.actions.run_out.insert(answer.into_boxed_str());
        put(out, Str::new(kept))
    })
}

pub(crate) extern "system" fn chat_filter(
    me: ModHandle,
    callback: Option<ChatFilterFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with_open(|s| {
        s.actions
            .chat_filters
            .push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn interaction_filter(
    me: ModHandle,
    callback: Option<InteractionFilterFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with_open(|s| {
        s.actions
            .interaction_filters
            .push(Registered::new(callback, user, me));
        Status::OK
    })
}

/// As the loader: an unreadable spec is a bad argument, then a call after
/// init is too late, then a malformed spec is a bad argument, and a second
/// prompt on an item is refused whichever mod asks.
pub(crate) extern "system" fn interaction_prompt(
    _me: ModHandle,
    spec: *const InteractionPrompt,
) -> Status {
    // SAFETY: the safe layer passes null or a live `InteractionPrompt` for
    // the call, readable for the size its header states.
    let Some(spec) =
        (unsafe { enshwick_core::abi::read_headed(spec, size_of::<InteractionPrompt>()) })
    else {
        return Status::BAD_ARGS;
    };
    with_open(|s| {
        if !spec.is_valid() {
            return Status::BAD_ARGS;
        }
        if s.actions
            .interaction_prompts
            .iter()
            .any(|p| p.item == spec.item)
        {
            return Status::REFUSED;
        }
        s.actions.interaction_prompts.push(spec);
        Status::OK
    })
}

pub(crate) extern "system" fn craft_filter(
    me: ModHandle,
    callback: Option<CraftFilterFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with_open(|s| {
        s.actions
            .craft_filters
            .push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn build_filter(
    me: ModHandle,
    callback: Option<BuildFilterFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with_open(|s| {
        s.actions
            .build_filters
            .push(Registered::new(callback, user, me));
        Status::OK
    })
}

/// Without the `unstable` feature the fake runs no actions.
#[cfg(not(feature = "unstable"))]
pub(crate) extern "system" fn run_action(
    _me: ModHandle,
    _action: Str,
    _args: Str,
    target: *const ActionTarget,
) -> Status {
    // SAFETY: the safe layer passes null or a live `ActionTarget` for the call.
    let _read = unsafe { enshwick_core::abi::read_headed(target, size_of::<ActionTarget>()) };
    Status::UNAVAILABLE
}

/// Without the `unstable` feature the fake has no controls.
#[cfg(not(feature = "unstable"))]
pub(crate) extern "system" fn control(
    _me: ModHandle,
    spec: *const ControlSpec,
    callback: Option<ControlFn>,
    _user: *mut c_void,
) -> Status {
    if callback.is_none() {
        return Status::BAD_ARGS;
    }
    // SAFETY: the safe layer passes a live `ControlSpec` for the call.
    match unsafe { enshwick_core::abi::read_headed(spec, size_of::<ControlSpec>()) } {
        Some(_) => Status::UNAVAILABLE,
        None => Status::BAD_ARGS,
    }
}

/// The client's keyboard and mouse bindings of the game's own controls on
/// the fake build, each key in the ABI's form.
#[derive(Debug, Default)]
pub(crate) struct GameBindings {
    /// Each game action the build names, by its reflected name, with the
    /// two keys the player bound to it.
    pub(crate) player: Vec<(String, [u32; 2])>,
    /// The mods' rebinds, each action's first key; the last one wins.
    pub(crate) rebinds: Vec<(String, u32)>,
    /// What the interface read at its last step, rebinds written in;
    /// `None` before a step ran, as before the loader's first frame.
    read: Option<Vec<(String, [u32; 2])>>,
}

impl GameBindings {
    /// The keys `action` holds with the rebinds written in; `None` for an
    /// action the build does not name.
    pub(crate) fn keys(&self, action: &str) -> Option<[u32; 2]> {
        let &(_, [first, second]) = self.player.iter().find(|(name, _)| name == action)?;
        let first = self
            .rebinds
            .iter()
            .find(|(name, _)| name == action)
            .map_or(first, |&(_, key)| key);
        Some([first, second])
    }

    /// An interface step: the loader writes the rebinds into the game's
    /// bindings and reads them back.
    pub(crate) fn step(&mut self) {
        let read = self
            .player
            .iter()
            .filter_map(|(name, _)| Some((name.clone(), self.keys(name)?)))
            .collect();
        self.read = Some(read);
    }
}

/// The keys of the game action `action` as the interface last read them;
/// as the loader: unavailable on a server and before the interface's first
/// step, missing for an action the build does not name.
pub(crate) extern "system" fn game_binding(
    _me: ModHandle,
    action: Str,
    out: *mut enshwick_core::abi::GameBinding,
) -> Status {
    use enshwick_core::abi::GameBinding;
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    let action = text(action);
    with(Status::MISSING, |s| {
        if s.host.base.side == enshwick_core::abi::Side::SERVER {
            return Status::UNAVAILABLE;
        }
        let bindings = &s.actions.bindings;
        if bindings.keys(&action).is_none() {
            return Status::MISSING;
        }
        let Some(read) = &bindings.read else {
            return Status::UNAVAILABLE;
        };
        let [first, second] = read
            .iter()
            .find(|(name, _)| *name == action)
            .map_or([0, 0], |&(_, keys)| keys);
        super::put_headed(
            out,
            GameBinding {
                first,
                second,
                ..GameBinding::EMPTY
            },
        )
    })
}

/// Rebinds the first key of the game action `action`, or with 0 gives it
/// back, as the loader: unavailable on a server, missing for an action the
/// build does not name, a bad argument for a key a mod control may not
/// take. The interface's next step writes it.
pub(crate) extern "system" fn game_rebind(_me: ModHandle, action: Str, key: u32) -> Status {
    let action = text(action);
    with(Status::MISSING, |s| {
        if s.host.base.side == enshwick_core::abi::Side::SERVER {
            return Status::UNAVAILABLE;
        }
        let bindings = &mut s.actions.bindings;
        if bindings.keys(&action).is_none() {
            return Status::MISSING;
        }
        if key != 0 && !bindable(key) {
            return Status::BAD_ARGS;
        }
        bindings.rebinds.retain(|(name, _)| *name != action);
        if key != 0 {
            bindings.rebinds.push((action, key));
        }
        Status::OK
    })
}

/// Whether `key` is one a mod control may take, as the loader checks it.
#[cfg(feature = "unstable")]
fn bindable(key: u32) -> bool {
    enshwick_core::controls::Key::from_bits(key).is_some()
}

/// Without `unstable` no mod reaches a rebind, so every key passes.
#[cfg(not(feature = "unstable"))]
const fn bindable(_key: u32) -> bool {
    true
}
