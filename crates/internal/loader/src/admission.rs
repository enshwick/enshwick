//! The serving side of the join check, on a dedicated server and in a game a
//! client hosts. A server that runs join mods waits for each player who
//! enters the world to say hello with the join mods the player's game runs.
//! A game that says nothing in time, such as one without Enshwick, or one
//! that lacks a join mod or runs it in another version, gets a chat line
//! naming what it needs and where to get it, then the game's own kick;
//! administrators too. A game whose hello matches stays, also when the hello
//! comes after the line. The check is about compatibility, not cheating: a
//! game that lies about its mods passes. A hosted game never checks its
//! host, and checks no one until the host's own hello names it and only
//! while the host's key holds a player slot in the game.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use crate::join::Names;
use crate::texts;

/// The table id of the server's session, an offset in the server object.
pub const SESSION: &str = "server.session";
/// The table id of the game's kick of one machine from its session.
pub const KICK: &str = "session.kick";
/// The table id of the session's own machine, an offset in the session: the
/// machine the game itself never kicks, the host's in a hosted game.
pub const HOST_MACHINE: &str = "session.host_machine";
/// The build table ids this part of the loader names.
pub const TABLE_IDS: &[&str] = &[SESSION, KICK, HOST_MACHINE];

/// Players the gate follows at most; the game has 16 slots.
const MAX_PLAYERS: usize = 64;
/// Mods a note names one per line; the rest are counted. With the opening,
/// the loader's line, the count and the kick, a note stays within the 16
/// lines a reply may take.
const MAX_NAMED: usize = 12;
/// How long a kick that did not remove the player waits to be tried again.
const KICK_AGAIN: Duration = Duration::from_secs(10);
/// Where Enshwick itself comes from.
const LOADER_PLACE: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

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
    players: HashMap<u64, Standing>,
    /// A game a client hosts: no one is checked until the host is known.
    hosted: bool,
    /// The host's account key, once its own hello came; the first host's
    /// stays.
    host: Option<u64>,
    /// Whether the host's key holds a player slot in the game.
    host_in_game: bool,
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
    }

    /// Forgets every player whose key holds no live slot in `live`.
    pub fn keep(&mut self, live: &[u64]) {
        self.players.retain(|key, _| live.contains(key));
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
        !self.players.is_empty()
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
fn own_machine(raw: u32) -> Option<u32> {
    (raw != 0).then_some(raw)
}

/// The machine to kick for the player `key` among the live slots'
/// `machines`, `(key, machine)`: never the host's, nor one the host's slot
/// shares, nor the session's own `host_machine`; or why not.
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
    let machine = machine_of(key).ok_or("the player holds no slot")?;
    if host.and_then(machine_of) == Some(machine) {
        return Err("the player's machine is the host's");
    }
    if host_machine == Some(machine) {
        return Err("the player's machine is the session's own");
    }
    Ok(machine)
}

/// The longest page a server may advertise, in bytes.
const MAX_PAGE: usize = 127;

/// Where players get a mod whose homepage is `page`: the page itself when it
/// is an `https://` or `http://` address of printable characters without
/// spaces, as a peer may send anything.
#[must_use]
pub fn place(page: &str) -> Option<String> {
    let rest = page
        .strip_prefix("https://")
        .or_else(|| page.strip_prefix("http://"))?;
    let plain =
        !rest.is_empty() && page.len() <= MAX_PAGE && page.chars().all(|c| c.is_ascii_graphic());
    plain.then(|| page.to_owned())
}

/// The note for a player whose game lacks `needs`: `hello` when the game
/// said which mods it runs, mods named by `names` with the homepages `pages`
/// by id, and the time to the kick, `leaving`, when the server kicks.
#[must_use]
pub fn note(
    needs: &[Need],
    hello: bool,
    names: &Names,
    pages: &BTreeMap<String, String>,
    leaving: Option<Duration>,
) -> String {
    let mut lines = Vec::with_capacity(MAX_NAMED + 4);
    if hello {
        lines.push(texts::fill("join.refused.mods", &[]));
    } else {
        lines.push(texts::fill("join.refused.vanilla", &[]));
        lines.push(texts::fill(
            "join.refused.loader",
            &[("place", LOADER_PLACE)],
        ));
    }
    let nowhere = texts::fill("join.refused.nowhere", &[]);
    for need in needs.iter().take(MAX_NAMED) {
        let name = enshwick::mods::label(names.get(&need.id).map_or("", String::as_str), &need.id);
        let place = pages
            .get(&need.id)
            .and_then(|line| place(line))
            .unwrap_or_else(|| nowhere.clone());
        let mut args = vec![
            ("mod", name.as_str()),
            ("version", need.version.as_str()),
            ("place", place.as_str()),
        ];
        let line = match &need.yours {
            Some(yours) => {
                args.push(("yours", yours.as_str()));
                texts::fill("join.refused.mod.yours", &args)
            }
            None => texts::fill("join.refused.mod", &args),
        };
        lines.push(line);
    }
    if needs.len() > MAX_NAMED {
        let count = (needs.len() - MAX_NAMED).to_string();
        lines.push(texts::fill("join.refused.more", &[("count", &count)]));
    }
    if let Some(leaving) = leaving {
        let seconds = leaving.as_secs().to_string();
        lines.push(texts::fill(
            "join.refused.leaving",
            &[("seconds", &seconds)],
        ));
    }
    lines.join("\n")
}

#[cfg(windows)]
pub(crate) use runtime::{after_step, configure, forget, hello, joined, left, nonce, start};

#[cfg(windows)]
mod runtime {
    use std::collections::BTreeMap;
    use std::hash::BuildHasher as _;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock, PoisonError};
    use std::time::{Duration, Instant};

    use enshwick::abi::Side;

    use super::{Gate, HOST_MACHINE, Hello, KICK, Rules, SESSION, Step};
    use crate::fuse::Fuse;
    use crate::host::Loader;
    use crate::join::Names;

    /// The game's kick of one machine from its session: the session, the
    /// machine's handle, and whether to ban it too. The server's own
    /// account actions call it on the game thread, on dedicated servers
    /// (builds 387, 399) and in the server clients host (builds 410, 426).
    type KickFn = unsafe extern "system" fn(session: *mut u8, machine: u32, ban: bool);

    /// How often the step looks at the players.
    const LOOK_EVERY: Duration = Duration::from_secs(1);

    /// The game's kick and where the server keeps its session.
    #[derive(Debug)]
    struct Kick {
        call: KickFn,
        session: usize,
        /// The session's own machine, an offset in the session.
        host_machine: Option<usize>,
    }

    /// What a serving game knows from its start.
    #[derive(Debug)]
    struct Serving {
        rules: Rules,
        names: Names,
        pages: BTreeMap<String, String>,
        kick: Option<Kick>,
        side: Side,
    }

    /// The admission's state.
    #[derive(Debug)]
    struct State {
        /// The rules, when the settings require Enshwick.
        rules: OnceLock<Rules>,
        /// Set at start when the settings require Enshwick.
        serving: OnceLock<Serving>,
        gate: Mutex<Option<Gate>>,
        /// Set while the gate follows anyone, so an idle step takes no lock.
        busy: AtomicBool,
        /// When the step looks next, in milliseconds since `epoch`.
        next: AtomicU64,
        epoch: OnceLock<Instant>,
        /// This process's number in its hellos to the game it hosts.
        nonce: OnceLock<u64>,
        /// Set once a player without an account key entered the world.
        unkeyed: AtomicBool,
        /// Set once the host's hello came under a second key.
        rehosted: AtomicBool,
        fuse: Fuse,
    }

    static STATE: State = State {
        rules: OnceLock::new(),
        serving: OnceLock::new(),
        gate: Mutex::new(None),
        busy: AtomicBool::new(false),
        next: AtomicU64::new(0),
        epoch: OnceLock::new(),
        nonce: OnceLock::new(),
        unkeyed: AtomicBool::new(false),
        rehosted: AtomicBool::new(false),
        fuse: Fuse::new("join admission"),
    };

    /// Takes the loader settings' rules, before the loader starts.
    pub(crate) fn configure(settings: &crate::settings::Settings) {
        if settings.require_enshwick {
            let _first = STATE.rules.set(Rules {
                wait: Duration::from_secs(settings.join_hello_seconds.into()),
                grace: Duration::from_secs(settings.join_note_seconds.into()),
            });
        }
    }

    /// Starts admitting players, naming mods by `names` with the homepages
    /// `pages` by id.
    pub(crate) fn start(loader: &Loader, names: Names, pages: BTreeMap<String, String>) {
        let Some(&rules) = STATE.rules.get() else {
            crate::log!(
                INFO,
                "join check off: players without the join mods are let in (require_enshwick = false)"
            );
            return;
        };
        let kick = loader
            .address(KICK)
            .and_then(|address| usize::try_from(address).ok())
            .zip(loader.offset(SESSION))
            .map(|(address, session)| Kick {
                // SAFETY: the table's kick is the game's kick, with this
                // signature, on this build.
                call: unsafe { core::mem::transmute::<usize, KickFn>(address) },
                session,
                host_machine: loader.offset(HOST_MACHINE),
            });
        match &kick {
            None => crate::log!(
                WARN,
                "join check kicks off: not in this build; players without the join mods get the note and stay"
            ),
            Some(Kick {
                host_machine: None, ..
            }) if loader.side == Side::CLIENT => crate::log!(
                WARN,
                "join check: the session's own machine is not in this build; the host is known by its hello alone"
            ),
            Some(_) => {}
        }
        crate::log!(
            INFO,
            "join check: a player in the world has {} s to say hello, then {} s to read the note",
            rules.wait.as_secs(),
            rules.grace.as_secs()
        );
        let _first = STATE.serving.set(Serving {
            rules,
            names,
            pages,
            kick,
            side: loader.side,
        });
    }

    /// This process's number for its hellos to the game it hosts.
    pub(crate) fn nonce() -> u64 {
        *STATE.nonce.get_or_init(|| {
            std::collections::hash_map::RandomState::new()
                .hash_one(0u8)
                .max(1)
        })
    }

    /// The server's join mods now, `id@version,...`.
    fn server_mods() -> String {
        crate::advertise::mod_list(&crate::host::join_mods())
    }

    /// Runs `f` on the gate, made for `serving`'s side, and notes whether it
    /// follows anyone.
    fn with_gate<R>(serving: &Serving, f: impl FnOnce(&mut Gate) -> R) -> R {
        let mut guard = STATE.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let gate = guard.get_or_insert_with(|| {
            if serving.side == Side::CLIENT {
                Gate::hosted()
            } else {
                Gate::default()
            }
        });
        let result = f(gate);
        STATE.busy.store(gate.busy(), Ordering::Release);
        result
    }

    /// Forgets every player, when a hosted game ends.
    pub(crate) fn forget() {
        *STATE.gate.lock().unwrap_or_else(PoisonError::into_inner) = None;
        STATE.busy.store(false, Ordering::Release);
    }

    /// The player `key` entered the world; on the game thread.
    pub(crate) fn joined(key: u64) {
        let Some(serving) = STATE.serving.get() else {
            return;
        };
        if key == 0 {
            if !STATE.unkeyed.swap(true, Ordering::Relaxed) {
                crate::log!(
                    WARN,
                    "join check: a player entered the world without an account key and is not checked"
                );
            }
            return;
        }
        let _ran = STATE.fuse.run("a player's join", || {
            let server = server_mods();
            let steps = with_gate(serving, |gate| {
                gate.make_room(key, crate::world::live_keys);
                gate.joined(key, &server, Instant::now(), serving.rules)
            });
            for step in steps {
                run(crate::host::loader(), serving, step);
            }
        });
    }

    /// Forgets the player `key`, who left.
    pub(crate) fn left(key: u64) {
        let Some(serving) = STATE.serving.get() else {
            return;
        };
        let _ran = STATE.fuse.run("a player's leave", || {
            with_gate(serving, |gate| gate.left(key));
        });
    }

    /// Takes the `hello` of the player `key`; whether the server answers it.
    /// On the game thread.
    pub(crate) fn hello(key: u64, hello: &Hello) -> bool {
        if key == 0 {
            return false;
        }
        let Some(serving) = STATE.serving.get() else {
            return true;
        };
        STATE
            .fuse
            .run("a hello", || {
                let own = serving.side == Side::CLIENT && hello.nonce == nonce();
                if own {
                    let (answered, _) = with_gate(serving, |gate| gate.host_hello(key, hello));
                    if answered {
                        crate::log!(INFO, "join check: the host is {key:016x}");
                    } else if !STATE.rehosted.swap(true, Ordering::Relaxed) {
                        crate::log!(
                            WARN,
                            "join check: the host's hello came under {key:016x}, not the host's first key; not answered"
                        );
                    }
                    return answered;
                }
                let server = server_mods();
                let (stays, step) = with_gate(serving, |gate| {
                    gate.make_room(key, crate::world::live_keys);
                    gate.hello(key, &server, hello, Instant::now(), serving.rules)
                });
                if let Some(step) = step {
                    run(crate::host::loader(), serving, step);
                }
                stays
            })
            .unwrap_or(true)
    }

    /// Forgets the players whose slot is gone, tells and kicks those whose
    /// time ran out; on the game thread, after the step.
    pub(crate) fn after_step(loader: &Loader) {
        if !STATE.busy.load(Ordering::Acquire) {
            return;
        }
        let Some(serving) = STATE.serving.get() else {
            return;
        };
        let epoch = *STATE.epoch.get_or_init(Instant::now);
        let now = Instant::now();
        let ms = u64::try_from(now.duration_since(epoch).as_millis()).unwrap_or(u64::MAX);
        if ms < STATE.next.load(Ordering::Relaxed) {
            return;
        }
        let next = u64::try_from(LOOK_EVERY.as_millis()).unwrap_or(u64::MAX);
        STATE.next.store(ms.saturating_add(next), Ordering::Relaxed);
        let _ran = STATE.fuse.run("the join admission step", || {
            let Ok(machines) = crate::world::machines(loader) else {
                return;
            };
            let live: Vec<u64> = machines.iter().map(|&(key, _)| key).collect();
            crate::net::keep_greeting(&live);
            let in_game: Vec<u64> = if serving.side == Side::CLIENT {
                crate::world::players(loader)
                    .map(|players| players.iter().map(|p| p.key).collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let server = server_mods();
            let steps = with_gate(serving, |gate| {
                gate.keep(&live);
                gate.host_in_game(&in_game);
                gate.due(&server, now, serving.rules)
            });
            for step in steps {
                run(Some(loader), serving, step);
            }
        });
    }

    /// Does one step; a kick needs the `loader`.
    fn run(loader: Option<&Loader>, serving: &Serving, step: Step) {
        match step {
            Step::Note { key, hello, needs } => {
                let leaving = serving.kick.as_ref().map(|_| serving.rules.grace);
                let text = super::note(&needs, hello, &serving.names, &serving.pages, leaving);
                let ids: Vec<&str> = needs.iter().map(|n| n.id.as_str()).collect();
                let why = if hello {
                    "runs other join mods"
                } else {
                    "said no hello"
                };
                crate::log!(
                    INFO,
                    "join check: the game of {key:016x} {why}, lacking {}; told",
                    ids.join(", ")
                );
                crate::chat::send(key, &text);
            }
            Step::Stay { key } => {
                crate::log!(
                    INFO,
                    "join check: the game of {key:016x} said a matching hello late; it stays"
                );
                crate::chat::send(key, &crate::texts::fill("join.admitted", &[]));
            }
            Step::Kick { key } => {
                let (Some(loader), Some(kick)) = (loader, &serving.kick) else {
                    return;
                };
                let host = with_gate(serving, |gate| gate.host);
                match disconnect(loader, kick, key, host) {
                    Ok(machine) => crate::log!(
                        INFO,
                        "join check: kicked the game of {key:016x} (machine {machine})"
                    ),
                    Err(why) => crate::log!(
                        WARN,
                        "join check: could not kick the game of {key:016x}: {why}; trying again"
                    ),
                }
            }
        }
    }

    /// Kicks the player `key` through the game, on the game thread, never
    /// on the machine of `host`, a hosted game's host, nor on the session's
    /// own machine; the player's machine handle, or why not.
    fn disconnect(
        loader: &Loader,
        kick: &Kick,
        key: u64,
        host: Option<u64>,
    ) -> Result<u32, &'static str> {
        let server = crate::world::reader().map_err(|_| "not on the game thread")?;
        let machines =
            crate::world::machines(loader).map_err(|_| "the player slots do not read")?;
        // SAFETY: the offset lies inside the server object, as the pattern
        // that pins it reads it from the server in the game's own kick; the
        // server is read on its own thread.
        let session: usize = unsafe { crate::world::read(server, kick.session) };
        if session == 0 {
            return Err("the server has no session");
        }
        let host_machine = kick.host_machine.map(|offset| {
            // SAFETY: a `u32` machine handle inside the live session, as the
            // pattern that pins the offset finds the game's own kick compare
            // it with the machine to kick; read on the game thread.
            unsafe { crate::world::read::<u32>(session, offset) }
        });
        let host_machine = host_machine.and_then(super::own_machine);
        let machine = super::machine_to_kick(&machines, key, host, host_machine)?;
        // SAFETY: the game's kick with the server's session and the machine
        // of one of its player slots, on the game thread, as the server's
        // own account actions call it.
        unsafe {
            (kick.call)(
                core::ptr::with_exposed_provenance_mut(session),
                machine,
                false,
            );
        };
        Ok(machine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: &str = "safes@0.3.0,zones@0.1.0";

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
        }
    }

    fn note(key: u64, hello: bool, needs: Vec<Need>) -> Step {
        Step::Note { key, hello, needs }
    }

    fn all_lacking() -> Vec<Need> {
        vec![need("safes", "0.3.0", None), need("zones", "0.1.0", None)]
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
        let (stays, step) = gate.hello(7, SERVER, &said("safes@0.2.0"), start, rules());
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
                    need("safes", "0.3.0", Some("0.2.0")),
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
        gate.hello(8, SERVER, &said("safes@0.2.0"), start, rules());
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
        let client = said("camera@1.0.0,safes@0.3.0,zones@0.1.0");
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
        gate.hello(7, "safes@0.3.0", &said("safes@0.3.0"), start, rules());
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
        let (stays, step) = gate.hello(7, SERVER, &said("safes@0.2.0"), start, rules());
        assert!(!stays);
        assert_eq!(
            step,
            Some(note(
                7,
                true,
                vec![
                    need("safes", "0.3.0", Some("0.2.0")),
                    need("zones", "0.1.0", None)
                ]
            ))
        );
        assert_eq!(
            gate.hello(7, SERVER, &said("safes@0.2.0"), start, rules()),
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
            [need("safes", "0.3.0", None)]
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
            mods: "area@1.0.0,safes@0.2.0".to_owned(),
            complete: false,
            nonce: 0,
        };
        let server = "area@1.0.0,camera@1.0.0,safes@0.3.0,zones@0.1.0";
        assert_eq!(
            needs(server, Some(&cut)),
            [
                need("camera", "1.0.0", None),
                need("safes", "0.3.0", Some("0.2.0"))
            ],
            "zones sorts after the cut"
        );
        let empty = Hello {
            mods: String::new(),
            complete: false,
            nonce: 0,
        };
        assert_eq!(needs(server, Some(&empty)).len(), 4);
    }

    #[test]
    fn places_are_links_players_can_open() {
        assert_eq!(
            place("https://github.com/someone/safes").as_deref(),
            Some("https://github.com/someone/safes")
        );
        assert!(place("http://example.com/safes").is_some());
        for bad in [
            "",
            "https://",
            "ftp://example.com",
            "github.com/someone/safes",
            "https://exa mple.com",
            "https://example.com/\nInstall",
        ] {
            assert_eq!(place(bad), None, "{bad}");
        }
        assert_eq!(place(&format!("https://{}", "x".repeat(200))), None);
    }

    #[test]
    fn a_note_names_enshwick_each_mod_its_version_and_where_to_get_it() {
        let names = Names::from([("safes".to_owned(), "Safes".to_owned())]);
        let pages = BTreeMap::from([(
            "safes".to_owned(),
            "https://github.com/someone/safes".to_owned(),
        )]);
        let needs = [
            need("safes", "0.3.0", Some("0.2.0")),
            need("zones", "0.1.0", None),
        ];
        let text = super::note(&needs, false, &names, &pages, Some(Duration::from_secs(10)));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5, "{text}");
        assert!(lines[1].contains(LOADER_PLACE), "{text}");
        assert!(
            lines[2].contains("Safes 0.3.0")
                && !lines[2].contains("(safes)")
                && lines[2].contains("0.2.0")
                && lines[2].contains("https://github.com/someone/safes"),
            "{text}"
        );
        assert!(
            lines[3].contains("zones") && lines[3].contains("0.1.0"),
            "{text}"
        );
        assert!(lines[4].contains("10"), "{text}");
        let told = super::note(&needs, true, &names, &pages, None);
        assert!(!told.contains(LOADER_PLACE), "{told}");
        assert_eq!(told.lines().count(), 3, "no kick line: {told}");
    }

    #[test]
    fn a_long_note_counts_the_mods_it_does_not_name() {
        let needs: Vec<Need> = (0..40)
            .map(|i| need(&format!("mod-{i:02}"), "1.0.0", None))
            .collect();
        let text = super::note(
            &needs,
            false,
            &Names::new(),
            &BTreeMap::new(),
            Some(Duration::from_secs(10)),
        );
        assert!(text.lines().count() <= 16, "{text}");
        assert!(text.contains("28"), "{text}");
    }
}
