//! Host functions for mods' commands and action windows.

use crate::host::memory::{put_headed, text};
use crate::host::{caller, calling, entry};
use enshwick_core::abi::{ActionReply, CommandInfo, Header, Level, ModHandle, Status, Str};

thread_local! {
    /// The text `run_command` last handed out on this thread.
    static REPLY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

/// The reply to `answer`, its text kept in this thread's buffer until the
/// next call.
fn kept(answer: crate::actions::player_actions::Answer) -> ActionReply {
    REPLY.with_borrow_mut(|kept| {
        *kept = answer.text;
        ActionReply {
            header: Header::of::<ActionReply>(),
            done: u8::from(answer.done),
            text: Str::new(kept),
        }
    })
}

pub(crate) extern "system" fn commands(me: ModHandle, _name: Str, alias: Str) -> Status {
    entry("commands", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::host::in_init(index.get()) {
            return Status::TOO_LATE;
        }
        let Some(alias) = text(alias) else {
            return Status::BAD_ARGS;
        };
        let status = crate::actions::commands::register(loader, index, alias);
        if status.is_ok() {
            crate::engine::log::write(
                Level::DEBUG,
                &record.id,
                &format!("commands, alias /{alias}"),
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
        match crate::actions::commands::run_remote(loader, line, label, permissions) {
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
        let infos = crate::actions::commands::list(loader);
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { crate::host::memory::list(&infos, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn actions_changed(me: ModHandle) -> Status {
    entry("actions_changed", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::actions::player_actions::declaring(loader).contains(&index) {
            return Status::MISSING;
        }
        crate::actions::controls::rules_changed();
        Status::OK
    })
}

pub(crate) extern "system" fn action_window(
    me: ModHandle,
    id: Str,
    players: *const enshwick_core::PlayerKey,
    count: u32,
    close: u32,
) -> Status {
    entry("action_window", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let Some(window) = text(id) else {
            return Status::BAD_ARGS;
        };
        let count = usize::try_from(count).unwrap_or(usize::MAX);
        if count > crate::ui::windows::MAX_PLAYERS || (players.is_null() && count != 0) {
            return Status::BAD_ARGS;
        }
        let keys = if count == 0 {
            &[]
        } else {
            // SAFETY: `HostApi::action_window` has the mod pass `count` keys
            // at `players`, not null by the guard before it, alive and
            // unchanged for the call; `count` is at most `MAX_PLAYERS`.
            unsafe { core::slice::from_raw_parts(players, count) }
        };
        crate::ui::windows::show(loader, index, window, keys, close)
    })
}
