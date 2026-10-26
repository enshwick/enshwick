//! The game's account action handler, which kicks, bans and unbans by
//! account for the server's administrators; the console calls it the same
//! way, so the server keeps its own ban list (`bannedAccounts` in
//! `enshrouded_server.json`).

use enshwick::abi::Status;

use crate::host::Loader;

use super::STATE;
use super::accounts::{Act, event};

/// The table id of the game's account action handler.
const HANDLER: &str = "server.account_action";

/// The build table ids the console names.
pub(crate) const TABLE_IDS: &[&str] = &[HANDLER];

/// The reflected account action the handler takes.
const EVENT: &str = "keen::ecs::AccountActionEvent";
/// Its account field, a `u64`.
const EVENT_ACCOUNT: &str = "accountId";
/// Its action field, a `u8`.
const EVENT_ACT: &str = "actionType";

/// The game's handler: the server, and one account action. It runs on the
/// game thread and checks no permission of its own (server 387 and 399,
/// clients 410 and 426).
type HandlerFn = unsafe extern "system" fn(server: *mut u8, action: *const u64);

/// The handler and the action's layout on this build.
#[derive(Debug)]
pub(super) struct Handler {
    call: HandlerFn,
    size: usize,
    account: usize,
    act: usize,
}

/// The handler on this build, or what this build lacks, logged once.
fn handler(loader: &Loader) -> Option<&'static Handler> {
    STATE
        .handler
        .get_or_init(|| {
            let found = find(loader);
            if let Err(lacks) = &found {
                crate::log!(
                    WARN,
                    "console kicks and bans off: {} lacks {lacks}",
                    loader.build.name
                );
            }
            found
        })
        .as_ref()
        .ok()
}

/// The handler and its action's layout, or what is missing.
fn find(loader: &Loader) -> Result<Handler, String> {
    let reflection = &loader.reflection;
    let offset = |field: &str| {
        reflection
            .field_offset(EVENT, field)
            .ok()
            .and_then(|at| usize::try_from(at).ok())
            .ok_or_else(|| format!("the reflected {EVENT}.{field}"))
    };
    let address = loader
        .address(HANDLER)
        .and_then(|address| usize::try_from(address).ok())
        .ok_or_else(|| format!("the table entry {HANDLER}"))?;
    let size = reflection
        .type_info(EVENT)
        .ok()
        .and_then(|info| usize::try_from(info.size()).ok())
        .ok_or_else(|| format!("the reflected {EVENT}"))?;
    let account = offset(EVENT_ACCOUNT)?;
    let act = offset(EVENT_ACT)?;
    if event(size, account, act, Act::Kick, 0).is_none() {
        return Err(format!("a reflected {EVENT} that holds its fields"));
    }
    Ok(Handler {
        // SAFETY: the table's `server.account_action` address is the game's
        // account action handler on this build: the row of the table the
        // build's fingerprint selected (servers 387 and 399, clients 410 and
        // 426), or on another build the one match of its pattern; its
        // signature is `HandlerFn`.
        call: unsafe { core::mem::transmute::<usize, HandlerFn>(address) },
        size,
        account,
        act,
    })
}

/// Asks the game to `act` on `account`, on the game thread.
///
/// # Errors
///
/// [`Status::MISSING`] when this build lacks the handler or the action's
/// layout; [`Status::WRONG_THREAD`] off the game thread.
pub(crate) fn act(loader: &Loader, act: Act, account: u64) -> Result<(), Status> {
    let handler = handler(loader).ok_or(Status::MISSING)?;
    let server = crate::world::server()?;
    let action =
        event(handler.size, handler.account, handler.act, act, account).ok_or(Status::MISSING)?;
    // SAFETY: `server` comes from `crate::world::server`, so this runs on the
    // game thread, the only one the handler runs on, with the live server the
    // game passes it; `action` holds the reflected action's size, aligned to
    // 8, zeroed but for the account and the action at their reflected
    // offsets, and outlives the call, which only reads it.
    unsafe {
        (handler.call)(
            core::ptr::with_exposed_provenance_mut(server),
            action.as_ptr(),
        );
    }
    Ok(())
}
