//! The server's side of the join check. A dedicated server that runs join
//! mods waits for each joining player's game to say hello with the join mods
//! it runs. A game that says nothing in time, such as one without Enshwick,
//! or one that lacks a join mod or runs it in another version, gets a chat
//! line naming what it needs and where to get it, then the game's own kick.
//! A game whose hello matches stays, also when the hello comes after the
//! line.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use enshwick_loader_api::mods::Updates;

use crate::join::Names;
use crate::texts;

/// The build table ids this part of the loader names.
pub const TABLE_IDS: &[&str] = &["server.session", "session.kick"];

/// Players the gate follows at most; the game has 16 slots.
const MAX_PLAYERS: usize = 64;
/// Mods a note names one per line; the rest are counted. With the opening,
/// the loader's line, the count and the kick, a note stays within the 16
/// lines a reply may take.
const MAX_NAMED: usize = 12;
/// Where Enshwick itself comes from.
const LOADER_PLACE: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

/// How long the server waits on a joining player's game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// From the join to the note, for a game that says no hello.
    pub wait: Duration,
    /// From the note to the kick, for the player to read it.
    pub grace: Duration,
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
    /// Tells the player what the game lacks; `hello` when the game said
    /// which mods it runs.
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

/// Where one player stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Standing {
    /// Joined at this moment; no hello yet.
    Waiting(Instant),
    /// Said a hello that matched.
    Admitted,
    /// Noted; kicked at this moment.
    Refused(Instant),
    /// Kicked; gone once the game removes the player.
    Kicked,
}

/// The players the server follows from their join until their game said a
/// matching hello or they were kicked.
#[derive(Debug, Default)]
pub struct Gate {
    players: HashMap<u64, Standing>,
}

impl Gate {
    /// Starts the wait for the player `key`, who entered the world at `now`;
    /// a player whose game said hello already keeps where it stands.
    pub fn joined(&mut self, key: u64, now: Instant) {
        if self.players.len() < MAX_PLAYERS || self.players.contains_key(&key) {
            self.players.entry(key).or_insert(Standing::Waiting(now));
        }
    }

    /// Takes the hello of the player `key`, whose game runs `client`, on a
    /// server that runs `server` (both `id@version,...`); whether the player
    /// may stay, and what to tell the player.
    pub fn hello(
        &mut self,
        key: u64,
        server: &str,
        client: &str,
        now: Instant,
        rules: Rules,
    ) -> (bool, Option<Step>) {
        let needs = needs(server, Some(client));
        let before = self.players.get(&key).copied();
        if needs.is_empty() {
            if self.players.len() < MAX_PLAYERS || before.is_some() {
                self.players.insert(key, Standing::Admitted);
            }
            let stay = matches!(before, Some(Standing::Refused(_)));
            return (true, stay.then_some(Step::Stay { key }));
        }
        if matches!(before, Some(Standing::Refused(_) | Standing::Kicked)) {
            return (false, None);
        }
        if self.players.len() < MAX_PLAYERS || before.is_some() {
            self.players
                .insert(key, Standing::Refused(now + rules.grace));
        }
        (
            false,
            Some(Step::Note {
                key,
                hello: true,
                needs,
            }),
        )
    }

    /// Forgets the player `key`, who left.
    pub fn left(&mut self, key: u64) {
        self.players.remove(&key);
    }

    /// The steps due at `now` on a server that runs `server`.
    pub fn due(&mut self, server: &str, now: Instant, rules: Rules) -> Vec<Step> {
        let mut steps = Vec::new();
        for (&key, standing) in &mut self.players {
            match *standing {
                Standing::Waiting(since) if now.saturating_duration_since(since) >= rules.wait => {
                    let needs = needs(server, None);
                    if needs.is_empty() {
                        *standing = Standing::Admitted;
                        continue;
                    }
                    *standing = Standing::Refused(now + rules.grace);
                    steps.push(Step::Note {
                        key,
                        hello: false,
                        needs,
                    });
                }
                Standing::Refused(at) if now >= at => {
                    *standing = Standing::Kicked;
                    steps.push(Step::Kick { key });
                }
                _ => {}
            }
        }
        steps
    }

    /// Whether a player waits for a hello or a kick, so a step has work.
    #[must_use]
    pub fn watching(&self) -> bool {
        self.players
            .values()
            .any(|s| matches!(s, Standing::Waiting(_) | Standing::Refused(_)))
    }
}

/// The server's join mods, of `server`, that a game running `client` lacks
/// or runs in another version; every one for a game that said no hello.
#[must_use]
pub fn needs(server: &str, client: Option<&str>) -> Vec<Need> {
    let client = client.unwrap_or_default();
    let yours: BTreeMap<&str, &str> = crate::join::entries(client).collect();
    crate::join::wanted(server, client)
        .into_iter()
        .map(|(id, version)| Need {
            yours: yours.get(id.as_str()).map(|&v| v.to_owned()),
            id,
            version,
        })
        .collect()
}

/// Where players get a mod with the update source `line`, as a link.
#[must_use]
pub fn place(line: &str) -> Option<String> {
    match Updates::parse(line)? {
        Updates::GitHub { repository, .. } => {
            Some(format!("https://github.com/{repository}/releases"))
        }
        Updates::Url(url) => Some(url),
    }
}

/// The note for a player whose game lacks `needs`: `hello` when the game
/// said which mods it runs, mods named by `names` with the update sources
/// `sources` by id, and the time to the kick, `leaving`, when the server
/// kicks.
#[must_use]
pub fn note(
    needs: &[Need],
    hello: bool,
    names: &Names,
    sources: &BTreeMap<String, String>,
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
        let name = enshwick_loader_api::mods::label(
            names.get(&need.id).map_or("", String::as_str),
            &need.id,
        );
        let place = sources
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
    lines.join(
        "
",
    )
}

#[cfg(windows)]
pub(crate) use runtime::{after_step, configure, hello, joined, left, start};

#[cfg(windows)]
mod runtime {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock, PoisonError};
    use std::time::{Duration, Instant};

    use enshwick_loader_api::abi::Side;

    use super::{Gate, Rules, Step};
    use crate::host::Loader;
    use crate::join::Names;

    /// The game's kick of one machine from its session: the session, the
    /// machine's handle, and whether to ban it too. The server's own
    /// account actions call it on the game thread (server builds 387, 399).
    type KickFn = unsafe extern "system" fn(session: *mut u8, machine: u32, ban: bool);

    /// How often the step looks at the players who wait.
    const LOOK_EVERY: Duration = Duration::from_secs(1);

    /// The game's kick and where the server keeps its session.
    #[derive(Debug)]
    struct Kick {
        call: KickFn,
        session: usize,
    }

    /// What a dedicated server that admits players knows from its start.
    #[derive(Debug)]
    struct Serving {
        rules: Rules,
        names: Names,
        sources: BTreeMap<String, String>,
        kick: Option<Kick>,
    }

    /// The players who wait, and when the step looks at them next.
    #[derive(Debug)]
    struct Watch {
        gate: Gate,
        next: Instant,
    }

    /// The admission's state.
    #[derive(Debug)]
    struct State {
        /// The rules, when the settings require Enshwick.
        rules: OnceLock<Rules>,
        /// Set at start on a dedicated server whose settings require
        /// Enshwick.
        serving: OnceLock<Serving>,
        watch: Mutex<Option<Watch>>,
        /// Set while a player waits, so an idle step takes no lock.
        watching: AtomicBool,
    }

    static STATE: State = State {
        rules: OnceLock::new(),
        serving: OnceLock::new(),
        watch: Mutex::new(None),
        watching: AtomicBool::new(false),
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

    /// Starts admitting players on a dedicated server, naming mods by `names`
    /// with the update sources `sources` by id.
    pub(crate) fn start(loader: &Loader, names: Names, sources: BTreeMap<String, String>) {
        if loader.side != Side::SERVER {
            return;
        }
        let Some(&rules) = STATE.rules.get() else {
            crate::log!(
                INFO,
                "join check off: players without the server's join mods are let in (require_enshwick = false)"
            );
            return;
        };
        let kick = loader
            .address("session.kick")
            .and_then(|address| usize::try_from(address).ok())
            .zip(loader.offset("server.session"))
            .map(|(address, session)| Kick {
                // SAFETY: the table's `session.kick` is the game's kick, with
                // this signature, on this build.
                call: unsafe { core::mem::transmute::<usize, KickFn>(address) },
                session,
            });
        if kick.is_none() {
            crate::log!(
                WARN,
                "join check kicks off: not in this build; players without the server's join mods get the note and stay"
            );
        }
        crate::log!(
            INFO,
            "join check: a joining player's game has {} s to say hello, then {} s to read the note",
            rules.wait.as_secs(),
            rules.grace.as_secs()
        );
        let _first = STATE.serving.set(Serving {
            rules,
            names,
            sources,
            kick,
        });
    }

    /// The server's join mods now, `id@version,...`.
    fn server_mods() -> String {
        crate::advertise::mod_list(&crate::host::join_mods())
    }

    /// Runs `f` on the players who wait, and notes whether any still wait.
    fn with_gate<R>(f: impl FnOnce(&mut Watch) -> R) -> R {
        let mut guard = STATE.watch.lock().unwrap_or_else(PoisonError::into_inner);
        let watch = guard.get_or_insert_with(|| Watch {
            gate: Gate::default(),
            next: Instant::now(),
        });
        let result = f(watch);
        STATE
            .watching
            .store(watch.gate.watching(), Ordering::Release);
        result
    }

    /// Starts the wait for the player `key`, who entered the world.
    pub(crate) fn joined(key: u64) {
        if STATE.serving.get().is_some() {
            with_gate(|watch| watch.gate.joined(key, Instant::now()));
        }
    }

    /// Forgets the player `key`, who left.
    pub(crate) fn left(key: u64) {
        if STATE.serving.get().is_some() {
            with_gate(|watch| watch.gate.left(key));
        }
    }

    /// Takes the hello of the player `key`, whose game runs `mods`; whether
    /// the server answers it. On the game thread.
    pub(crate) fn hello(key: u64, mods: &str) -> bool {
        let Some(serving) = STATE.serving.get() else {
            return true;
        };
        let server = server_mods();
        let (stays, step) = with_gate(|watch| {
            watch
                .gate
                .hello(key, &server, mods, Instant::now(), serving.rules)
        });
        if let Some(step) = step {
            run(None, serving, step);
        }
        stays
    }

    /// Tells and kicks the players whose time ran out; on the game thread,
    /// after the step.
    pub(crate) fn after_step(loader: &Loader) {
        if !STATE.watching.load(Ordering::Acquire) {
            return;
        }
        let Some(serving) = STATE.serving.get() else {
            return;
        };
        let now = Instant::now();
        let steps = with_gate(|watch| {
            if now < watch.next {
                return Vec::new();
            }
            watch.next = now + LOOK_EVERY;
            watch.gate.due(&server_mods(), now, serving.rules)
        });
        for step in steps {
            run(Some(loader), serving, step);
        }
    }

    /// Does one step; a kick needs the `loader`.
    fn run(loader: Option<&Loader>, serving: &Serving, step: Step) {
        match step {
            Step::Note { key, hello, needs } => {
                let leaving = serving.kick.as_ref().map(|_| serving.rules.grace);
                let text = super::note(&needs, hello, &serving.names, &serving.sources, leaving);
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
                if let (Some(loader), Some(kick)) = (loader, &serving.kick) {
                    disconnect(loader, kick, key);
                }
            }
        }
    }

    /// Kicks the player `key` through the game; on the game thread.
    fn disconnect(loader: &Loader, kick: &Kick, key: u64) {
        let Ok(server) = crate::world::reader() else {
            return;
        };
        let Some(machine) = crate::world::machines(loader)
            .ok()
            .and_then(|machines| machines.into_iter().find(|&(k, _)| k == key))
            .map(|(_, machine)| machine)
        else {
            return;
        };
        // SAFETY: the server holds its session's pointer, read on its own
        // thread.
        let session: usize = unsafe { crate::world::read(server, kick.session) };
        if session == 0 {
            return;
        }
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
        crate::log!(
            INFO,
            "join check: disconnected the game of {key:016x} (machine {machine})"
        );
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

    #[test]
    fn a_game_that_never_says_hello_is_told_then_kicked() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, start);
        assert!(gate.watching());
        let early = start + Duration::from_secs(29);
        assert_eq!(gate.due(SERVER, early, rules()), []);
        let late = start + Duration::from_secs(30);
        assert_eq!(
            gate.due(SERVER, late, rules()),
            [Step::Note {
                key: 7,
                hello: false,
                needs: vec![need("safes", "0.3.0", None), need("zones", "0.1.0", None)],
            }]
        );
        assert_eq!(gate.due(SERVER, late, rules()), [], "one note");
        let kick = late + Duration::from_secs(10);
        assert_eq!(gate.due(SERVER, kick, rules()), [Step::Kick { key: 7 }]);
        assert_eq!(gate.due(SERVER, kick, rules()), [], "one kick");
        assert!(!gate.watching());
        gate.left(7);
        gate.joined(7, kick);
        assert!(gate.watching(), "a player who comes back waits again");
    }

    #[test]
    fn a_matching_hello_lets_the_player_stay_and_extra_mods_do_not_count() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, start);
        let client = "camera@1.0.0,safes@0.3.0,zones@0.1.0";
        assert_eq!(gate.hello(7, SERVER, client, start, rules()), (true, None));
        assert!(!gate.watching());
        let much_later = start + Duration::from_secs(3600);
        assert_eq!(gate.due(SERVER, much_later, rules()), []);
    }

    #[test]
    fn a_hello_before_the_join_is_kept() {
        let mut gate = Gate::default();
        let start = Instant::now();
        assert_eq!(gate.hello(7, SERVER, SERVER, start, rules()), (true, None));
        gate.joined(7, start);
        assert!(!gate.watching());
    }

    #[test]
    fn a_hello_that_lacks_a_mod_is_told_at_once_and_kicked_once() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, start);
        let (stays, step) = gate.hello(7, SERVER, "safes@0.2.0", start, rules());
        assert!(!stays);
        assert_eq!(
            step,
            Some(Step::Note {
                key: 7,
                hello: true,
                needs: vec![
                    need("safes", "0.3.0", Some("0.2.0")),
                    need("zones", "0.1.0", None)
                ],
            })
        );
        assert_eq!(
            gate.hello(7, SERVER, "safes@0.2.0", start, rules()),
            (false, None),
            "a repeated hello is not told again"
        );
        let kick = start + Duration::from_secs(10);
        assert_eq!(gate.due(SERVER, kick, rules()), [Step::Kick { key: 7 }]);
    }

    #[test]
    fn a_late_matching_hello_calls_the_kick_off() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, start);
        let late = start + Duration::from_secs(30);
        assert_eq!(gate.due(SERVER, late, rules()).len(), 1);
        assert_eq!(
            gate.hello(7, SERVER, SERVER, late, rules()),
            (true, Some(Step::Stay { key: 7 }))
        );
        let kick = late + Duration::from_secs(10);
        assert_eq!(gate.due(SERVER, kick, rules()), []);
    }

    #[test]
    fn a_server_without_join_mods_admits_everyone() {
        let mut gate = Gate::default();
        let start = Instant::now();
        gate.joined(7, start);
        let late = start + Duration::from_secs(60);
        assert_eq!(gate.due("", late, rules()), []);
        assert!(!gate.watching());
    }

    #[test]
    fn the_gate_follows_a_bounded_number_of_players() {
        let mut gate = Gate::default();
        let start = Instant::now();
        for key in 1..=1000 {
            gate.joined(key, start);
        }
        assert_eq!(gate.players.len(), MAX_PLAYERS);
    }

    #[test]
    fn needs_name_the_server_s_mods_the_game_lacks_or_runs_otherwise() {
        assert_eq!(
            needs(SERVER, Some("zones@0.1.0,other@1.0.0")),
            [need("safes", "0.3.0", None)]
        );
        assert_eq!(needs(SERVER, Some(SERVER)), []);
        assert_eq!(needs(SERVER, None).len(), 2);
        assert_eq!(needs("", None), []);
        assert_eq!(
            needs(SERVER, Some("not a list")).len(),
            2,
            "an unreadable hello lacks everything"
        );
    }

    #[test]
    fn places_are_links_players_can_open() {
        assert_eq!(
            place("github:themaiby/enshwick:enshwick-safes").as_deref(),
            Some("https://github.com/themaiby/enshwick/releases")
        );
        assert_eq!(
            place("url:https://example.com/latest.json").as_deref(),
            Some("https://example.com/latest.json")
        );
        assert_eq!(place("ftp:somewhere"), None);
    }

    #[test]
    fn a_note_names_enshwick_each_mod_its_version_and_where_to_get_it() {
        let names = Names::from([("safes".to_owned(), "Safes".to_owned())]);
        let sources =
            BTreeMap::from([("safes".to_owned(), "github:someone/safes:safes".to_owned())]);
        let needs = [
            need("safes", "0.3.0", Some("0.2.0")),
            need("zones", "0.1.0", None),
        ];
        let text = note(
            &needs,
            false,
            &names,
            &sources,
            Some(Duration::from_secs(10)),
        );
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5, "{text}");
        assert!(lines[1].contains(LOADER_PLACE), "{text}");
        assert!(
            lines[2].contains("Safes (safes)")
                && lines[2].contains("0.3.0")
                && lines[2].contains("0.2.0")
                && lines[2].contains("https://github.com/someone/safes/releases"),
            "{text}"
        );
        assert!(
            lines[3].contains("zones") && lines[3].contains("0.1.0"),
            "{text}"
        );
        assert!(lines[4].contains("10"), "{text}");
        let told = note(&needs, true, &names, &sources, None);
        assert!(!told.contains(LOADER_PLACE), "{told}");
        assert_eq!(told.lines().count(), 3, "no kick line: {told}");
    }

    #[test]
    fn a_long_note_counts_the_mods_it_does_not_name() {
        let needs: Vec<Need> = (0..40)
            .map(|i| need(&format!("mod-{i:02}"), "1.0.0", None))
            .collect();
        let text = note(
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
