//! Host functions for the actions domain: players' actions and controls, the
//! console, chat filters, and the interaction, crafting and build filters.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::{ChatFilterFn, Level, Status, Str};

use crate::engine::conflicts::Mode;
use crate::host::layer::guards::{in_init, on_client};
use crate::host::layer::{Callback, Caller, Headed, HeadedOut, Mod, Out, Text, entries};
use crate::host::memory::text;

pub(crate) mod commands;

thread_local! {
    /// The answer `run_console` last handed out on this thread.
    static CONSOLE_REPLY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

/// [`Status::UNAVAILABLE`] when the server has no handler for the action.
fn sent(ran: bool) -> Result<(), Status> {
    if ran {
        Ok(())
    } else {
        Err(Status::UNAVAILABLE)
    }
}

entries! {
    fn chat_filter(me as Mod, callback as Callback<ChatFilterFn>, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "chat filter",
            Mode::Chained,
            crate::actions::chat::register(me.loader, me.index, callback, user),
        )
    }

    fn interaction_filter(
        me as Mod,
        callback as Callback<api_frame::InteractionFilterFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "interaction filter",
            Mode::Chained,
            crate::actions::interactions::register(me.loader, me.index, callback, user),
        )
    }

    fn interaction_prompt(me as Mod, spec as Headed<api_frame::InteractionPrompt>) -> Status {
        me.claim(
            crate::actions::interaction_prompts::feature(spec.item),
            Mode::Exclusive,
            crate::actions::interaction_prompts::register(me.loader, me.index, &spec),
        )
    }

    fn craft_filter(me as Mod, callback as Callback<api_frame::CraftFilterFn>, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "crafting veto",
            Mode::Chained,
            crate::actions::vetoes::register_craft(me.loader, me.index, callback, user),
        )
    }

    fn build_filter(me as Mod, callback as Callback<api_frame::BuildFilterFn>, user: *mut core::ffi::c_void) -> Status {
        me.claim(
            "building veto",
            Mode::Chained,
            crate::actions::vetoes::register_build(me.loader, me.index, callback, user),
        )
    }

    fn actions(me as Mod, callback as Callback<api_frame::ActionFn>, user: *mut core::ffi::c_void) -> Status {
        in_init(me.index)?;
        crate::actions::player_actions::register(me.index, callback, user)?;
        crate::engine::log::write(Level::DEBUG, &me.record.id, "player actions");
        Ok(())
    }

    fn control(
        me as Mod,
        spec as Headed<api_frame::ControlSpec>,
        callback as Callback<api_frame::ControlFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        in_init(me.index)?;
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
        crate::actions::controls::register(me.index, &me.record.id, &read, callback, user)?;
        crate::engine::log::write(Level::DEBUG, &me.record.id, "a control");
        Ok(())
    }

    fn run_action(
        me as Mod,
        action as Text,
        args as Text,
        target: *const enshwick_core::abi::ActionTarget,
    ) -> Status {
        on_client(me.loader)?;
        // SAFETY: `HostApi::run_action` has the mod pass `target` null or
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

    fn game_binding(loader as Caller, action as Text, out as HeadedOut<api_frame::GameBinding>) -> Status {
        on_client(loader)?;
        let [first, second] = crate::actions::game_bindings::binding(action)?;
        out.put(api_frame::GameBinding {
            first,
            second,
            ..api_frame::GameBinding::EMPTY
        })
    }

    fn game_rebind(me as Mod, action as Text, key: u32) -> Status {
        on_client(me.loader)?;
        Ok(crate::actions::game_bindings::rebind(me.index, action, key)?)
    }
}
