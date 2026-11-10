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

use enshwick::abi::{
    ActionCall, ActionFn, ActionReply, ActionTarget, Header, PlayerInfo, Status, Str,
};

use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::remote::Message;

/// The least time between two runs one player sends.
const RUN_GAP: Duration = Duration::from_millis(200);

/// The most bytes of arguments a run a client sends takes: enshwick's
/// bound, room for several texts of the most bytes a text argument takes.
const MAX_ARGS: usize = 2_048;

/// Whether a client's arguments `args` pass the boundary: within
/// [`MAX_ARGS`] and without a control character but the tab between
/// fields; the mod checks each value against its argument again.
fn args_in_bounds(args: &str) -> bool {
    args.len() <= MAX_ARGS && !args.chars().any(|c| c.is_control() && c != '\t')
}

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

/// Asks `registered` its question `kind` about `player`, with the action,
/// its arguments and the word that named the mod in a typed line, and the
/// object `target` the run names; the answer.
fn ask(
    registered: Registered,
    kind: u32,
    player: &PlayerInfo,
    language: u32,
    (action, args, word): (&str, &str, &str),
    target: ActionTarget,
) -> Answer {
    let mut written: ActionReply = bytemuck::Zeroable::zeroed();
    let call = ActionCall {
        header: Header::of::<ActionCall>(),
        kind,
        language,
        player: *player,
        action: Str::new(action),
        args: Str::new(args),
        target,
        reply: if answers_text(kind) {
            core::ptr::null_mut()
        } else {
            &raw mut written
        },
        word: Str::new(word),
    };
    let mut reply = Str::EMPTY;
    // SAFETY: the callback lives in the mod's DLL, which stays loaded; the
    // safe layer catches its panics. `written` outlives the call.
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
    if written.header.size != 0 {
        // SAFETY: the mod wrote the reply, whose text stays valid until the
        // callback runs again.
        let text = unsafe { written.text.as_str() }.unwrap_or("").to_owned();
        return Answer {
            done: written.done == 1,
            text,
        };
    }
    // SAFETY: the reply stays valid until the callback runs again.
    let text = unsafe { reply.as_str() }.unwrap_or("");
    if answers_text(kind) {
        return Answer {
            done: true,
            text: text.to_owned(),
        };
    }
    // A mod built before ABI 11.19 answers a run as `1` or `0` and the text.
    let (done, text) = match text.split_at_checked(1) {
        Some((flag, text)) => (flag == "1", text),
        None => (false, ""),
    };
    Answer {
        done,
        text: text.to_owned(),
    }
}

/// Whether a mod answers question `kind` with a text alone, a sheet or a
/// window, not a run's outcome.
const fn answers_text(kind: u32) -> bool {
    kind == ActionCall::SHEET || kind == ActionCall::WINDOW
}

/// Mod `owner`'s window `id` as `player` sees it in `language`: its text,
/// empty when the player may not see it; `None` when the mod is off or
/// declared no actions.
pub(crate) fn window(
    loader: &Loader,
    owner: ModIndex,
    player: &PlayerInfo,
    language: u32,
    id: &str,
) -> Option<String> {
    let registered = running(loader).into_iter().find(|r| r.owner == owner)?;
    let answer = ask(
        registered,
        ActionCall::WINDOW,
        player,
        language,
        (id, "", ""),
        ActionTarget::NONE,
    );
    Some(answer.text)
}

/// What a mod answered: whether a run was done, and its text, or its
/// sheet's text.
#[derive(Debug, Default)]
pub(crate) struct Answer {
    pub(crate) done: bool,
    pub(crate) text: String,
}

/// Runs the console line `rest` on the actions of mod `index`, named there
/// by `word`; the answer, or `None` when the mod is off or declared no
/// actions.
pub(crate) fn console(loader: &Loader, index: usize, word: &str, rest: &str) -> Option<String> {
    let console = crate::console::the_console();
    let reply = command(
        loader,
        ModIndex::new(index),
        ActionCall::CONSOLE,
        &console,
        0,
        (word, rest),
    )?;
    Some(reply.text)
}

/// Asks mod `owner` to run the text form `rest` of one of its actions for
/// `caller`, as question `kind` ([`ActionCall::CONSOLE`],
/// [`ActionCall::CHAT`] or [`ActionCall::REMOTE`]), `word` being what named
/// the mod in the line, in lower case; its reply, `1` or `0` and the text,
/// or `None` when the mod is off or declared no actions.
pub(crate) fn command(
    loader: &Loader,
    owner: ModIndex,
    kind: u32,
    caller: &PlayerInfo,
    language: u32,
    (word, rest): (&str, &str),
) -> Option<Answer> {
    let registered = running(loader).into_iter().find(|r| r.owner == owner)?;
    Some(ask(
        registered,
        kind,
        caller,
        language,
        (rest, "", word),
        ActionTarget::NONE,
    ))
}

/// Admits a run that the player of account `key` sends now, at most one per
/// [`RUN_GAP`]: the connected player and the language they read, or `None`
/// for a run too soon or a player not in the world.
fn admit(loader: &Loader, key: u64) -> Option<(PlayerInfo, u32)> {
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
        return None;
    }
    caller(loader, key, language)
}

/// The connected player of account `key` and the language they read,
/// `hello` being the one their client named in its hello.
fn caller(loader: &Loader, key: u64, hello: u32) -> Option<(PlayerInfo, u32)> {
    Some((player(loader, key)?, read_language(key, hello)))
}

/// The language the player of account `key` reads: the one their client
/// names now, else `hello`, the one it named in its hello.
fn read_language(key: u64, hello: u32) -> u32 {
    crate::net::language_of(key).map_or(hello, |l| l.0)
}

/// The connected player of account `key` and the language they read, for a
/// command they typed, which counts against the chat's rate, not
/// [`RUN_GAP`].
pub(crate) fn commander(loader: &Loader, key: u64) -> Option<(PlayerInfo, u32)> {
    Some((player(loader, key)?, language_of(key)))
}

/// The language the player of account `key` reads, as their client named
/// it; 0, the fallback, for one that named none.
pub(crate) fn language_of(key: u64) -> u32 {
    read_language(
        key,
        server_state(|s| s.clients.get(&key).map_or(0, |c| c.language)),
    )
}

/// Tells the mods that the client of account `key` closed mod `owner`'s
/// window `window`, given as `Some((owner, window))`, which that mod hears,
/// or with `None` that the player left, which every running mod with
/// actions hears. The mods get the key alone, since the player may be gone.
pub(crate) fn window_closed(loader: &Loader, key: u64, closed: Option<(ModIndex, &str)>) {
    let who = PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: u32::MAX,
        entity: enshwick::Entity(0),
        key: enshwick::PlayerKey::new(key),
        permissions: 0,
        name_len: 0,
        name: [0; 64],
    };
    for registered in running(loader) {
        let window = match closed {
            None => "",
            Some((owner, window)) if owner == registered.owner => window,
            Some(_) => continue,
        };
        let _heard = ask(
            registered,
            ActionCall::WINDOW_CLOSED,
            &who,
            0,
            (window, "", ""),
            ActionTarget::NONE,
        );
    }
}

/// The running mods that declared actions, by index.
pub(crate) fn declaring(loader: &Loader) -> Vec<ModIndex> {
    running(loader).into_iter().map(|r| r.owner).collect()
}

/// The ids of mod `owner`'s actions `player` may run now, from the sheet
/// the mod gives for them; `None` when the mod is off or declared no
/// actions.
pub(crate) fn allowed(
    loader: &Loader,
    owner: ModIndex,
    player: &PlayerInfo,
) -> Option<Vec<String>> {
    let registered = running(loader).into_iter().find(|r| r.owner == owner)?;
    let sheet = ask(
        registered,
        ActionCall::SHEET,
        player,
        0,
        ("", "", ""),
        ActionTarget::NONE,
    );
    Some(sheet_ids(&sheet.text))
}

/// The action ids a mod's sheet lists.
fn sheet_ids(sheet: &str) -> Vec<String> {
    enshwick::actions::sheet::decode(sheet)
        .into_iter()
        .map(|entry| entry.id)
        .collect()
}

/// The connected player with account key `key`.
pub(crate) fn player(loader: &Loader, key: u64) -> Option<PlayerInfo> {
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

/// A client runs mod `id`'s action on the object `target` its client named,
/// on the server's game thread.
pub(crate) fn run(
    loader: &Loader,
    key: u64,
    id: &str,
    action: &str,
    args: &str,
    target: Option<ActionTarget>,
) {
    if !args_in_bounds(args) {
        crate::log!(
            DEBUG,
            "{key:016x} sent {} bytes of arguments out of bounds for {id}:{action}",
            args.len()
        );
        return;
    }
    let Some((player, language)) = admit(loader, key) else {
        return;
    };
    let Some(registered) = running(loader)
        .into_iter()
        .find(|r| loader.mods.get(r.owner.get()).is_some_and(|m| m.id == id))
    else {
        return;
    };
    let target = match target {
        None => ActionTarget::NONE,
        Some(named) => {
            let position = |entity| crate::world::fixed_position(loader, entity);
            let Some(target) = reached(named, player.entity, |e| {
                position(e).map(enshwick::ecs::metres)
            }) else {
                crate::log!(
                    DEBUG,
                    "{key:016x} named an object out of reach for {id}:{action}"
                );
                let text =
                    crate::texts::for_player(key, || crate::texts::text("actions.out-of-reach"));
                tell(loader, key, text);
                return;
            };
            target
        }
    };
    let reply = ask(
        registered,
        ActionCall::RUN,
        &player,
        language,
        (action, args, ""),
        target,
    );
    crate::windows::ran(registered.owner, key);
    let (ok, text) = (reply.done, reply.text.as_str());
    let name =
        String::from_utf8_lossy(&player.name[..usize::from(player.name_len).min(64)]).into_owned();
    crate::log!(
        DEBUG,
        "{name} ({key:016x}) ran {id}:{action}: {}",
        if ok { "done" } else { "refused" }
    );
    if !text.is_empty() {
        tell(loader, key, &one_line(text));
    }
}

/// Tells player `key` `text` as a notice, or in chat to a client that
/// cannot show one.
pub(crate) fn tell(loader: &Loader, key: u64, text: &str) {
    if crate::notices::send(loader, key, text).is_err() {
        let _queued = crate::chat::send(key, text);
    }
}

/// How far from the player an object their client names may stand, in
/// metres: enshwick's bound, wider than the reach of anything a player
/// opens where they stand.
const REACH: f64 = 12.0;

/// The object `named` as the server sees it, for `player`, with `position`
/// reading an entity's place in the server's world: a named entity must
/// stand there and gives its own position over the client's, and the
/// object must stand within [`REACH`] of the player; `None` refuses the run.
fn reached(
    named: ActionTarget,
    player: enshwick::Entity,
    position: impl Fn(enshwick::Entity) -> Option<[f64; 3]>,
) -> Option<ActionTarget> {
    let mut target = named;
    if target.flags & ActionTarget::ENTITY != 0 {
        target.position = position(target.entity)?;
        target.flags |= ActionTarget::POSITION;
    }
    if target.flags & ActionTarget::POSITION == 0 {
        return None;
    }
    let from = position(player)?;
    let distance_sq: f64 = (0..3).map(|i| (target.position[i] - from[i]).powi(2)).sum();
    (distance_sq <= REACH * REACH).then_some(target)
}

/// A reply's lines joined into the one line a notice shows.
fn one_line(text: &str) -> String {
    text.lines().collect::<Vec<_>>().join(" ")
}

/// Forgets the clients, as when the server's world goes away.
pub(crate) fn forget() {
    *SERVER.relock() = None;
}

/// Runs mod `id`'s action on the server, on the object `target` names if
/// any; false while it takes no messages.
pub(crate) fn run_on_server(
    id: &str,
    action: &str,
    args: &str,
    target: Option<ActionTarget>,
) -> bool {
    crate::net::to_server(&Message::ActionRun {
        id: id.to_owned(),
        action: action.to_owned(),
        args: args.to_owned(),
        target,
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

    #[test]
    fn the_actions_a_sheet_lists_are_the_ones_carried() {
        use enshwick::actions::sheet::{Entry, encode};

        let entry = |id: &str| Entry {
            id: id.into(),
            label: format!("{id}\tlabel"),
            ..Entry::default()
        };
        assert_eq!(
            sheet_ids(&encode(&[entry("greet"), entry("wipe")])),
            ["greet", "wipe"]
        );
        assert_eq!(sheet_ids(""), Vec::<String>::new());
    }

    #[test]
    fn a_sheet_and_a_window_answer_as_text_and_the_rest_as_runs() {
        assert!(answers_text(ActionCall::SHEET));
        assert!(answers_text(ActionCall::WINDOW));
        for kind in [
            ActionCall::RUN,
            ActionCall::CONSOLE,
            ActionCall::CHAT,
            ActionCall::REMOTE,
        ] {
            assert!(!answers_text(kind), "{kind}");
        }
    }

    #[test]
    fn arguments_over_the_bound_or_with_a_control_character_are_dropped() {
        assert!(args_in_bounds("tBan for griefing\tb1"));
        assert!(args_in_bounds(&"t".repeat(MAX_ARGS)));
        assert!(!args_in_bounds(&"t".repeat(MAX_ARGS + 1)));
        assert!(!args_in_bounds("tline\nbreak"));
        assert!(!args_in_bounds("tbell\u{7}"));
    }

    fn places(entity: enshwick::Entity) -> Option<[f64; 3]> {
        match entity.0 {
            2 => Some([100.0, 5.0, 100.0]),
            40 => Some([104.0, 5.0, 103.0]),
            41 => Some([400.0, 5.0, 100.0]),
            _ => None,
        }
    }

    fn named(flags: u32, entity: u32, position: [f64; 3]) -> ActionTarget {
        ActionTarget {
            flags,
            entity: enshwick::Entity(entity),
            position,
            ..ActionTarget::NONE
        }
    }

    #[test]
    fn a_named_position_counts_only_within_reach_of_the_player() {
        let near = named(ActionTarget::POSITION, 0, [108.0, 5.0, 100.0]);
        assert_eq!(reached(near, enshwick::Entity(2), places), Some(near));
        let far = named(ActionTarget::POSITION, 0, [113.0, 5.0, 100.0]);
        assert_eq!(reached(far, enshwick::Entity(2), places), None);
        assert_eq!(reached(near, enshwick::Entity(9), places), None);
    }

    #[test]
    fn a_named_entity_counts_where_the_server_sees_it_not_where_the_client_says() {
        let both = ActionTarget::ENTITY | ActionTarget::POSITION;
        let lied = named(both, 41, [100.0, 5.0, 100.0]);
        assert_eq!(reached(lied, enshwick::Entity(2), places), None);
        let chest = reached(
            named(ActionTarget::ENTITY, 40, [0.0; 3]),
            enshwick::Entity(2),
            places,
        );
        assert_eq!(chest, Some(named(both, 40, [104.0, 5.0, 103.0])));
        let gone = named(ActionTarget::ENTITY, 77, [100.0, 5.0, 100.0]);
        assert_eq!(reached(gone, enshwick::Entity(2), places), None);
    }
}
