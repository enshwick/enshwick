//! Host functions for the actions domain: players' actions and controls, the
//! console, chat filters, and the interaction, crafting and build filters.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::{ChatFilterFn, Level, Status, Str};

use crate::engine::conflicts::Mode;
use crate::host::layer::guards::{in_init, on_client};
use crate::host::layer::{Caller, Headed, Mod, Out, Text, checked, entries};
use crate::host::memory::text;

pub(crate) mod commands;

thread_local! {
    /// The answer `run_console` last handed out on this thread.
    static CONSOLE_REPLY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

/// [`Status::MISSING`] when the server has no handler for the action.
fn sent(ran: bool) -> Result<(), Status> {
    if ran { Ok(()) } else { Err(Status::MISSING) }
}

entries! {
    fn chat_filter(me as Mod, callback: ChatFilterFn, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "chat filter",
            Mode::Chained,
            checked(crate::actions::chat::register(me.loader, me.index, callback, user)),
        )
    }

    fn interaction_filter(
        me as Mod,
        callback: api_frame::InteractionFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "interaction filter",
            Mode::Chained,
            checked(crate::actions::interactions::register(me.loader, me.index, callback, user)),
        )
    }

    fn craft_filter(me as Mod, callback: api_frame::CraftCheckFn, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "crafting veto",
            Mode::Chained,
            checked(crate::actions::vetoes::register_craft(me.loader, me.index, callback, user)),
        )
    }

    fn build_filter(me as Mod, callback: api_frame::BuildCheckFn, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "building veto",
            Mode::Chained,
            checked(crate::actions::vetoes::register_build(me.loader, me.index, callback, user)),
        )
    }

    fn actions(me as Mod, callback: api_frame::ActionFn, user: *mut core::ffi::c_void) -> Status {
        in_init(me.index)?;
        checked(crate::actions::player_actions::register(me.index, callback, user))?;
        crate::engine::log::write(Level::DEBUG, &me.record.id, "player actions");
        Ok(())
    }

    fn control(
        me as Mod,
        spec as Headed<api_frame::ControlSpec>,
        callback: api_frame::ControlFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        in_init(me.index)?;
        // A mod before ABI 11.11 has no action, which the copy leaves zero: empty.
        let (Some(name), Some(label), Some(link), Some(action)) = (
            text(spec.name),
            text(spec.label),
            text(spec.link),
            text(spec.action),
        ) else {
            return Err(Status::BAD_ARGS.into());
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
        checked(crate::actions::controls::register(
            me.index,
            &me.record.id,
            &read,
            callback,
            user,
        ))?;
        crate::engine::log::write(Level::DEBUG, &me.record.id, "a control");
        Ok(())
    }

    fn run_action(me as Mod, action as Text, args as Text) -> Status {
        on_client(me.loader)?;
        sent(crate::actions::player_actions::run_on_server(&me.record.id, action, args, None))?;
        Ok(())
    }

    fn run_action_at(
        me as Mod,
        action as Text,
        args as Text,
        target: *const enshwick_core::abi::ActionTarget,
    ) -> Status {
        on_client(me.loader)?;
        // SAFETY: `HostApi::run_action_at` has the mod pass `target` null or
        // a live `ActionTarget` readable for the size its header states, for
        // the call.
        let target = unsafe { enshwick_core::abi::ActionTarget::read(target) }?;
        sent(crate::actions::player_actions::run_on_server(&me.record.id, action, args, target))?;
        Ok(())
    }

    fn run_console(loader as Caller, line as Text, out as Out<Str>) -> Status {
        let reply = crate::actions::console::run(loader, line)?;
        CONSOLE_REPLY.with_borrow_mut(|kept| {
            *kept = reply;
            // The text stays in this thread's buffer until its next call.
            out.put(Str::new(kept))
        })
    }
}
