//! Host functions for the actions domain: players' actions and controls, the
//! console, chat filters, and the interaction, crafting and build filters.

use crate::host::memory::{put, text};
use crate::host::{caller, caller_index, calling, entry};
use enshwick_core::abi as api_frame;
use enshwick_core::abi::{ChatFilterFn, Level, ModHandle, Side, Status, Str};

pub(crate) mod commands;

pub(crate) extern "system" fn chat_filter(
    me: ModHandle,
    callback: ChatFilterFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("chat_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "chat filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::actions::chat::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn interaction_filter(
    me: ModHandle,
    callback: api_frame::InteractionFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("interaction_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "interaction filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::actions::interactions::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn craft_filter(
    me: ModHandle,
    callback: api_frame::CraftCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("craft_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "crafting veto",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::actions::vetoes::register_craft(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn build_filter(
    me: ModHandle,
    callback: api_frame::BuildCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("build_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "building veto",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::actions::vetoes::register_build(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn actions(
    me: ModHandle,
    callback: api_frame::ActionFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("actions", || {
        let (_, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::host::in_init(index.get()) {
            return Status::TOO_LATE;
        }
        let status = crate::actions::player_actions::register(index, callback, user);
        if status.is_ok() {
            crate::engine::log::write(Level::DEBUG, &record.id, "player actions");
        }
        status
    })
}

pub(crate) extern "system" fn control(
    me: ModHandle,
    spec: *const api_frame::ControlSpec,
    callback: api_frame::ControlFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("control", || {
        let (_, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::host::in_init(index.get()) {
            return Status::TOO_LATE;
        }
        // SAFETY: `HostApi::control` has the mod pass `spec` null or
        // readable for the size its header gives, with no other writer
        // during the call.
        let Some(spec) = (unsafe {
            crate::host::memory::get_headed(
                spec,
                <api_frame::ControlSpec as api_frame::Headed>::BORN,
            )
        }) else {
            return Status::BAD_ARGS;
        };
        // A mod before ABI 11.11 has no action, which the copy leaves zero: empty.
        let (Some(name), Some(label), Some(link), Some(action)) = (
            text(spec.name),
            text(spec.label),
            text(spec.link),
            text(spec.action),
        ) else {
            return Status::BAD_ARGS;
        };
        let read = crate::actions::controls::Spec {
            name,
            label,
            link,
            action,
            keys: [spec.key, spec.second],
            fires: spec.fires,
            hold_ms: spec.hold_ms,
        };
        let status = crate::actions::controls::register(index, &record.id, &read, callback, user);
        if status.is_ok() {
            crate::engine::log::write(Level::DEBUG, &record.id, "a control");
        }
        status
    })
}

pub(crate) extern "system" fn run_action(me: ModHandle, action: Str, args: Str) -> Status {
    entry("run_action", || {
        let (loader, _, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.side != Side::CLIENT {
            return Status::MISSING;
        }
        let (Some(action), Some(args)) = (text(action), text(args)) else {
            return Status::BAD_ARGS;
        };
        if crate::actions::player_actions::run_on_server(&record.id, action, args, None) {
            Status::OK
        } else {
            Status::MISSING
        }
    })
}

pub(crate) extern "system" fn run_action_at(
    me: ModHandle,
    action: Str,
    args: Str,
    target: *const enshwick_core::abi::ActionTarget,
) -> Status {
    entry("run_action_at", || {
        let (loader, _, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.side != Side::CLIENT {
            return Status::MISSING;
        }
        let (Some(action), Some(args)) = (text(action), text(args)) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: `HostApi::run_action_at` has the mod pass `target` null or
        // a live `ActionTarget` readable for the size its header states, for
        // the call.
        let target = match unsafe { enshwick_core::abi::ActionTarget::read(target) } {
            Ok(target) => target,
            Err(status) => return status,
        };
        if crate::actions::player_actions::run_on_server(&record.id, action, args, target) {
            Status::OK
        } else {
            Status::MISSING
        }
    })
}

thread_local! {
    /// The answer `run_console` last handed out on this thread.
    static CONSOLE_REPLY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

pub(crate) extern "system" fn run_console(me: ModHandle, line: Str, out: *mut Str) -> Status {
    entry("run_console", || {
        let Some(line) = text(line) else {
            return Status::BAD_ARGS;
        };
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        match crate::actions::console::run(loader, line) {
            Ok(reply) => CONSOLE_REPLY.with_borrow_mut(|kept| {
                *kept = reply;
                // The text stays in this thread's buffer until its next call.
                // SAFETY: `HostApi::run_console` has the mod pass `out` null or
                // pointing at a writable, aligned `Str` that no other thread
                // touches during the call.
                unsafe { put(out, Str::new(kept)) }
            }),
            Err(status) => status,
        }
    })
}
