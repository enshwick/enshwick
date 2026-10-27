//! The gate at work on a server: hellos, kicks and notes in the game.

use std::collections::BTreeMap;
use std::hash::BuildHasher as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick::abi::Side;

use crate::fuse::Fuse;
use crate::host::Loader;
use crate::join::Names;
use crate::locks::Relock;
use crate::pinned;

use super::{Gate, Hello, Rules, Step};

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
    side: Side,
}

/// The admission's state.
#[derive(Debug)]
struct State {
    /// The rules, when the settings require Enshwick.
    rules: OnceLock<Rules>,
    /// Set at start when the settings require Enshwick.
    serving: OnceLock<Serving>,
    /// The game's kick, set at start whatever the settings: it also
    /// removes accounts banned while not online.
    kick: OnceLock<Option<Kick>>,
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
    kick: OnceLock::new(),
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
    let kick = STATE.kick.get_or_init(|| {
        loader
            .address(pinned::session::KICK)
            .and_then(|address| usize::try_from(address).ok())
            .zip(loader.offset(pinned::server::SESSION))
            .map(|(address, session)| Kick {
                // SAFETY: the table's `session.kick` address is the game's kick on this
                // build, whose signature `KickFn` states.
                call: unsafe { core::mem::transmute::<usize, KickFn>(address) },
                session,
                host_machine: loader.offset(pinned::session::HOST_MACHINE),
            })
    });
    let Some(&rules) = STATE.rules.get() else {
        crate::log!(
            INFO,
            "join check off: players without the join mods are let in (require_enshwick = false)"
        );
        return;
    };
    match kick {
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
    let mut guard = STATE.gate.relock();
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
    crate::console::forget_offline_bans();
    *STATE.gate.relock() = None;
    STATE.busy.store(false, Ordering::Release);
}

/// The player `key` entered the world; on the game thread.
pub(crate) fn joined(key: u64) {
    if key != 0
        && crate::console::banned_offline(key)
        && STATE
            .fuse
            .run("a banned account's join", || refuse_banned(key))
            .unwrap_or(false)
    {
        return;
    }
    if key != 0
        && STATE
            .fuse
            .run("the allowlist's check", || refuse_unlisted(key))
            .unwrap_or(false)
    {
        return;
    }
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

/// Removes the player `key` from a dedicated server whose allowlist is on
/// and lists neither the account nor an administrator; a game a client
/// hosts keeps its own invitations. A player whose slot does not read
/// counts as no administrator. Whether the player was removed.
fn refuse_unlisted(key: u64) -> bool {
    let Some(loader) = crate::host::loader().filter(|loader| loader.side == Side::SERVER) else {
        return false;
    };
    let host = STATE
        .serving
        .get()
        .and_then(|serving| with_gate(serving, |gate| gate.host));
    if host == Some(key) {
        return false;
    }
    let admin = crate::world::players(loader).is_ok_and(|players| {
        players
            .iter()
            .any(|p| p.key == key && crate::console::player_is_admin(p))
    });
    if !crate::allowlist::refuses(loader, key, admin) {
        return false;
    }
    crate::chat::send(key, &crate::texts::fill("allowlist.refused", &[]));
    let Some(kick) = STATE.kick.get().and_then(Option::as_ref) else {
        crate::log!(
            WARN,
            "allowlist: {key:016x} is not listed but joined: this build lacks the session's kick"
        );
        return false;
    };
    match disconnect(loader, kick, key, host, false) {
        Ok(machine) => {
            crate::log!(
                INFO,
                "allowlist: {key:016x} is not listed: removed (machine {machine})"
            );
            true
        }
        Err(why) => {
            crate::log!(
                WARN,
                "allowlist: {key:016x} is not listed but could not be removed: {why}"
            );
            false
        }
    }
}

/// Removes and bans the player `key`, whose account went onto the ban list
/// while not online: the game checks joins only against its session's own
/// list, filled from the ban list when hosting starts. The session's kick
/// with its ban adds the account there for the rest of the run, also for a
/// player who joined with an administrator's password. Whether the player
/// was removed; an account the game's own unban took off the list since is
/// forgotten and stays.
fn refuse_banned(key: u64) -> bool {
    let Some(loader) = crate::host::loader() else {
        return false;
    };
    if !crate::console::still_banned(loader, key) {
        crate::console::forget_offline_ban(key);
        return false;
    }
    let Some(kick) = STATE.kick.get().and_then(Option::as_ref) else {
        crate::log!(
            WARN,
            "{key:016x} is on the ban list but joined: this build lacks the session's kick"
        );
        return false;
    };
    let host = STATE
        .serving
        .get()
        .and_then(|serving| with_gate(serving, |gate| gate.host));
    match disconnect(loader, kick, key, host, true) {
        Ok(machine) => {
            crate::log!(
                INFO,
                "{key:016x} is on the ban list: removed and banned (machine {machine})"
            );
            true
        }
        Err(why) => {
            crate::log!(
                WARN,
                "{key:016x} is on the ban list but could not be removed: {why}"
            );
            false
        }
    }
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
            let leaving = STATE
                .kick
                .get()
                .and_then(Option::as_ref)
                .map(|_| serving.rules.grace);
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
            let (Some(loader), Some(kick)) = (loader, STATE.kick.get().and_then(Option::as_ref))
            else {
                return;
            };
            let host = with_gate(serving, |gate| gate.host);
            match disconnect(loader, kick, key, host, false) {
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

/// Kicks the player `key` through the game, banning it from the session
/// when `ban`, on the game thread, never
/// on the machine of `host`, a hosted game's host, nor on the session's
/// own machine; the player's machine handle, or why not.
fn disconnect(
    loader: &Loader,
    kick: &Kick,
    key: u64,
    host: Option<u64>,
    ban: bool,
) -> Result<u32, &'static str> {
    let server = crate::world::reader().map_err(|_| "not on the game thread")?;
    let machines = crate::world::machines(loader).map_err(|_| "the player slots do not read")?;
    // SAFETY: the session pointer lies at `kick.session` inside the server
    // object, as the table's pattern for that offset reads it from the server
    // in the game's own kick; `world::reader` succeeded, so this runs on the
    // game thread, which alone writes the server.
    let session = unsafe { crate::memory::read_ptr::<u8>(server, kick.session) };
    if session.is_null() {
        return Err("the server has no session");
    }
    let host_machine = kick.host_machine.map(|offset| {
        // SAFETY: `session` is non-null, checked just before, and holds a
        // `u32` machine handle at `offset`, as the table's pattern for it finds
        // the game's own kick compare it with the machine to kick; read on
        // the game thread, which alone writes the session.
        unsafe { crate::memory::read::<u32>(session.expose_provenance(), offset) }
    });
    let host_machine = host_machine.and_then(super::own_machine);
    let machine = super::machine_to_kick(&machines, key, host, host_machine)?;
    // SAFETY: `kick.call` is the table's kick, typed `KickFn` on this build,
    // called with the server's non-null session and the machine of one of
    // its player slots, on the game thread, as the server's own account
    // actions call it.
    unsafe { (kick.call)(session, machine, ban) };
    Ok(machine)
}
