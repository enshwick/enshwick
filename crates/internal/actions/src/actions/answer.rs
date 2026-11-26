//! Answers the loader's questions about a mod's actions: which a player may
//! run, a window, a run from the interface, or a typed line.

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::Arc;

use crate::abi::{ActionCall, ActionReply, Str, read_headed};
use crate::i18n::Language;
use crate::world::Player;

use super::registry::{Registry, Typed};
use super::request::{ActionTarget, Outcome, Refused, RefusedReason, Via};

/// What the mod answers the loader: a text, such as a window, or how a run
/// went.
enum Answered {
    Text(String),
    Run(Outcome),
}

thread_local! {
    /// The last reply, alive until the next question on this thread.
    static REPLY: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Runs a typed line for `player`, from chat, the console or a remote
/// caller: a panic in a handler turns off only the mod's typed lines, which
/// the loader is told, and every later one is refused saying so.
fn run_command(
    registry: &Registry,
    player: &Player,
    language: Language,
    typed: &Typed<'_>,
) -> Outcome {
    use core::sync::atomic::Ordering;

    if registry.commands_off.load(Ordering::Acquire) {
        let outcome = Outcome::refused(registry.say("actions.commands_off", language, &[]));
        let first = typed.line.split_whitespace().next().unwrap_or("");
        let refused = Refused {
            player,
            via: typed.via,
            action: &first.to_lowercase(),
            text: typed.line,
            values: &[],
            reason: RefusedReason::CommandsOff,
        };
        return registry.refused(&refused, outcome);
    }
    let run = || registry.run_from_console(player, language, typed);
    crate::host::caught(run).unwrap_or_else(|panicked| {
        if !registry.commands_off.swap(true, Ordering::AcqRel) {
            report_commands_off(panicked.text());
        }
        Outcome::refused("")
    })
}

/// Reports the mod's typed lines off to the loader, which logs it, names it
/// on the Mods screen and lists the mod's commands no more.
#[cold]
fn report_commands_off(panic: &str) {
    let Some(host) = crate::host() else {
        return;
    };
    let reason = format!("a handler panicked: {panic}");
    let _reported = (host.api().feature)(
        host.handle(),
        Str::new(crate::command_names::FEATURE),
        Str::new(&reason),
    );
}

/// Tells the mod's own function of a run the loader refused before asking
/// the mod, as [`ActionCall::REFUSED`] holds it: the run's question `via`,
/// the loader's `reason`, and the action or line and arguments it would
/// have held. A question or reason this build does not know is not told.
fn loader_refused(
    registry: &Registry,
    player: &Player,
    (via, reason): (u32, u32),
    action: &str,
    args: &str,
) {
    let via = match via {
        ActionCall::RUN => Via::Interface,
        ActionCall::CONSOLE => Via::Console,
        ActionCall::CHAT => Via::Chat,
        ActionCall::REMOTE => Via::Remote,
        _ => return,
    };
    let reason = match reason {
        ActionCall::REFUSED_RATE => RefusedReason::Rate,
        ActionCall::REFUSED_BOUNDS => RefusedReason::Bounds,
        ActionCall::REFUSED_BUSY => RefusedReason::Busy,
        ActionCall::REFUSED_REACH => RefusedReason::OutOfReach,
        _ => return,
    };
    let (action, text) = if via == Via::Interface {
        (action.to_owned(), args)
    } else {
        let first = action.split_whitespace().next().unwrap_or("");
        (first.to_lowercase(), action)
    };
    let refused = Refused {
        player,
        via,
        action: &action,
        text,
        values: &[],
        reason,
    };
    let _told = registry.refused(&refused, Outcome::refused(""));
}

/// Answers one of the loader's questions about this mod's actions.
///
/// # Safety
///
/// - `call` is null or points at an [`ActionCall`] readable for the size its
///   header states, with no other writer during the call.
/// - `call`'s strings are null or point at their `len` bytes, alive and
///   unchanged for the call.
/// - `call`'s `reply` is null or writable for one [`ActionReply`], which
///   the loader reads before it asks again on this thread.
/// - `user` is the `Box<Arc<Registry>>` that
///   `Load::actions` handed over, which the loader
///   keeps for the process.
/// - `out` is null or writable for one [`Str`], which the loader reads
///   before it asks again on this thread.
pub(super) unsafe extern "system" fn answer(
    call: *const ActionCall,
    user: *mut c_void,
    out: *mut Str,
) {
    // SAFETY: the caller guarantees `call` is null or an `ActionCall`
    // readable for the size its header states with no concurrent writer,
    // which is the contract of `read_headed`.
    let Some(call) = (unsafe { read_headed(call, size_of::<ActionCall>()) }) else {
        return;
    };
    // SAFETY: the caller guarantees `user` is the boxed `Arc<Registry>` that
    // `hand_over` leaked for the process, so it is aligned, alive and only
    // read through shared references.
    let registry = unsafe { &*user.cast::<Arc<Registry>>() };
    // SAFETY: the loader passes the player of every question, valid for the call.
    let Some(info) = (unsafe { call.player.as_ref() }) else {
        return;
    };
    let player = enshwick_world::__internal::player(info);
    let language = Language(call.language);
    // SAFETY: the caller keeps the action's bytes alive and unchanged for
    // the call, and `action` is not used after it returns.
    let action = unsafe { call.action.as_str() }.unwrap_or("");
    // SAFETY: the caller keeps the arguments' bytes alive and unchanged for
    // the call, and `args` is not used after it returns.
    let args = unsafe { call.args.as_str() }.unwrap_or("");
    // SAFETY: the caller keeps the word's bytes alive and unchanged for the
    // call, and `word` is not used after it returns.
    let word = unsafe { call.word.as_str() }.unwrap_or("");
    let typed = |via| Typed {
        line: action,
        word,
        via,
    };
    let answered = crate::host::guard(
        "panic in an action",
        || Answered::Text(String::new()),
        || match call.kind {
            ActionCall::ALLOWED => Answered::Text(registry.allowed_ids(&player)),
            ActionCall::WINDOW => Answered::Text(registry.window(&player, language, action)),
            ActionCall::WINDOW_CLOSED => {
                registry.window_closed(player.key, action);
                Answered::Text(String::new())
            }
            ActionCall::REFUSED => {
                loader_refused(registry, &player, (call.via, call.reason), action, args);
                Answered::Text(String::new())
            }
            ActionCall::RUN => {
                // SAFETY: the loader passes null or the target, valid for the call.
                let target = unsafe { call.target.as_ref() }.and_then(ActionTarget::from_raw);
                Answered::Run(registry.run_from_interface(&player, action, args, language, target))
            }
            ActionCall::CONSOLE => Answered::Run(run_command(
                registry,
                &player,
                Language::FALLBACK,
                &typed(Via::Console),
            )),
            ActionCall::CHAT => {
                Answered::Run(run_command(registry, &player, language, &typed(Via::Chat)))
            }
            ActionCall::REMOTE => Answered::Run(run_command(
                registry,
                &player,
                Language::FALLBACK,
                &typed(Via::Remote),
            )),
            _ => Answered::Text(String::new()),
        },
    );
    let slot = call.reply;
    REPLY.with_borrow_mut(|kept| match answered {
        Answered::Run(outcome) if !slot.is_null() => {
            *kept = outcome.text;
            let reply = ActionReply {
                header: crate::abi::Header::of::<ActionReply>(),
                done: u8::from(outcome.done),
                text: Str::new(kept),
            };
            // SAFETY: `slot` is not null, as the guard checks, and the caller
            // guarantees it writable for one `ActionReply`; its text lives in
            // this thread's buffer until the next question, after the loader
            // read it.
            unsafe { slot.write(reply) };
        }
        answered if !out.is_null() => {
            *kept = match answered {
                Answered::Text(text) => text,
                // A call without a reply slot reads a run as `1` or `0` and the text.
                Answered::Run(outcome) => format!("{}{}", u8::from(outcome.done), outcome.text),
            };
            // SAFETY: `out` is not null, as the guard checks, and the caller
            // guarantees it is writable for one `Str`; the text lives in this
            // thread's buffer until the next question, after the loader read
            // it.
            unsafe { out.write(Str::new(kept)) };
        }
        _ => {}
    });
}
