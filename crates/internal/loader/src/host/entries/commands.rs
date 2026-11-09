//! Host functions for mods' commands.

use enshwick::abi::{ActionReply, CommandInfo, Header, Level, ModHandle, Status, Str};

use crate::host::memory::{put_headed, text};
use crate::host::{caller, calling, entry};

thread_local! {
    /// The text `run_command` last handed out on this thread.
    static REPLY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

/// The reply to `answer`, its text kept in this thread's buffer until the
/// next call.
fn kept(answer: crate::actions::Answer) -> ActionReply {
    REPLY.with_borrow_mut(|kept| {
        *kept = answer.text;
        ActionReply {
            header: Header::of::<ActionReply>(),
            done: u8::from(answer.done),
            text: Str::new(kept),
        }
    })
}

pub(crate) extern "system" fn commands(me: ModHandle, name: Str, alias: Str) -> Status {
    entry("commands", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::host::in_init(index.get()) {
            return Status::TOO_LATE;
        }
        let (Some(name), Some(alias)) = (text(name), text(alias)) else {
            return Status::BAD_ARGS;
        };
        let status = crate::commands::register(loader, index, name, alias);
        if status.is_ok() {
            crate::log::write(
                Level::DEBUG,
                &record.id,
                &format!("commands /{name} /{alias}"),
            );
        }
        status
    })
}

pub(crate) extern "system" fn run_command(
    me: ModHandle,
    line: Str,
    label: Str,
    permissions: u32,
    out: *mut ActionReply,
) -> Status {
    entry("run_command", || {
        let (Some(line), Some(label)) = (text(line), text(label)) else {
            return Status::BAD_ARGS;
        };
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        match crate::commands::run_remote(loader, line, label, permissions) {
            // SAFETY: `HostApi::run_command` has the mod pass `out` null or
            // pointing at its `ActionReply`, its header filled and writable
            // for the size that header states, untouched by others during
            // the call.
            Ok(answer) => unsafe { put_headed(out, kept(answer)) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn command_list(
    me: ModHandle,
    out: *mut CommandInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("command_list", || {
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        let infos = crate::commands::list(loader);
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { super::mods::list(&infos, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn actions_changed(me: ModHandle) -> Status {
    entry("actions_changed", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::actions::declaring(loader).contains(&index) {
            return Status::MISSING;
        }
        crate::controls::rules_changed();
        Status::OK
    })
}
