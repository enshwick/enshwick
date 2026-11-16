//! The fake's action entries: chat filters, controls, commands, and the
//! filters on interactions, crafting and building.

use core::ffi::c_void;

use crate::test::table::{Registered, Registration, put, text, with};
use enshwick_core::abi::{
    ActionTarget, BuildFilterFn, ChatFilterFn, CraftFilterFn, InteractionFilterFn,
    InteractionPrompt, ModHandle, Status, Str,
};
#[cfg(not(feature = "unstable"))]
use enshwick_core::abi::{ControlFn, ControlSpec};

pub(crate) mod commands;

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
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.actions.is_some() {
            return Status::REFUSED;
        }
        s.actions = Some(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn run_console(_me: ModHandle, line: Str, out: *mut Str) -> Status {
    let line = text(line);
    let (word, rest) = line.split_once(' ').unwrap_or((&line, ""));
    let (word, rest) = (word.to_owned(), rest.trim().to_owned());
    with((), |s| s.console.push(line.clone()));
    let answer = match with(None, |s| s.host.console_answers.get(&word).cloned()) {
        Some(answer) => answer,
        // The mod's own actions, asked with the table unlocked: they may call
        // back.
        None => match with(None, |s| s.actions) {
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
        let kept = s.run_out.insert(answer.into_boxed_str());
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
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.chat_filters.push(Registered::new(callback, user, me));
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
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.interaction_filters
            .push(Registered::new(callback, user, me));
        Status::OK
    })
}

/// As the loader: a malformed spec is a bad argument, a second prompt on an
/// item is refused whichever mod asks.
pub(crate) extern "system" fn interaction_prompt(
    _me: ModHandle,
    spec: *const InteractionPrompt,
) -> Status {
    // SAFETY: the safe layer passes null or a live `InteractionPrompt` for
    // the call, readable for the size its header states.
    let Some(spec) =
        (unsafe { enshwick_core::abi::read_headed(spec, size_of::<InteractionPrompt>()) })
            .filter(InteractionPrompt::is_valid)
    else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.interaction_prompts.iter().any(|p| p.item == spec.item) {
            return Status::REFUSED;
        }
        s.interaction_prompts.push(spec);
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
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.craft_filters.push(Registered::new(callback, user, me));
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
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.build_filters.push(Registered::new(callback, user, me));
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
