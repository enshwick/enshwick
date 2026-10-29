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
/// the game has 16 slots. The gate's refused players may pass it for a
/// moment; see [`Gate::guarded`].
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
    /// Players without an account share key 0, so a refusal of key 0 stays
    /// until no live slot lacks an account, as [`Self::keep`] finds.
    pub fn left(&mut self, key: u64) {
        self.players.remove(&key);
        if key != 0 {
            self.refused.remove(&key);
        }
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
    /// guards now let in is forgotten; a broken guard counts as refusing,
    /// tried again only when the build `kicks`. A full list first forgets
    /// the players whose key holds no live slot in `live`, read only then,
    /// and follows the newcomer even past [`MAX_PLAYERS`]: the step forgets
    /// the gone slots each second. Whether the player is newly refused with
    /// no kick, for its one error.
    pub fn guarded(
        &mut self,
        key: u64,
        guard: Guard,
        kicks: bool,
        now: Instant,
        live: impl FnOnce() -> Option<Vec<u64>>,
    ) -> bool {
        let next = match guard {
            Guard::Passed => {
                self.refused.remove(&key);
                return false;
            }
            Guard::Unkickable => None,
            Guard::Broken if !kicks => None,
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
mod tests;
