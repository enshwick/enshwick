//! The game's account action handler, which kicks, bans and unbans by
//! account for the server's administrators; mods call it the same way, so
//! the server keeps its own ban list (`bannedAccounts` in
//! `enshrouded_server.json`).

use enshwick_core::abi::Status;

use crate::engine::loader::Loader;
use crate::engine::pinned;

use super::accounts::{Act, Online, Target, decide, event};
use super::{FindGameCodeError, STATE};

/// The build table ids the console names.
pub(crate) const TABLE_IDS: &[&str] = &[pinned::server::ACCOUNT_ACTION.id()];

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
                    "account actions off: {} lacks {lacks}",
                    loader.build.name
                );
            }
            found
        })
        .as_ref()
        .ok()
}

/// The handler and its action's layout, or what is missing.
fn find(loader: &Loader) -> Result<Handler, FindGameCodeError> {
    let reflection = &loader.reflection;
    let offset = |field: &'static str| {
        reflection
            .field_offset(EVENT, field)
            .ok()
            .and_then(|at| usize::try_from(at).ok())
            .ok_or(FindGameCodeError::Field { ty: EVENT, field })
    };
    let address = loader
        .address(pinned::server::ACCOUNT_ACTION)
        .and_then(|address| usize::try_from(address).ok())
        .ok_or(FindGameCodeError::Entry {
            id: pinned::server::ACCOUNT_ACTION.id(),
        })?;
    let size = reflection
        .type_info(EVENT)
        .ok()
        .and_then(|info| usize::try_from(info.size()).ok())
        .ok_or(FindGameCodeError::Type { ty: EVENT })?;
    let account = offset(EVENT_ACCOUNT)?;
    let act = offset(EVENT_ACT)?;
    if event(size, account, act, Act::Kick, 0).is_none() {
        return Err(FindGameCodeError::Layout { ty: EVENT });
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
    let server = crate::engine::world::server()?;
    let action =
        event(handler.size, handler.account, handler.act, act, account).ok_or(Status::MISSING)?;
    // SAFETY: `server` comes from `crate::engine::world::server`, so this runs on the
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

/// A mod's account action on `account`, through the game's account action
/// handler.
///
/// # Errors
///
/// [`Status::BAD_ARGS`] for account 0; [`Status::REFUSED`] for an
/// administrator online or a hosted game's host, unless unbanned, and for a
/// kick or a ban of an account not online; as [`act`] otherwise.
pub(crate) fn account_action(loader: &Loader, act: Act, account: u64) -> Result<(), Status> {
    if account == 0 {
        return Err(Status::BAD_ARGS);
    }
    let online: Vec<Online> = crate::engine::world::players(loader)?
        .iter()
        .map(|p| Online {
            name: super::tools::name(p),
            key: p.key.get(),
            admin: super::tools::is_admin(p),
        })
        .collect();
    let target = online
        .iter()
        .find(|p| p.key == account)
        .map_or(Target::Account(account), |p| Target::Player(p.clone()));
    let key = decide(act, &target).map_err(|_| Status::REFUSED)?;
    if act != Act::Unban && crate::base::admission::host() == Some(key) {
        return Err(Status::REFUSED);
    }
    self::act(loader, act, key)
}
