//! The fake's action entries: chat filters, controls, commands, and the
//! filters on interactions, crafting and building.

use core::ffi::c_void;

use crate::test::table::{Registered, Registration, put, text, with};
use enshwick_core::abi::{
    ActionTarget, BuildCheckFn, ChatFilterFn, CraftCheckFn, InteractionFn, ModHandle, Status, Str,
};
#[cfg(not(feature = "unstable"))]
use enshwick_core::abi::{ControlFn, ControlSpec};

pub(crate) mod commands;

#[cfg(feature = "unstable")]
pub(crate) use crate::test::actions::controls::{control, run_action, run_action_at};

pub(crate) extern "system" fn actions(
    me: ModHandle,
    callback: enshwick_core::abi::ActionFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
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
    callback: ChatFilterFn,
    user: *mut c_void,
) -> Status {
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
    callback: InteractionFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.interaction_filters
            .push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn craft_filter(
    me: ModHandle,
    callback: CraftCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.craft_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn build_filter(
    me: ModHandle,
    callback: BuildCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.build_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}

/// Without the `unstable` feature the fake runs no actions.
#[cfg(not(feature = "unstable"))]
pub(crate) extern "system" fn run_action_at(
    _me: ModHandle,
    _action: Str,
    _args: Str,
    target: *const ActionTarget,
) -> Status {
    // SAFETY: the safe layer passes null or a live `ActionTarget` for the call.
    let _read = unsafe { enshwick_core::abi::read_headed(target, size_of::<ActionTarget>()) };
    Status::MISSING
}

/// Without the `unstable` feature the fake has no controls.
#[cfg(not(feature = "unstable"))]
pub(crate) extern "system" fn control(
    _me: ModHandle,
    spec: *const ControlSpec,
    _callback: ControlFn,
    _user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes a live `ControlSpec` for the call.
    match unsafe { enshwick_core::abi::read_headed(spec, size_of::<ControlSpec>()) } {
        Some(_) => Status::MISSING,
        None => Status::BAD_ARGS,
    }
}

/// Without the `unstable` feature the fake runs no actions.
#[cfg(not(feature = "unstable"))]
pub(crate) extern "system" fn run_action(_me: ModHandle, _action: Str, _args: Str) -> Status {
    Status::MISSING
}
