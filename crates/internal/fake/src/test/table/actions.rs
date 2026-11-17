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

/// What the installed fake's actions answer and have recorded: the
/// player actions callback, controls, commands and the vetoes.
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
