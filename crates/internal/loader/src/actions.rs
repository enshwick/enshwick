//! Player actions across the wire: the server runs what a mod's control or
//! panel sent, or what the console named, through the mod that declared the
//! action.
//!
//! The server decides everything. A client sends an action's id and
//! arguments; the server runs the mod's own checks again on every run, and
//! the mod's handler answers. The reply reaches the player as one of the
//! game's notices, in the language the client named when it joined.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use enshwick::abi::{ActionCall, ActionFn, Header, PlayerInfo, Status, Str};

use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::remote::Message;

/// The least time between two runs one player sends.
const RUN_GAP: Duration = Duration::from_millis(200);

/// A mod's actions callback.
#[derive(Clone, Copy)]
struct Registered {
    owner: ModIndex,
    callback: ActionFn,
    user: usize,
}

static REGISTERED: Mutex<Vec<Registered>> = Mutex::new(Vec::new());

/// The server's state per client: the language it reads and when it last
/// ran an action.
#[derive(Default)]
struct Client {
    language: u32,
    last_run: Option<Instant>,
}

struct ServerState {
    clients: rustc_hash::FxHashMap<u64, Client>,
}

static SERVER: Mutex<Option<ServerState>> = Mutex::new(None);

fn server_state<R>(f: impl FnOnce(&mut ServerState) -> R) -> R {
    let mut slot = SERVER.relock();
    f(slot.get_or_insert_with(|| ServerState {
        clients: rustc_hash::FxHashMap::default(),
    }))
}

/// Registers mod `owner`'s actions callback; a second one is refused.
pub(crate) fn register(
    owner: ModIndex,
    callback: ActionFn,
    user: *mut core::ffi::c_void,
) -> Status {
    let mut registered = REGISTERED.relock();
    if registered.iter().any(|r| r.owner == owner) {
        return Status::REFUSED;
    }
    registered.push(Registered {
        owner,
        callback,
        user: user.expose_provenance(),
    });
    Status::OK
}

/// The registered mods that are running.
fn running(loader: &Loader) -> Vec<Registered> {
    let registered = REGISTERED.relock();
    registered
        .iter()
        .copied()
        .filter(|r| {
            loader
                .mods
                .get(r.owner.get())
                .is_some_and(|m| !m.off.load(core::sync::atomic::Ordering::Acquire))
        })
        .collect()
}

/// Asks `registered` its question `kind` about `player`; the reply.
fn ask(
    registered: Registered,
    kind: u32,
    player: &PlayerInfo,
    language: u32,
    action: &str,
    args: &str,
) -> String {
    let call = ActionCall {
        header: Header::of::<ActionCall>(),
        kind,
        language,
        player: *player,
        action: Str::new(action),
        args: Str::new(args),
    };
    let mut reply = Str::EMPTY;
    // SAFETY: the callback lives in the mod's DLL, which stays loaded; the
    // safe layer catches its panics.
    crate::profile::call(
        registered.owner,
        crate::profile::Feature::Actions,
        || unsafe {
            (registered.callback)(
                &raw const call,
                core::ptr::with_exposed_provenance_mut(registered.user),
                &raw mut reply,
            );
        },
    );
    // SAFETY: the reply stays valid until the callback runs again.
    unsafe { reply.as_str() }.unwrap_or("").to_owned()
}

/// Runs the console line `rest` on the actions of mod `index`; the answer,
/// or `None` when the mod is off or declared no actions.
pub(crate) fn console(loader: &Loader, index: usize, rest: &str) -> Option<String> {
    let registered = running(loader)
        .into_iter()
        .find(|r| r.owner == ModIndex::new(index))?;
    let console = crate::console::the_console();
    let reply = ask(registered, ActionCall::CONSOLE, &console, 0, rest, "");
    Some(reply.get(1..).unwrap_or_default().to_owned())
}

/// The running mods that declared actions, by index.
pub(crate) fn declaring(loader: &Loader) -> Vec<ModIndex> {
    running(loader).into_iter().map(|r| r.owner).collect()
}

/// The connected player with account key `key`.
fn player(loader: &Loader, key: u64) -> Option<PlayerInfo> {
    crate::world::players(loader)
        .ok()?
        .into_iter()
        .find(|p| p.key == enshwick::PlayerKey::new(key))
}

/// A client names the language its player reads replies in, on the
/// server's game thread; a run takes its hello's language before this one.
pub(crate) fn language(key: u64, language: u32) {
    server_state(|s| s.clients.entry(key).or_default().language = language);
}

/// A client runs mod `id`'s action, on the server's game thread.
pub(crate) fn run(loader: &Loader, key: u64, id: &str, action: &str, args: &str) {
    let now = Instant::now();
    let (language, admitted) = server_state(|s| {
        let client = s.clients.entry(key).or_default();
        let admitted = client
            .last_run
            .is_none_or(|last| now.duration_since(last) >= RUN_GAP);
        if admitted {
            client.last_run = Some(now);
        }
        (client.language, admitted)
    });
    if !admitted {
        return;
    }
    let language = crate::net::language_of(key).map_or(language, |l| l.0);
    let Some(player) = player(loader, key) else {
        return;
    };
    let Some(registered) = running(loader)
        .into_iter()
        .find(|r| loader.mods.get(r.owner.get()).is_some_and(|m| m.id == id))
    else {
        return;
    };
    let reply = ask(registered, ActionCall::RUN, &player, language, action, args);
    let (ok, text) = match reply.split_at_checked(1) {
        Some(("1", text)) => (true, text),
        Some((_, text)) => (false, text),
        None => (false, ""),
    };
    let name =
        String::from_utf8_lossy(&player.name[..usize::from(player.name_len).min(64)]).into_owned();
    crate::log!(
        DEBUG,
        "{name} ({key:016x}) ran {id}:{action}: {}",
        if ok { "done" } else { "refused" }
    );
    if !text.is_empty() {
        let shown = one_line(text);
        if crate::notices::send(loader, key, &shown).is_err() {
            let _queued = crate::chat::send(key, &shown);
        }
    }
}

/// A reply's lines joined into the one line a notice shows.
fn one_line(text: &str) -> String {
    text.lines().collect::<Vec<_>>().join(" ")
}

/// Forgets the clients, as when the server's world goes away.
pub(crate) fn forget() {
    *SERVER.relock() = None;
}

/// Runs mod `id`'s action on the server; false while it takes no messages.
pub(crate) fn run_on_server(id: &str, action: &str, args: &str) -> bool {
    crate::net::to_server(&Message::ActionRun {
        id: id.to_owned(),
        action: action.to_owned(),
        args: args.to_owned(),
    })
}

/// The client joined a server that answered: it tells the server its
/// language.
pub(crate) fn joined() {
    let _sent = crate::net::to_server(&Message::ActionsLanguage {
        language: crate::content::language().0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_of_several_lines_shows_every_line_in_its_notice() {
        assert_eq!(one_line("3 players\nAnn\r\nBob"), "3 players Ann Bob");
        assert_eq!(one_line("Сейф"), "Сейф");
    }
}
