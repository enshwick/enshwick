//! Host functions for mods' commands and actions.

use enshwick_core::abi::{ActionReply, CommandInfo, Level, Status, Str};

use crate::host::layer::guards::in_init;
use crate::host::layer::{Caller, HeadedOut, Mod, SizedListOut, Text, entries};

thread_local! {
    /// The text `run_command` last handed out on this thread.
    static REPLY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

/// The reply to `answer`, its text kept in this thread's buffer until the
/// next call.
fn kept(answer: crate::actions::player_actions::Answer) -> ActionReply {
    REPLY.with_borrow_mut(|kept| {
        *kept = answer.text;
        enshwick_abi::headed!(ActionReply {
            done: u8::from(answer.done),
            text: Str::new(kept),
        })
    })
}

entries! {
    fn commands(me as Mod, alias as Text) -> Status {
        in_init(me.index)?;
        crate::actions::commands::register(me.loader, me.index, alias)?;
        crate::engine::log::write(Level::DEBUG, &me.record.id, &format!("commands, alias /{alias}"));
        Ok(())
    }

    fn run_command(
        loader as Caller,
        line as Text,
        label as Text,
        permissions: u32,
        out as HeadedOut<ActionReply>,
    ) -> Status {
        let answer = crate::actions::commands::run_remote(loader, line, label, permissions)?;
        out.put(kept(answer))
    }

    fn command_list(loader as Caller, [out, entry_size, capacity, count] as SizedListOut<CommandInfo>) -> Status {
        out.put(&crate::actions::commands::list(loader))
    }

    fn actions_changed(me as Mod) -> Status {
        if !crate::actions::player_actions::declaring(me.loader).contains(&me.index) {
            return Err(Status::MISSING.into());
        }
        crate::actions::controls::rules_changed(me.index);
        Ok(())
    }
}
