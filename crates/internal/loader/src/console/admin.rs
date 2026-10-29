//! The game's account action handler, which kicks, bans and unbans by
//! account for the server's administrators; the console calls it the same
//! way, so the server keeps its own ban list (`bannedAccounts` in
//! `enshrouded_server.json`).

use enshwick::abi::Status;

use crate::host::Loader;
use crate::pinned;

use super::STATE;
use super::accounts::{Act, Decision, Online, Route, Target, decide, event};

/// The build table ids the console names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::server::ACCOUNT_ACTION.id(),
    pinned::server::BAN_ADD.id(),
    pinned::server::BAN_COUNT.id(),
    pinned::server::BAN_ENTRY_ACCOUNT.id(),
    pinned::server::BAN_ENTRY_CHARACTER_NAME.id(),
    pinned::server::BAN_ENTRY_DATE.id(),
    pinned::server::BAN_ENTRY_DISPLAY_NAME.id(),
    pinned::server::BAN_ENTRY_SIZE.id(),
    pinned::server::BAN_LIST.id(),
    pinned::server::BAN_LIST_CHANGED.id(),
    pinned::server::BAN_NAME_LENGTH.id(),
    pinned::server::BAN_NAME_TEXT.id(),
    pinned::server::CONFIG.id(),
];

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
        .address(pinned::server::ACCOUNT_ACTION)
        .and_then(|address| usize::try_from(address).ok())
        .ok_or_else(|| format!("the table entry {}", pinned::server::ACCOUNT_ACTION.id()))?;
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

/// Asks the game for `decision`: through the account action handler, or
/// for a ban of an account not online, straight onto the ban list.
///
/// # Errors
///
/// [`Status::REFUSED`] for a kick or a ban of a hosted game's host; as
/// [`act`] and [`super::ban_list::add`] otherwise.
pub(crate) fn carry(loader: &Loader, act: Act, decision: &Decision) -> Result<(), Status> {
    if act != Act::Unban && crate::admission::host() == Some(decision.key) {
        return Err(Status::REFUSED);
    }
    match decision.route {
        Route::Handler => {
            self::act(loader, act, decision.key)?;
            if act == Act::Unban {
                super::forget_offline_ban(decision.key);
            }
            Ok(())
        }
        Route::BanList => super::ban_list::add(loader, decision.key),
    }
}

/// A mod's account action on `account`: the console's checks, then
/// [`carry`].
///
/// # Errors
///
/// [`Status::BAD_ARGS`] for account 0; [`Status::REFUSED`] for an
/// administrator online or a hosted game's host, unless unbanned, and for a
/// kick of an account not online; as [`carry`] otherwise.
pub(crate) fn account_action(loader: &Loader, act: Act, account: u64) -> Result<(), Status> {
    if account == 0 {
        return Err(Status::BAD_ARGS);
    }
    let online: Vec<Online> = crate::world::players(loader)?
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
    let decision = decide(act, &target).map_err(|_| Status::REFUSED)?;
    carry(loader, act, &decision)
}
