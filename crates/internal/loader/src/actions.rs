//! Player actions across the wire: the server asks each mod that declared
//! actions what a player may use, runs what the player picked in the
//! interface or the console named and pushes the hints the player has at
//! hand; the client keeps what its server offered, for the Mods screen, the
//! HUD and the actions window.
//!
//! The server decides everything. A client only shows the sheet the server
//! sent and sends back an action's id and arguments; the server runs the
//! mod's own checks again on every run, and the mod's handler answers. The
//! reply reaches the player as one of the game's notices and on the screen
//! that sent it.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use enshwick::abi::{ActionCall, ActionFn, Header, PlayerInfo, Status, Str};
use enshwick::actions::sheet::{self, Entry};

use crate::host::Loader;
use crate::remote::Message;

/// How often the server looks for hints.
const HINT_GAP: Duration = Duration::from_secs(1);
/// The least time between two runs one player sends.
const RUN_GAP: Duration = Duration::from_millis(200);

/// A mod's actions callback.
#[derive(Clone, Copy)]
struct Registered {
    owner: usize,
    callback: ActionFn,
    user: usize,
}

static REGISTERED: Mutex<Vec<Registered>> = Mutex::new(Vec::new());

/// The server's state per client: the language it reads, the hints last
/// sent and when it last ran an action.
#[derive(Default)]
struct Client {
    language: u32,
    hints: String,
    last_run: Option<Instant>,
}

struct ServerState {
    clients: HashMap<u64, Client>,
    next_hints: Option<Instant>,
}

static SERVER: Mutex<Option<ServerState>> = Mutex::new(None);

fn server_state<R>(f: impl FnOnce(&mut ServerState) -> R) -> R {
    let mut slot = SERVER.lock().unwrap_or_else(PoisonError::into_inner);
    f(slot.get_or_insert_with(|| ServerState {
        clients: HashMap::new(),
        next_hints: None,
    }))
}

/// Registers mod `owner`'s actions callback; a second one is refused unless
/// the mod is initialised again.
pub(crate) fn register(
    owner: usize,
    callback: ActionFn,
    user: *mut core::ffi::c_void,
    again: bool,
) -> Status {
    let mut registered = REGISTERED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(known) = registered.iter_mut().find(|r| r.owner == owner) {
        if !again {
            return Status::REFUSED;
        }
        known.callback = callback;
        known.user = user.expose_provenance();
        return Status::OK;
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
    let registered = REGISTERED.lock().unwrap_or_else(PoisonError::into_inner);
    registered
        .iter()
        .copied()
        .filter(|r| {
            loader
                .mods
                .get(r.owner)
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
    let registered = running(loader).into_iter().find(|r| r.owner == index)?;
    let console = crate::console::the_console();
    let reply = ask(registered, ActionCall::CONSOLE, &console, 0, rest, "");
    Some(reply.get(1..).unwrap_or_default().to_owned())
}

/// The running mods that declared actions, by index.
pub(crate) fn declaring(loader: &Loader) -> Vec<usize> {
    running(loader).into_iter().map(|r| r.owner).collect()
}

/// The connected player with account key `key`.
fn player(loader: &Loader, key: u64) -> Option<PlayerInfo> {
    crate::world::players(loader)
        .ok()?
        .into_iter()
        .find(|p| p.key == key)
}

/// `text`, a sheet, cut until its message fits a control message: players
/// first, then whole actions from the end.
fn fitted(id: &str, label: &str, text: String) -> String {
    let fits = |text: &str| {
        Message::ActionsSheet {
            first: true,
            last: true,
            id: id.to_owned(),
            label: label.to_owned(),
            text: text.to_owned(),
        }
        .body()
        .len()
            <= crate::net::MAX_CONTROL
    };
    if fits(&text) {
        return text;
    }
    let mut entries = sheet::decode(&text);
    while !fits(&sheet::encode(&entries)) {
        if let Some(entry) = entries.iter_mut().rev().find(|e| !e.players.is_empty()) {
            let keep = entry.players.len() / 2;
            entry.players.truncate(keep);
        } else if entries.pop().is_none() {
            break;
        }
    }
    sheet::encode(&entries)
}

/// Sends the client of `key` its player's actions, one message per mod.
fn send_sheets(loader: &Loader, key: u64, language: u32) {
    let Some(player) = player(loader, key) else {
        return;
    };
    let sheets: Vec<(String, String, String)> = running(loader)
        .into_iter()
        .filter_map(|r| {
            let text = ask(r, ActionCall::SHEET, &player, language, "", "");
            let record = loader.mods.get(r.owner)?;
            (!text.is_empty()).then(|| {
                let label = record.label();
                let text = fitted(&record.id, &label, text);
                (record.id.clone(), label, text)
            })
        })
        .collect();
    if sheets.is_empty() {
        crate::net::to_player(
            key,
            &Message::ActionsSheet {
                first: true,
                last: true,
                id: String::new(),
                label: String::new(),
                text: String::new(),
            },
        );
        return;
    }
    let count = sheets.len();
    for (i, (id, label, text)) in sheets.into_iter().enumerate() {
        crate::net::to_player(
            key,
            &Message::ActionsSheet {
                first: i == 0,
                last: i + 1 == count,
                id,
                label,
                text,
            },
        );
    }
}

/// A client asks for its player's actions, on the server's game thread.
pub(crate) fn query(loader: &Loader, key: u64, language: u32) {
    server_state(|s| s.clients.entry(key).or_default().language = language);
    send_sheets(loader, key, language);
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
    let Some(player) = player(loader, key) else {
        return;
    };
    let Some(registered) = running(loader)
        .into_iter()
        .find(|r| loader.mods.get(r.owner).is_some_and(|m| m.id == id))
    else {
        return;
    };
    let reply = ask(registered, ActionCall::RUN, &player, language, action, args);
    let (ok, text) = match reply.split_at_checked(1) {
        Some(("1", text)) => (true, text.to_owned()),
        Some((_, text)) => (false, text.to_owned()),
        None => (false, String::new()),
    };
    let name =
        String::from_utf8_lossy(&player.name[..usize::from(player.name_len).min(64)]).into_owned();
    crate::log!(
        DEBUG,
        "{name} ({key:016x}) ran {id}:{action} from the interface: {}",
        if ok { "done" } else { "refused" }
    );
    if !text.is_empty() {
        let shown: String = text.lines().next().unwrap_or("").to_owned();
        let _shown = crate::notices::send(loader, key, cut(&shown, crate::notices::MAX_TEXT));
    }
    crate::net::to_player(key, &Message::ActionResult { ok, text });
    send_sheets(loader, key, language);
    // The run may have changed what is at hand.
    server_state(|s| {
        if let Some(client) = s.clients.get_mut(&key) {
            client.hints.clear();
        }
        s.next_hints = None;
    });
}

/// `text` cut to at most `max` bytes, at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Pushes each client the hints its player has at hand, once a second, on
/// the server's game thread.
pub(crate) fn after_step(loader: &Loader) {
    let now = Instant::now();
    let due = server_state(|s| {
        if s.next_hints.is_some_and(|next| now < next) {
            return false;
        }
        s.next_hints = Some(now + HINT_GAP);
        true
    });
    if !due {
        return;
    }
    let mods = running(loader);
    if mods.is_empty() {
        return;
    }
    let keys = crate::net::clients();
    if keys.is_empty() {
        return;
    }
    let Ok(players) = crate::world::players(loader) else {
        return;
    };
    for key in keys {
        let Some(player) = players.iter().find(|p| p.key == key) else {
            continue;
        };
        let language = server_state(|s| s.clients.entry(key).or_default().language);
        let mut text = String::new();
        for r in &mods {
            let Some(record) = loader.mods.get(r.owner) else {
                continue;
            };
            let lines = ask(*r, ActionCall::HINTS, player, language, "", "");
            for line in lines.lines().filter(|l| !l.is_empty()) {
                text.push_str(&sheet::clean(&record.id));
                text.push('\t');
                text.push_str(line);
                text.push('\n');
            }
        }
        let changed = server_state(|s| {
            let client = s.clients.entry(key).or_default();
            if client.hints == text {
                return false;
            }
            client.hints.clone_from(&text);
            true
        });
        if changed {
            crate::net::to_player(key, &Message::ActionHints { text });
        }
    }
}

/// Forgets the clients, as when the server's world goes away.
pub(crate) fn forget() {
    *SERVER.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

/// A mod's actions as the client heard them.
#[derive(Clone, Debug, Default)]
pub(crate) struct ModSheet {
    /// The mod's id.
    pub(crate) id: String,
    /// Its name.
    pub(crate) label: String,
    /// Its actions.
    pub(crate) entries: Vec<Entry>,
}

/// A hint the client heard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Hint {
    /// The mod.
    pub(crate) id: String,
    /// The action.
    pub(crate) action: String,
    /// Its label.
    pub(crate) label: String,
    /// What it acts on.
    pub(crate) subject: String,
}

/// What the client heard from its server.
#[derive(Debug, Default)]
pub(crate) struct Heard {
    /// The mods with actions, once the server answered.
    pub(crate) sheets: Option<Vec<ModSheet>>,
    /// Sheets of an answer still coming.
    incoming: Vec<ModSheet>,
    /// The last run's outcome.
    pub(crate) result: Option<(bool, String)>,
    /// The hints at hand.
    pub(crate) hints: Vec<Hint>,
    /// Counts what was heard, so screens notice news.
    pub(crate) news: u64,
}

static HEARD: Mutex<Option<Heard>> = Mutex::new(None);

/// Reads what the client heard, under the lock.
pub(crate) fn heard<R>(read: impl FnOnce(&Heard) -> R) -> R {
    let mut slot = HEARD.lock().unwrap_or_else(PoisonError::into_inner);
    read(slot.get_or_insert_with(Heard::default))
}

fn hear(change: impl FnOnce(&mut Heard)) {
    let mut slot = HEARD.lock().unwrap_or_else(PoisonError::into_inner);
    let heard = slot.get_or_insert_with(Heard::default);
    change(heard);
    heard.news += 1;
}

/// A sheet the server sent, on the client.
pub(crate) fn sheet_heard(first: bool, last: bool, id: String, label: String, text: &str) {
    hear(|heard| {
        if first {
            heard.incoming.clear();
        }
        if !id.is_empty() {
            heard.incoming.push(ModSheet {
                id,
                label,
                entries: sheet::decode(text),
            });
        }
        if last {
            heard.sheets = Some(std::mem::take(&mut heard.incoming));
        }
    });
}

/// The answer to a run, on the client.
pub(crate) fn result_heard(ok: bool, text: String) {
    hear(|heard| heard.result = Some((ok, text)));
}

/// The hints the server sent, on the client.
pub(crate) fn hints_heard(text: &str) {
    let hints = parse_hints(text);
    hear(|heard| heard.hints = hints);
}

fn parse_hints(text: &str) -> Vec<Hint> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            Some(Hint {
                id: fields.next()?.to_owned(),
                action: fields.next()?.to_owned(),
                label: fields.next()?.to_owned(),
                subject: fields.next()?.to_owned(),
            })
        })
        .collect()
}

/// Asks the server for the player's actions; false while it takes no
/// messages.
pub(crate) fn ask_server() -> bool {
    hear(|heard| heard.result = None);
    crate::net::to_server(&Message::ActionsQuery {
        language: crate::content::language().0,
    })
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
/// language, which the hints need, and forgets the last server's actions.
pub(crate) fn joined() {
    *HEARD.lock().unwrap_or_else(PoisonError::into_inner) = None;
    let _asked = ask_server();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_are_read_from_their_lines() {
        let hints = parse_hints("you.trading-post\taccess\tWho may use it\tYour stall\nbad line\n");
        assert_eq!(
            hints,
            [Hint {
                id: "you.trading-post".into(),
                action: "access".into(),
                label: "Who may use it".into(),
                subject: "Your stall".into(),
            }]
        );
    }

    #[test]
    fn a_long_sheet_loses_players_before_actions() {
        let entry = Entry {
            id: "access".into(),
            label: "Who".into(),
            per_player: true,
            offered: true,
            players: (0..2000)
                .map(|i| (i, format!("Player {i}"), false))
                .collect(),
            ..Entry::default()
        };
        let text = fitted("you.trading-post", "Trading Post", sheet::encode(&[entry]));
        let entries = sheet::decode(&text);
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].players.is_empty() && entries[0].players.len() < 2000);
        assert_eq!(cut("Сейф", 3), "С");
    }

    #[test]
    fn sheets_arrive_in_parts_and_replace_the_last_answer() {
        sheet_heard(true, false, "a".into(), "A".into(), "A\tx\t4\tX\t\t\n");
        assert!(heard(|h| h.sheets.is_none()));
        sheet_heard(false, true, "b".into(), "B".into(), "");
        let ids = heard(|h| {
            h.sheets
                .as_ref()
                .map(|s| s.iter().map(|m| m.id.clone()).collect::<Vec<_>>())
        });
        assert_eq!(ids, Some(vec!["a".to_owned(), "b".to_owned()]));
        sheet_heard(true, true, String::new(), String::new(), "");
        assert_eq!(heard(|h| h.sheets.as_ref().map(Vec::len)), Some(0));
    }
}
