//! Player actions across the wire: the server runs what a mod's control or
//! panel sent, or what the console named, through the mod that declared the
//! action.
//!
//! The server decides everything. A client sends an action's id and
//! arguments; the server runs the mod's own checks again on every run, and
//! the mod's handler answers. The reply reaches the player as one of the
//! game's notices, in the language the client named when it joined.

use std::sync::Mutex;

use enshwick_core::abi::{
    ActionCall, ActionFn, ActionReply, ActionTarget, Header, PlayerInfo, Status, Str,
};

use crate::actions::admission::{self, Path, Refusal, Verdict};
use crate::base::remote::Message;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// A mod's actions callback.
#[derive(Clone, Copy)]
struct Registered {
    owner: ModIndex,
    callback: ActionFn,
    user: usize,
}

static REGISTERED: Mutex<Vec<Registered>> = Mutex::new(Vec::new());

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

/// A question the loader asks a mod: its kind, and for
/// [`ActionCall::REFUSED`] the run's own kind and the reason.
#[derive(Clone, Copy, Debug)]
struct Question {
    kind: u32,
    via: u32,
    reason: u32,
}

impl Question {
    /// The question `kind`, no refusal.
    const fn of(kind: u32) -> Self {
        Self {
            kind,
            via: 0,
            reason: 0,
        }
    }
}

/// Asks `registered` its `question` about `player`, with the action, its
/// arguments and the word that named the mod in a typed line, and the
/// object `target` the run names; the answer.
fn ask(
    registered: Registered,
    question: Question,
    player: &PlayerInfo,
    language: u32,
    (action, args, word): (&str, &str, &str),
    target: ActionTarget,
) -> Answer {
    let kind = question.kind;
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
        via: question.via,
        reason: question.reason,
    };
    let mut reply = Str::EMPTY;
    // SAFETY: the callback lives in the mod's DLL, which stays loaded; the
    // safe layer catches its panics. `written` outlives the call.
    crate::engine::profile::call(
        registered.owner,
        crate::engine::profile::Feature::Actions,
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
        Question::of(ActionCall::WINDOW),
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
    let console = crate::actions::console::the_console();
    let reply = command(
        loader,
        ModIndex::new(index),
        Path::Console,
        &console,
        0,
        (word, rest),
    )?;
    Some(reply.text)
}

/// Asks mod `owner` to run the text form `rest` of one of its actions for
/// `caller`, who typed it on `path` (the console, chat or a remote caller),
/// `word` being what named the mod in the line, in lower case; its reply,
/// or `None` when the mod is off or declared no actions.
pub(crate) fn command(
    loader: &Loader,
    owner: ModIndex,
    path: Path,
    caller: &PlayerInfo,
    language: u32,
    (word, rest): (&str, &str),
) -> Option<Answer> {
    let registered = running(loader).into_iter().find(|r| r.owner == owner)?;
    Some(ask(
        registered,
        Question::of(path.kind()),
        caller,
        language,
        (rest, "", word),
        ActionTarget::NONE,
    ))
}

/// Tells the mods that the client of account `key` closed mod `owner`'s
/// window `window`, given as `Some((owner, window))`, which that mod hears,
/// or with `None` that the player left, which every running mod with
/// actions hears. The mods get the key alone, since the player may be gone.
pub(crate) fn window_closed(loader: &Loader, key: u64, closed: Option<(ModIndex, &str)>) {
    let who = PlayerInfo::without_character(enshwick_core::PlayerKey::new(key), 0, "");
    for registered in running(loader) {
        let window = match closed {
            None => "",
            Some((owner, window)) if owner == registered.owner => window,
            Some(_) => continue,
        };
        let _heard = ask(
            registered,
            Question::of(ActionCall::WINDOW_CLOSED),
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
        Question::of(ActionCall::SHEET),
        player,
        0,
        ("", "", ""),
        ActionTarget::NONE,
    );
    Some(sheet_ids(&sheet.text))
}

/// The action ids a mod's sheet lists.
fn sheet_ids(sheet: &str) -> Vec<String> {
    enshwick_core::actions::sheet::decode(sheet)
        .into_iter()
        .map(|entry| entry.id)
        .collect()
}

/// The connected player with account key `key`.
pub(crate) fn player(loader: &Loader, key: u64) -> Option<PlayerInfo> {
    crate::engine::world::players(loader)
        .ok()?
        .into_iter()
        .find(|p| p.key == enshwick_core::PlayerKey::new(key))
}

/// A client runs mod `id`'s action on the object `target` its client named,
/// on the server's game thread: through the [admission](admission) of the
/// interface, then the mod, and its answer back the one way replies go.
pub(crate) fn run(
    loader: &Loader,
    key: u64,
    id: &str,
    action: &str,
    args: &str,
    target: Option<ActionTarget>,
) {
    let registered = running(loader)
        .into_iter()
        .find(|r| loader.mods.get(r.owner.get()).is_some_and(|m| m.id == id));
    let refused = |refusal| {
        let owner = registered.map(|r| r.owner);
        refuse(
            loader,
            Path::Interface,
            key,
            refusal,
            owner,
            (action, args, ""),
        );
    };
    match admission::admit(loader, Path::Interface, key) {
        Verdict::Run => {}
        Verdict::Refuse(refusal) => return refused(refusal),
        Verdict::Drop => return,
    }
    if let Err(refusal) = admitted_run(loader, key, registered, (id, action, args), target) {
        crate::log!(DEBUG, "{key:016x} ran {id}:{action}: {refusal:?}");
        refused(refusal);
    }
}

/// Answers the caller of account `key` the run from `path` refused for
/// `refusal`, then tells mod `owner`, when it runs, with what the run
/// would have held (action or line, arguments, word), so it hears every
/// refused run of its actions after the caller got the answer.
pub(crate) fn refuse(
    loader: &Loader,
    path: Path,
    key: u64,
    refusal: Refusal,
    owner: Option<ModIndex>,
    held: (&str, &str, &str),
) {
    admission::refuse(loader, path, key, refusal);
    let Some(reason) = refusal.abi_reason() else {
        return;
    };
    let Some(registered) =
        owner.and_then(|owner| running(loader).into_iter().find(|r| r.owner == owner))
    else {
        return;
    };
    let caller = player(loader, key).unwrap_or_else(|| {
        PlayerInfo::without_character(enshwick_core::PlayerKey::new(key), 0, "")
    });
    let question = Question {
        kind: ActionCall::REFUSED,
        via: path.kind(),
        reason,
    };
    let language = crate::base::net::language_number(key);
    let _heard = ask(
        registered,
        question,
        &caller,
        language,
        held,
        ActionTarget::NONE,
    );
}

/// Runs a client's run of `action` of the mod `registered` (whose id the
/// client named as `id`) with `args` past its rate: the peer bounds, the
/// mod, the reach of `target`, then the mod's handler and its answer; the
/// refusal of a run the platform stops before the mod.
fn admitted_run(
    loader: &Loader,
    key: u64,
    registered: Option<Registered>,
    (id, action, args): (&str, &str, &str),
    target: Option<ActionTarget>,
) -> Result<(), Refusal> {
    if !admission::args_in_bounds(args) {
        return Err(Refusal::Bounds);
    }
    // A player not in the world has no screen to answer on.
    let Some(player) = player(loader, key) else {
        return Ok(());
    };
    let registered = registered.ok_or(Refusal::Unavailable)?;
    let target = match target {
        None => ActionTarget::NONE,
        Some(named) => {
            let position = |entity| crate::engine::world::fixed_position(loader, entity);
            reached(named, player.entity, |e| {
                position(e).map(enshwick_core::ecs::metres)
            })
            .ok_or(Refusal::Reach)?
        }
    };
    let reply = ask(
        registered,
        Question::of(ActionCall::RUN),
        &player,
        crate::base::net::language_number(key),
        (action, args, ""),
        target,
    );
    crate::ui::windows::ran(registered.owner, key);
    crate::log!(
        DEBUG,
        "{} ({key:016x}) ran {id}:{action}: {}",
        player.name_text(),
        if reply.done { "done" } else { "refused" }
    );
    crate::actions::reply::answer(loader, Path::Interface, key, reply.done, &reply.text);
    Ok(())
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
    player: enshwick_core::Entity,
    position: impl Fn(enshwick_core::Entity) -> Option<[f64; 3]>,
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

/// Runs mod `id`'s action on the server, on the object `target` names if
/// any; false while it takes no messages.
pub(crate) fn run_on_server(
    id: &str,
    action: &str,
    args: &str,
    target: Option<ActionTarget>,
) -> bool {
    crate::base::net::to_server(&Message::ActionRun {
        id: id.to_owned(),
        action: action.to_owned(),
        args: args.to_owned(),
        target,
    })
}

/// The client joined a server that answered: it tells the server its
/// language.
pub(crate) fn joined() {
    let _sent = crate::base::net::to_server(&Message::ActionsLanguage {
        language: crate::content::served::language().0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_actions_a_sheet_lists_are_the_ones_carried() {
        use enshwick_core::actions::sheet::{Entry, encode};

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

    fn places(entity: enshwick_core::Entity) -> Option<[f64; 3]> {
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
            entity: enshwick_core::Entity(entity),
            position,
            ..ActionTarget::NONE
        }
    }

    #[test]
    fn a_named_position_counts_only_within_reach_of_the_player() {
        let near = named(ActionTarget::POSITION, 0, [108.0, 5.0, 100.0]);
        assert_eq!(reached(near, enshwick_core::Entity(2), places), Some(near));
        let far = named(ActionTarget::POSITION, 0, [113.0, 5.0, 100.0]);
        assert_eq!(reached(far, enshwick_core::Entity(2), places), None);
        assert_eq!(reached(near, enshwick_core::Entity(9), places), None);
    }

    #[test]
    fn a_named_entity_counts_where_the_server_sees_it_not_where_the_client_says() {
        let both = ActionTarget::ENTITY | ActionTarget::POSITION;
        let lied = named(both, 41, [100.0, 5.0, 100.0]);
        assert_eq!(reached(lied, enshwick_core::Entity(2), places), None);
        let chest = reached(
            named(ActionTarget::ENTITY, 40, [0.0; 3]),
            enshwick_core::Entity(2),
            places,
        );
        assert_eq!(chest, Some(named(both, 40, [104.0, 5.0, 103.0])));
        let gone = named(ActionTarget::ENTITY, 77, [100.0, 5.0, 100.0]);
        assert_eq!(reached(gone, enshwick_core::Entity(2), places), None);
    }
}
