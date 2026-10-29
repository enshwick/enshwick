//! The serving side of the join check, on a dedicated server and in a game a
//! client hosts.
//!
//! A server that runs join mods waits for each player who enters the world to
//! say hello with the join mods the player's game runs. A game that says
//! nothing in time, such as one without Enshwick, or one that lacks a join mod
//! or runs it in another version, gets a chat line naming what it needs and
//! where to get it, then the game's own kick; administrators too. A game whose
//! hello matches stays, also when the hello comes after the line. The check is
//! about compatibility, not cheating: a game that lies about its mods passes. A
//! hosted game never checks its host, and checks no one until the host's own
//! hello names it and only while the host's key holds a player slot in the
//! game.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;

use crate::pinned;

mod note;

pub use note::{note, place};

/// The build table ids this part of the loader names.
pub const TABLE_IDS: &[&str] = &[
    pinned::server::SESSION.id(),
    pinned::session::KICK.id(),
    pinned::session::HOST_MACHINE.id(),
];

/// Players the loader follows at most, in the gate and the player reads;
/// the game has 16 slots.
pub(crate) const MAX_PLAYERS: usize = 64;
/// How long a kick that did not remove the player waits to be tried again.
const KICK_AGAIN: Duration = Duration::from_secs(10);
/// What a join's guard, the allowlist or the offline ban, did with a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guard {
    /// The guard let the player stay.
    Passed,
    /// The guard removed the player.
    Removed,
    /// The guard refused the player, but the kick did not remove it yet: it
    /// is tried again.
    Kicking,
    /// The guard refused the player, but this build has no kick, so the
    /// player stays.
    Unkickable,
    /// The guard could not run: it panicked, or an earlier panic turned it
    /// off. The player must still go, so a broken guard never lets anyone in.
    Broken,
}

impl Guard {
    /// The guard's run through its fuse: what it did, or `None` when it
    /// could not run.
    #[must_use]
    pub const fn of(ran: Option<Self>) -> Self {
        match ran {
            Some(guard) => guard,
            None => Self::Broken,
        }
    }

    /// Whether the player must not stay.
    #[must_use]
    pub const fn keeps_out(self) -> bool {
        !matches!(self, Self::Passed)
    }
}

/// How long the server waits on a player who entered the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// From entering the world to the note, for a game that says no hello.
    pub wait: Duration,
    /// From the note to the kick, for the player to read it.
    pub grace: Duration,
}

/// What a player's game said in its hello.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hello {
    /// The game's join mods, `id@version,...`, sorted.
    pub mods: String,
    /// False when the list was cut to fit the hello: mods that sort after
    /// its last entry are not known.
    pub complete: bool,
    /// In a game the client hosts, the host's own number for its hello; 0
    /// elsewhere.
    pub nonce: u64,
    /// The language the player reads the game in; `None` for a hello that
    /// names none.
    pub language: Option<enshwick::i18n::Language>,
}

/// A server join mod the player's game lacks or runs in another version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Need {
    /// The mod's id.
    pub id: String,
    /// The server's version.
    pub version: String,
    /// The version the player's game runs, when it runs the mod.
    pub yours: Option<String>,
}

/// What the server does next for one player, by account key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Tells the player what the game lacks.
    Note {
        /// The player's account key.
        key: u64,
        /// Whether the game said hello.
        hello: bool,
        /// What the game lacks.
        needs: Vec<Need>,
    },
    /// Removes the player through the game's kick.
    Kick {
        /// The player's account key.
        key: u64,
    },
    /// Tells a player who got a note that a late hello matched, so the kick
    /// is off.
    Stay {
        /// The player's account key.
        key: u64,
    },
}

/// The server's word on one player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// Nothing said yet.
    Open,
    /// The hello matched.
    Admitted,
    /// Noted; kicked at this moment.
    Refused(Instant),
    /// Kicked at this moment, and kicked again later while the slot stays.
    Kicked(Instant),
}

/// Where one player stands.
#[derive(Clone, Debug)]
struct Standing {
    /// The game's last hello.
    hello: Option<Hello>,
    /// When the player entered the world; `None` while loading.
    entered: Option<Instant>,
    verdict: Verdict,
}

/// The players of the live slots the server follows.
#[derive(Debug, Default)]
pub struct Gate {
    players: FxHashMap<u64, Standing>,
    /// A game a client hosts: no one is checked until the host is known.
    hosted: bool,
    /// The host's account key, once its own hello came; the first host's
    /// stays.
    host: Option<u64>,
    /// Whether the host's key holds a player slot in the game.
    host_in_game: bool,
    /// The players a join's guard refused, by account key: when the guards
    /// run on each again, or `None` when this build cannot kick them.
    refused: FxHashMap<u64, Option<Instant>>,
}

impl Gate {
    /// The gate of a game a client hosts.
    #[must_use]
    pub fn hosted() -> Self {
        Self {
            hosted: true,
            ..Self::default()
        }
    }

    /// Whether the gate acts: always on a dedicated server, in a hosted game
    /// while the known host's key holds a player slot in the game.
    fn acts(&self) -> bool {
        !self.hosted || self.host_in_game
    }

    /// Takes the keys of the players in the game, `in_game`: a hosted game
    /// acts only while its host is among them, so a host whose hello came
    /// under another key than its slot's is never taken for a guest.
    pub fn host_in_game(&mut self, in_game: &[u64]) {
        self.host_in_game = self.host.is_some_and(|host| in_game.contains(&host));
    }

    /// Makes room for the player `key` in a full gate by forgetting the
    /// players whose key holds no live slot in `live`, read only then;
    /// nothing is forgotten when the slots do not read.
    pub fn make_room(&mut self, key: u64, live: impl FnOnce() -> Option<Vec<u64>>) {
        if !self.players.contains_key(&key)
            && self.players.len() >= MAX_PLAYERS
            && let Some(live) = live()
        {
            self.keep(&live);
        }
    }

    /// The player `key` entered the world at `now` on a server that runs
    /// `server`: the wait starts, or the hello that came while loading is
    /// judged. A full gate refuses the player at once.
    pub fn joined(&mut self, key: u64, server: &str, now: Instant, rules: Rules) -> Vec<Step> {
        if key == 0 || self.host == Some(key) {
            return Vec::new();
        }
        if !self.players.contains_key(&key) && self.players.len() >= MAX_PLAYERS {
            let needs = needs(server, None);
            if needs.is_empty() {
                return Vec::new();
            }
            return vec![
                Step::Note {
                    key,
                    hello: false,
                    needs,
                },
                Step::Kick { key },
            ];
        }
        let acts = self.acts();
        let standing = self.players.entry(key).or_insert(Standing {
            hello: None,
            entered: None,
            verdict: Verdict::Open,
        });
        // A hello from an earlier stay in the world, other than a matching
        // one, says nothing of this one; a hello from the loading screen does.
        if standing.entered.is_some() && standing.verdict != Verdict::Admitted {
            standing.hello = None;
        }
        standing.entered = Some(now);
        standing.verdict = Verdict::Open;
        if acts && standing.hello.is_some() {
            judge(key, standing, server, now, rules)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Takes the `hello` of the player `key` on a server that runs `server`;
    /// whether the server answers it, and what to tell the player.
    pub fn hello(
        &mut self,
        key: u64,
        server: &str,
        hello: &Hello,
        now: Instant,
        rules: Rules,
    ) -> (bool, Option<Step>) {
        if key == 0 {
            return (false, None);
        }
        if self.host == Some(key) {
            return (true, None);
        }
        if !self.players.contains_key(&key) && self.players.len() >= MAX_PLAYERS {
            return (false, None);
        }
        let acts = self.acts();
        let standing = self.players.entry(key).or_insert(Standing {
            hello: None,
            entered: None,
            verdict: Verdict::Open,
        });
        standing.hello = Some(hello.clone());
        let matches = needs(server, Some(hello)).is_empty();
        if standing.entered.is_none() || !acts {
            return (matches, None);
        }
        let before = standing.verdict;
        if !matches && matches!(before, Verdict::Refused(_) | Verdict::Kicked(_)) {
            return (false, None);
        }
        let step = judge(key, standing, server, now, rules);
        let step = match (step, before) {
            (None, Verdict::Refused(_)) if matches => Some(Step::Stay { key }),
            (step, _) => step,
        };
        (matches, step)
    }

    /// Takes the host's own hello in a hosted game: the host is never checked,
    /// and a hello under another key once the host is known is not answered.
    pub fn host_hello(&mut self, key: u64, hello: &Hello) -> (bool, Option<Step>) {
        if key == 0 || !self.hosted || hello.nonce == 0 || self.host.is_some_and(|h| h != key) {
            return (false, None);
        }
        self.host = Some(key);
        self.players.remove(&key);
        (true, None)
    }

    /// Forgets the player `key`, who left.
    pub fn left(&mut self, key: u64) {
        self.players.remove(&key);
        self.refused.remove(&key);
    }

    /// Forgets every player whose key holds no live slot in `live`.
    pub fn keep(&mut self, live: &[u64]) {
        self.players.retain(|key, _| live.contains(key));
        self.refused.retain(|key, _| live.contains(key));
    }

    /// Follows what a join's guards did with the player `key` at `now`, so
    /// a refusal fails closed: a refused player is refused again every
    /// [`KICK_AGAIN`] while its slot stays, whether the kick seemed to work
    /// or not; one this build cannot kick is followed without tries; one the
    /// guards now let in is forgotten. A full list first forgets the
    /// players whose key holds no live slot in `live`, read only then.
    /// Whether the player is newly refused with no kick, for its one error.
    pub fn guarded(
        &mut self,
        key: u64,
        guard: Guard,
        now: Instant,
        live: impl FnOnce() -> Option<Vec<u64>>,
    ) -> bool {
        let next = match guard {
            Guard::Passed => {
                self.refused.remove(&key);
                return false;
            }
            Guard::Unkickable => None,
            Guard::Removed | Guard::Kicking | Guard::Broken => Some(now + KICK_AGAIN),
        };
        if !self.refused.contains_key(&key)
            && self.refused.len() >= MAX_PLAYERS
            && let Some(live) = live()
        {
            self.refused.retain(|key, _| live.contains(key));
        }
        let before = self.refused.insert(key, next);
        next.is_none() && before != Some(None)
    }

    /// The refused players whose guards run again at `now`.
    pub fn refusals_due(&mut self, now: Instant) -> Vec<u64> {
        let mut due = Vec::new();
        for (&key, next) in &mut self.refused {
            if next.is_some_and(|at| now >= at) {
                *next = Some(now + KICK_AGAIN);
                due.push(key);
            }
        }
        due
    }

    /// The steps due at `now` on a server that runs `server`.
    pub fn due(&mut self, server: &str, now: Instant, rules: Rules) -> Vec<Step> {
        if !self.acts() {
            return Vec::new();
        }
        let mut steps = Vec::new();
        for (&key, standing) in &mut self.players {
            let Some(entered) = standing.entered else {
                continue;
            };
            let step = match standing.verdict {
                Verdict::Open if standing.hello.is_some() => {
                    judge(key, standing, server, now, rules)
                }
                Verdict::Open if now.saturating_duration_since(entered) >= rules.wait => {
                    judge(key, standing, server, now, rules)
                }
                Verdict::Refused(at) if now >= at => {
                    standing.verdict = Verdict::Kicked(now);
                    Some(Step::Kick { key })
                }
                Verdict::Kicked(at) if now.saturating_duration_since(at) >= KICK_AGAIN => {
                    standing.verdict = Verdict::Kicked(now);
                    Some(Step::Kick { key })
                }
                _ => None,
            };
            steps.extend(step);
        }
        steps
    }

    /// Whether the gate follows anyone, so a step has work.
    #[must_use]
    pub fn busy(&self) -> bool {
        !self.players.is_empty() || !self.refused.is_empty()
    }
}

/// Judges the player `key` in the world on its hello, or on none: admitted,
/// or refused with a note and the kick after the grace.
fn judge(
    key: u64,
    standing: &mut Standing,
    server: &str,
    now: Instant,
    rules: Rules,
) -> Option<Step> {
    let needs = needs(server, standing.hello.as_ref());
    if needs.is_empty() {
        standing.verdict = Verdict::Admitted;
        return None;
    }
    standing.verdict = Verdict::Refused(now + rules.grace);
    Some(Step::Note {
        key,
        hello: standing.hello.is_some(),
        needs,
    })
}

/// The server's join mods, of `server`, that a game with the `client` hello
/// lacks or runs in another version; every one for a game that said no hello.
///
/// A cut hello lacks only the mods its list could have named.
#[must_use]
pub fn needs(server: &str, client: Option<&Hello>) -> Vec<Need> {
    let mods = client.map_or("", |hello| hello.mods.as_str());
    let yours: BTreeMap<&str, &str> = crate::join::entries(mods).collect();
    let last = client
        .filter(|hello| !hello.complete && !yours.is_empty())
        .and_then(|hello| hello.mods.rsplit(',').next());
    crate::join::wanted(server, mods)
        .into_iter()
        .filter(|(id, _)| {
            yours.contains_key(id.as_str())
                || last.is_none_or(|last| format!("{id}@").as_str() < last)
        })
        .map(|(id, version)| Need {
            yours: yours.get(id.as_str()).map(|&v| v.to_owned()),
            id,
            version,
        })
        .collect()
}

/// The session's own machine from the handle it holds, `raw`: 0 until the
/// session hosts.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "the gate kicks only in the game")
)]
fn own_machine(raw: u32) -> Option<u32> {
    (raw != 0).then_some(raw)
}

/// The machine to kick for the player `key` among the live slots'
/// `machines`, `(key, machine)`: never the host's, nor one the host's slot
/// shares, nor the session's own `host_machine`; or why not.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "the gate kicks only in the game")
)]
fn machine_to_kick(
    machines: &[(u64, u32)],
    key: u64,
    host: Option<u64>,
    host_machine: Option<u32>,
) -> Result<u32, &'static str> {
    if host == Some(key) {
        return Err("the player is the host");
    }
    let machine_of = |key: u64| {
        machines
            .iter()
            .find(|&&(k, _)| k == key)
            .map(|&(_, machine)| machine)
    };
    if key == 0 && machines.iter().filter(|&&(k, _)| k == 0).nth(1).is_some() {
        return Err("several players hold no account, so which one joined is unknown");
    }
    let machine = machine_of(key).ok_or("the player holds no slot")?;
    if host.and_then(machine_of) == Some(machine) {
        return Err("the player's machine is the host's");
    }
    if host_machine == Some(machine) {
        return Err("the player's machine is the session's own");
    }
    Ok(machine)
}

#[cfg(windows)]
pub(crate) use runtime::{
    after_step, can_kick, configure, forget, hello, host, joined, left, nonce, start,
};

#[cfg(windows)]
mod runtime;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guard_that_cannot_run_keeps_the_player_out() {
        let fuse = crate::fuse::Fuse::new("test guard");
        let panicked = fuse.run("an unlisted account's join", || -> Guard {
            panic!("broken")
        });
        assert_eq!(Guard::of(panicked), Guard::Broken);
        let tripped = fuse.run("the next join", || Guard::Passed);
        assert_eq!(Guard::of(tripped), Guard::Broken);
        assert!(Guard::Broken.keeps_out());
    }

    #[test]
    fn a_guard_that_ran_says_what_it_did_with_the_player() {
        assert_eq!(Guard::of(Some(Guard::Removed)), Guard::Removed);
        assert!(Guard::Removed.keeps_out());
        assert_eq!(Guard::of(Some(Guard::Passed)), Guard::Passed);
        assert!(!Guard::Passed.keeps_out());
    }

    #[test]
    fn a_refused_player_the_kick_did_not_remove_never_counts_as_let_in() {
        assert!(Guard::Kicking.keeps_out());
        assert!(Guard::Unkickable.keeps_out());
    }

    fn no_slots() -> Option<Vec<u64>> {
        None
    }

    #[test]
    fn a_refusal_whose_kick_failed_is_tried_again_until_the_player_leaves() {
        let mut gate = Gate::default();
        let start = Instant::now();
        assert!(!gate.guarded(7, Guard::Kicking, start, no_slots));
        assert!(gate.busy());
        assert_eq!(gate.refusals_due(at(start, 9)), Vec::<u64>::new());
        assert_eq!(gate.refusals_due(at(start, 10)), [7]);
        assert_eq!(
            gate.refusals_due(at(start, 11)),
            Vec::<u64>::new(),
            "one try at a time"
        );
        assert!(!gate.guarded(7, Guard::Kicking, at(start, 10), no_slots));
        assert_eq!(gate.refusals_due(at(start, 20)), [7]);
        gate.left(7);
        assert!(!gate.busy());
        assert_eq!(gate.refusals_due(at(start, 60)), Vec::<u64>::new());
    }

    #[test]
    fn a_removed_or_unchecked_player_whose_slot_stays_is_refused_again() {
        for guard in [Guard::Removed, Guard::Broken] {
            let mut gate = Gate::default();
            let start = Instant::now();
            gate.guarded(7, guard, start, no_slots);
            assert_eq!(gate.refusals_due(at(start, 10)), [7], "{guard:?}");
        }
    }

    #[test]
    fn a_player_the_guards_now_let_in_is_no_longer_refused() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.guarded(7, Guard::Kicking, start, no_slots);
        assert!(!gate.guarded(7, Guard::Passed, at(start, 10), no_slots));
        assert!(!gate.busy());
        assert_eq!(gate.refusals_due(at(start, 60)), Vec::<u64>::new());
    }

    #[test]
    fn a_refusal_this_build_cannot_kick_is_reported_once_and_not_tried_again() {
        let mut gate = Gate::default();
        let start = Instant::now();
        assert!(gate.guarded(7, Guard::Unkickable, start, no_slots));
        assert!(!gate.guarded(7, Guard::Unkickable, at(start, 5), no_slots));
        assert!(
            gate.guarded(8, Guard::Unkickable, start, no_slots),
            "per player"
        );
        assert_eq!(gate.refusals_due(at(start, 60)), Vec::<u64>::new());
        assert!(gate.busy());
        gate.keep(&[8]);
        assert!(
            gate.guarded(7, Guard::Unkickable, at(start, 70), no_slots),
            "a new stay is reported again"
        );
    }

    #[test]
    fn refused_players_whose_slot_is_gone_are_forgotten() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.guarded(7, Guard::Kicking, start, no_slots);
        gate.keep(&[9]);
        assert!(!gate.busy());
        assert_eq!(gate.refusals_due(at(start, 60)), Vec::<u64>::new());
    }

    #[test]
    fn refusals_are_tried_again_in_a_hosted_game_that_does_not_know_its_host() {
        let mut gate = Gate::hosted();
        let start = Instant::now();
        gate.guarded(7, Guard::Kicking, start, no_slots);
        assert_eq!(gate.refusals_due(at(start, 10)), [7]);
    }

    #[test]
    fn a_full_refusal_list_makes_room_from_players_whose_slot_is_gone() {
        let mut gate = Gate::default();
        let start = Instant::now();
        for key in 1..=MAX_PLAYERS as u64 {
            gate.guarded(key, Guard::Kicking, start, no_slots);
        }
        let newcomer = MAX_PLAYERS as u64 + 1;
        gate.guarded(newcomer, Guard::Kicking, start, || Some(vec![1, newcomer]));
        assert_eq!(gate.refusals_due(at(start, 10)).len(), 2);
    }

    const SERVER: &str = "you.trading-post@0.3.0,zones@0.1.0";

    fn rules() -> Rules {
        Rules {
            wait: Duration::from_secs(30),
            grace: Duration::from_secs(10),
        }
    }

    fn need(id: &str, version: &str, yours: Option<&str>) -> Need {
        Need {
            id: id.to_owned(),
            version: version.to_owned(),
            yours: yours.map(str::to_owned),
        }
    }

    fn said(mods: &str) -> Hello {
        Hello {
            mods: mods.to_owned(),
            complete: true,
            nonce: 0,
            language: None,
        }
    }

    fn note(key: u64, hello: bool, needs: Vec<Need>) -> Step {
        Step::Note { key, hello, needs }
    }

    fn all_lacking() -> Vec<Need> {
        vec![
            need("you.trading-post", "0.3.0", None),
            need("zones", "0.1.0", None),
        ]
    }

    fn at(start: Instant, seconds: u64) -> Instant {
        start + Duration::from_secs(seconds)
    }

    #[test]
    fn a_game_that_never_says_hello_is_told_then_kicked() {
        let mut gate = Gate::default();
        let start = Instant::now();
        assert_eq!(gate.joined(7, SERVER, start, rules()), []);
        assert!(gate.busy());
        assert_eq!(gate.due(SERVER, at(start, 29), rules()), []);
        assert_eq!(
            gate.due(SERVER, at(start, 30), rules()),
            [note(7, false, all_lacking())]
        );
        assert_eq!(gate.due(SERVER, at(start, 30), rules()), [], "one note");
        assert_eq!(
            gate.due(SERVER, at(start, 40), rules()),
            [Step::Kick { key: 7 }]
        );
        assert_eq!(
            gate.due(SERVER, at(start, 41), rules()),
            [],
            "one kick at a time"
        );
    }

    #[test]
    fn a_kick_that_did_not_remove_the_player_is_tried_again() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, SERVER, start, rules());
        gate.due(SERVER, at(start, 30), rules());
        assert_eq!(
            gate.due(SERVER, at(start, 40), rules()),
            [Step::Kick { key: 7 }]
        );
        assert_eq!(gate.due(SERVER, at(start, 49), rules()), []);
        assert_eq!(
            gate.due(SERVER, at(start, 50), rules()),
            [Step::Kick { key: 7 }]
        );
    }

    #[test]
    fn kicked_never_left_joins_again() {
        let mut gate = Gate::default();
        let start = Instant::now();
        let (stays, step) = gate.hello(7, SERVER, &said("you.trading-post@0.2.0"), start, rules());
        assert_eq!(
            (stays, step),
            (false, None),
            "no note on the loading screen"
        );
        assert_eq!(
            gate.joined(7, SERVER, at(start, 5), rules()),
            [note(
                7,
                true,
                vec![
                    need("you.trading-post", "0.3.0", Some("0.2.0")),
                    need("zones", "0.1.0", None)
                ]
            )]
        );
        assert_eq!(
            gate.due(SERVER, at(start, 15), rules()),
            [Step::Kick { key: 7 }]
        );
        // The player's slot never reported a leave; the same key enters again.
        assert_eq!(gate.joined(7, SERVER, at(start, 60), rules()), []);
        assert_eq!(
            gate.due(SERVER, at(start, 90), rules()),
            [note(7, false, all_lacking())],
            "the new join waits and is told again"
        );
        assert_eq!(
            gate.due(SERVER, at(start, 100), rules()),
            [Step::Kick { key: 7 }]
        );
    }

    #[test]
    fn players_whose_slot_is_gone_are_forgotten() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, SERVER, start, rules());
        gate.hello(8, SERVER, &said("you.trading-post@0.2.0"), start, rules());
        gate.keep(&[9]);
        assert!(!gate.busy());
        assert_eq!(gate.due(SERVER, at(start, 60), rules()), []);
    }

    #[test]
    fn grace_starts_when_the_player_enters_the_world() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.hello(7, SERVER, &said(""), start, rules());
        assert_eq!(
            gate.due(SERVER, at(start, 100), rules()),
            [],
            "still loading"
        );
        assert_eq!(gate.joined(7, SERVER, at(start, 100), rules()).len(), 1);
        assert_eq!(gate.due(SERVER, at(start, 109), rules()), []);
        assert_eq!(
            gate.due(SERVER, at(start, 110), rules()),
            [Step::Kick { key: 7 }]
        );
    }

    #[test]
    fn a_matching_hello_lets_the_player_stay_and_extra_mods_do_not_count() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, SERVER, start, rules());
        let client = said("camera@1.0.0,you.trading-post@0.3.0,zones@0.1.0");
        assert_eq!(gate.hello(7, SERVER, &client, start, rules()), (true, None));
        assert_eq!(gate.due(SERVER, at(start, 3600), rules()), []);
    }

    #[test]
    fn a_matching_hello_before_the_join_is_answered_and_kept() {
        let mut gate = Gate::default();
        let start = Instant::now();
        assert_eq!(
            gate.hello(7, SERVER, &said(SERVER), start, rules()),
            (true, None)
        );
        assert_eq!(gate.joined(7, SERVER, start, rules()), []);
        assert_eq!(gate.due(SERVER, at(start, 3600), rules()), []);
    }

    #[test]
    fn an_admitted_player_is_checked_again_when_the_server_s_mods_changed() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.hello(
            7,
            "you.trading-post@0.3.0",
            &said("you.trading-post@0.3.0"),
            start,
            rules(),
        );
        assert_eq!(
            gate.joined(7, SERVER, start, rules()),
            [note(7, true, vec![need("zones", "0.1.0", None)])]
        );
    }

    #[test]
    fn a_hello_that_lacks_a_mod_in_the_world_is_told_at_once_and_kicked() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, SERVER, start, rules());
        let (stays, step) = gate.hello(7, SERVER, &said("you.trading-post@0.2.0"), start, rules());
        assert!(!stays);
        assert_eq!(
            step,
            Some(note(
                7,
                true,
                vec![
                    need("you.trading-post", "0.3.0", Some("0.2.0")),
                    need("zones", "0.1.0", None)
                ]
            ))
        );
        assert_eq!(
            gate.hello(7, SERVER, &said("you.trading-post@0.2.0"), start, rules()),
            (false, None),
            "a repeated hello is not told again"
        );
        assert_eq!(
            gate.due(SERVER, at(start, 10), rules()),
            [Step::Kick { key: 7 }]
        );
    }

    #[test]
    fn a_late_matching_hello_calls_the_kick_off() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, SERVER, start, rules());
        assert_eq!(gate.due(SERVER, at(start, 30), rules()).len(), 1);
        assert_eq!(
            gate.hello(7, SERVER, &said(SERVER), at(start, 31), rules()),
            (true, Some(Step::Stay { key: 7 }))
        );
        assert_eq!(gate.due(SERVER, at(start, 40), rules()), []);
    }

    #[test]
    fn a_server_without_join_mods_admits_everyone() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, "", start, rules());
        assert_eq!(gate.due("", at(start, 60), rules()), []);
    }

    #[test]
    fn an_unknown_sender_is_never_answered_or_told() {
        let mut gate = Gate::default();
        let start = Instant::now();
        assert_eq!(
            gate.hello(0, SERVER, &said(SERVER), start, rules()),
            (false, None)
        );
        assert_eq!(gate.joined(0, SERVER, start, rules()), []);
        assert_eq!(gate.due(SERVER, at(start, 60), rules()), []);
    }

    #[test]
    fn a_full_gate_refuses_newcomers() {
        let mut gate = Gate::default();
        let start = Instant::now();
        for key in 1..=MAX_PLAYERS as u64 {
            gate.joined(key, SERVER, start, rules());
        }
        assert_eq!(gate.players.len(), MAX_PLAYERS);
        let key = MAX_PLAYERS as u64 + 1;
        assert_eq!(
            gate.joined(key, SERVER, start, rules()),
            [note(key, false, all_lacking()), Step::Kick { key }]
        );
        assert_eq!(
            gate.hello(key + 1, SERVER, &said(SERVER), start, rules()),
            (false, None)
        );
        assert_eq!(gate.players.len(), MAX_PLAYERS);
    }

    #[test]
    fn a_full_gate_makes_room_from_players_whose_slot_is_gone() {
        let mut gate = Gate::default();
        let start = Instant::now();
        for key in 1..=MAX_PLAYERS as u64 {
            gate.joined(key, SERVER, start, rules());
        }
        let key = MAX_PLAYERS as u64 + 1;
        gate.make_room(1, || unreachable!("a followed player needs no room"));
        gate.make_room(key, || None);
        assert_eq!(
            gate.players.len(),
            MAX_PLAYERS,
            "no slots read, none forgotten"
        );
        gate.make_room(key, || Some(vec![2, key]));
        assert_eq!(gate.joined(key, SERVER, start, rules()), []);
        let mut followed: Vec<u64> = gate.players.keys().copied().collect();
        followed.sort_unstable();
        assert_eq!(followed, [2, key]);
        let mut roomy = Gate::default();
        roomy.joined(1, SERVER, start, rules());
        roomy.make_room(2, || unreachable!("a gate with room reads no slots"));
    }

    #[test]
    fn a_hosted_game_acts_only_while_its_host_holds_a_live_slot() {
        let mut gate = Gate::hosted();
        let start = Instant::now();
        let host = Hello {
            nonce: 5,
            ..said("")
        };
        assert_eq!(gate.host_hello(1, &host), (true, None));
        gate.joined(2, SERVER, start, rules());
        assert_eq!(
            gate.due(SERVER, at(start, 60), rules()),
            [],
            "the host's key holds no slot"
        );
        gate.host_in_game(&[2, 3]);
        assert_eq!(gate.due(SERVER, at(start, 61), rules()), []);
        gate.host_in_game(&[1, 2]);
        assert_eq!(
            gate.due(SERVER, at(start, 62), rules()),
            [note(2, false, all_lacking())]
        );
        gate.host_in_game(&[2]);
        assert_eq!(
            gate.due(SERVER, at(start, 200), rules()),
            [],
            "the host left its slot"
        );
    }

    #[test]
    fn the_host_s_machine_is_never_kicked() {
        let machines = [(1, 10), (2, 20), (3, 10)];
        assert_eq!(machine_to_kick(&machines, 2, Some(1), None), Ok(20));
        assert_eq!(machine_to_kick(&machines, 2, None, None), Ok(20));
        assert!(machine_to_kick(&machines, 1, Some(1), None).is_err());
        assert!(
            machine_to_kick(&machines, 3, Some(1), None).is_err(),
            "a guest on the host's machine"
        );
        assert!(machine_to_kick(&machines, 4, Some(1), None).is_err());
    }

    #[test]
    fn a_player_without_an_account_is_kicked_only_when_no_other_slot_lacks_one() {
        assert_eq!(machine_to_kick(&[(0, 30), (2, 20)], 0, None, None), Ok(30));
        assert!(machine_to_kick(&[(0, 30), (2, 20), (0, 40)], 0, None, None).is_err());
    }

    #[test]
    fn the_session_s_host_machine_is_never_kicked_before_the_host_said_hello() {
        let machines = [(1, 10), (2, 20)];
        assert!(machine_to_kick(&machines, 1, None, Some(10)).is_err());
        assert!(machine_to_kick(&machines, 2, Some(9), Some(20)).is_err());
        assert_eq!(machine_to_kick(&machines, 2, None, Some(10)), Ok(20));
    }

    #[test]
    fn a_session_that_does_not_host_yet_refuses_no_kick() {
        let machines = [(2, 0)];
        assert_eq!(own_machine(0), None);
        assert_eq!(own_machine(7), Some(7));
        assert_eq!(machine_to_kick(&machines, 2, None, own_machine(0)), Ok(0));
    }

    #[test]
    fn the_first_host_of_a_hosted_game_stays_its_host() {
        let mut gate = Gate::hosted();
        let start = Instant::now();
        let host = Hello {
            nonce: 5,
            ..said("")
        };
        assert_eq!(gate.host_hello(1, &host), (true, None));
        assert_eq!(gate.host_hello(2, &host), (false, None));
        assert_eq!(gate.host_hello(1, &host), (true, None));
        gate.host_in_game(&[1, 2]);
        gate.joined(2, SERVER, start, rules());
        assert_eq!(gate.joined(1, SERVER, start, rules()), []);
        assert_eq!(
            gate.due(SERVER, at(start, 30), rules()),
            [note(2, false, all_lacking())]
        );
    }

    #[test]
    fn a_hosted_game_acts_once_it_knows_its_host_and_never_on_the_host() {
        let mut gate = Gate::hosted();
        let start = Instant::now();
        gate.joined(1, SERVER, start, rules());
        gate.joined(2, SERVER, start, rules());
        assert_eq!(gate.due(SERVER, at(start, 60), rules()), [], "no host yet");
        let host = Hello {
            nonce: 5,
            ..said("")
        };
        assert_eq!(gate.host_hello(1, &host), (true, None));
        gate.host_in_game(&[1, 2]);
        assert_eq!(
            gate.due(SERVER, at(start, 61), rules()),
            [note(2, false, all_lacking())]
        );
        assert_eq!(gate.joined(1, SERVER, at(start, 62), rules()), []);
        assert_eq!(
            gate.due(SERVER, at(start, 200), rules()),
            [Step::Kick { key: 2 }]
        );
    }

    #[test]
    fn needs_name_the_server_s_mods_the_game_lacks_or_runs_otherwise() {
        assert_eq!(
            needs(SERVER, Some(&said("zones@0.1.0,other@1.0.0"))),
            [need("you.trading-post", "0.3.0", None)]
        );
        assert_eq!(needs(SERVER, Some(&said(SERVER))), []);
        assert_eq!(needs(SERVER, None).len(), 2);
        assert_eq!(needs("", None), []);
        assert_eq!(
            needs(SERVER, Some(&said("not a list"))).len(),
            2,
            "an unreadable hello lacks everything"
        );
    }

    #[test]
    fn a_cut_hello_lacks_only_what_its_list_could_have_named() {
        let cut = Hello {
            mods: "area@1.0.0,you.trading-post@0.2.0".to_owned(),
            complete: false,
            nonce: 0,
            language: None,
        };
        let server = "area@1.0.0,camera@1.0.0,you.trading-post@0.3.0,zones@0.1.0";
        assert_eq!(
            needs(server, Some(&cut)),
            [
                need("camera", "1.0.0", None),
                need("you.trading-post", "0.3.0", Some("0.2.0"))
            ],
            "zones sorts after the cut"
        );
        let empty = Hello {
            mods: String::new(),
            complete: false,
            nonce: 0,
            language: None,
        };
        assert_eq!(needs(server, Some(&empty)).len(), 4);
    }
}
