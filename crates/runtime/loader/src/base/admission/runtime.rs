//! The gate at work on a server: hellos, kicks and notes in the game.

use std::hash::BuildHasher as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick_core::abi::Side;

use crate::base::join::Names;
use crate::engine::fuse::Fuse;
use crate::engine::pinned;
use crate::host::Loader;
use enshwick_base::Relock;

use super::{Gate, Hello, Rules, Step};

/// The game's kick of one machine from its session: the session, the
/// machine's handle, and whether to ban it too. The server's own
/// account actions call it on the game thread, on dedicated servers
/// (builds 387, 399) and in the server clients host (builds 410, 426).
type KickFn = unsafe extern "system" fn(session: *mut u8, machine: u32, ban: bool);

/// How often the step looks at the players.
const LOOK_EVERY: Duration = Duration::from_secs(1);
/// How long a joining player's game has to say which mods it runs before
/// the server takes it for a game without Enshwick.
const HELLO_WAIT: Duration = Duration::from_secs(30);
/// How long between the chat line and the disconnect, for the player to
/// read it.
const NOTE_GRACE: Duration = Duration::from_secs(10);

/// The game's kick and where the server keeps its session.
#[derive(Debug)]
struct Kick {
    call: KickFn,
    session: usize,
    /// The session's own machine, an offset in the session.
    host_machine: Option<usize>,
}

/// How long the server waits on a player who entered the world.
const RULES: Rules = Rules {
    wait: HELLO_WAIT,
    grace: NOTE_GRACE,
};

/// What a serving game knows from its start.
#[derive(Debug)]
struct Serving {
    names: Names,
    side: Side,
}

/// The admission's state.
#[derive(Debug)]
struct State {
    /// Set at start.
    serving: OnceLock<Serving>,
    /// The running world's `require_enshwick` rule, read as it loads.
    required: AtomicBool,
    /// The game's kick, set at start.
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
    serving: OnceLock::new(),
    required: AtomicBool::new(true),
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

/// Takes the loaded world's `require_enshwick` rule: with it off, players
/// without the world's join mods stay in that world. The players followed
/// so far are forgotten when it turns off.
pub(crate) fn require(on: bool) {
    if !on {
        *STATE.gate.relock() = None;
        STATE.busy.store(false, Ordering::Release);
        crate::log!(
            INFO,
            "join check off in this world: players without the join mods are let in (require_enshwick = false)"
        );
    }
    STATE.required.store(on, Ordering::Release);
}

/// Whether the running world checks joins.
fn required() -> bool {
    STATE.required.load(Ordering::Acquire)
}

/// Starts admitting players, naming mods by `names`.
pub(crate) fn start(loader: &Loader, names: Names) {
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
        "join check: a player in the world has {} s to say hello, then {} s to read the note, where the world requires Enshwick",
        RULES.wait.as_secs(),
        RULES.grace.as_secs()
    );
    let _first = STATE.serving.set(Serving {
        names,
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
    crate::base::advertise::mod_list(&crate::host::join_mods())
}

/// Runs `f` on the gate, made for the serving game's side, and notes
/// whether it follows anyone.
fn with_gate<R>(f: impl FnOnce(&mut Gate) -> R) -> R {
    let mut guard = STATE.gate.relock();
    let gate = guard.get_or_insert_with(|| {
        if STATE
            .serving
            .get()
            .is_some_and(|serving| serving.side == Side::CLIENT)
        {
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
    *STATE.gate.relock() = None;
    STATE.busy.store(false, Ordering::Release);
}

/// The account of a hosted game's host, once its own hello named it.
pub(crate) fn host() -> Option<u64> {
    STATE.serving.get()?;
    with_gate(|gate| gate.host)
}
/// Starts the join check on the player `key`, who entered the world; on the
/// game thread.
pub(crate) fn joined(key: u64) {
    let Some(serving) = STATE.serving.get().filter(|_| required()) else {
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
        let steps = with_gate(|gate| {
            gate.make_room(key, crate::engine::world::live_keys);
            gate.joined(key, &server, Instant::now(), RULES)
        });
        for step in steps {
            run(crate::host::loader(), serving, step);
        }
    });
}

/// Forgets the player `key`, who left.
pub(crate) fn left(key: u64) {
    let _ran = STATE.fuse.run("a player's leave", || {
        with_gate(|gate| gate.left(key));
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
                let (answered, _) = with_gate(|gate| gate.host_hello(key, hello));
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
            if !required() {
                return true;
            }
            let server = server_mods();
            let (stays, step) = with_gate(|gate| {
                gate.make_room(key, crate::engine::world::live_keys);
                gate.hello(key, &server, hello, Instant::now(), RULES)
            });
            if let Some(step) = step {
                run(crate::host::loader(), serving, step);
            }
            stays
        })
        .unwrap_or(true)
}

/// Forgets the players whose slot is gone, tells and kicks those whose time
/// ran out; on the game thread, after the step.
pub(crate) fn after_step(loader: &Loader) {
    if !STATE.busy.load(Ordering::Acquire) {
        return;
    }
    let epoch = *STATE.epoch.get_or_init(Instant::now);
    let now = Instant::now();
    let ms = u64::try_from(now.duration_since(epoch).as_millis()).unwrap_or(u64::MAX);
    if ms < STATE.next.load(Ordering::Relaxed) {
        return;
    }
    let next = u64::try_from(LOOK_EVERY.as_millis()).unwrap_or(u64::MAX);
    STATE.next.store(ms.saturating_add(next), Ordering::Relaxed);
    let Ok(machines) = crate::engine::world::machines(loader) else {
        return;
    };
    let live: Vec<u64> = machines.iter().map(|&(key, _)| key).collect();
    let Some(serving) = STATE.serving.get() else {
        return;
    };
    let _ran = STATE.fuse.run("the join admission step", || {
        crate::base::net::keep_greeting(&live);
        let in_game: Vec<u64> = if serving.side == Side::CLIENT {
            crate::engine::world::players(loader)
                .map(|players| players.iter().map(|p| p.key.get()).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let server = server_mods();
        let steps = with_gate(|gate| {
            gate.keep(&live);
            gate.host_in_game(&in_game);
            gate.due(&server, now, RULES)
        });
        for step in steps {
            run(Some(loader), serving, step);
        }
    });
}

/// The names of the running join mods, `(id, name)`, in the reader's
/// language: each mod's own text, else its manifest's name.
fn reader_names(loader: &Loader) -> Names {
    loader
        .mods
        .iter()
        .filter(|m| m.joins())
        .map(|m| (m.id.clone(), m.name()))
        .filter(|(_, name)| !name.is_empty())
        .collect()
}

/// Does one step; a kick needs the `loader`.
fn run(loader: Option<&Loader>, serving: &Serving, step: Step) {
    match step {
        Step::Note { key, hello, needs } => {
            let leaving = STATE
                .kick
                .get()
                .and_then(Option::as_ref)
                .map(|_| RULES.grace);
            let text = crate::engine::texts::for_player(key, || {
                let names = loader.map_or_else(|| serving.names.clone(), reader_names);
                super::note(&needs, hello, &names, leaving)
            });
            if hello {
                // A game that said hello runs Enshwick: it lists the
                // difference itself once it is back in the menu.
                crate::base::net::refused_mods(key, &server_mods());
            }
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
            crate::actions::chat::send(key, &text);
        }
        Step::Stay { key } => {
            crate::log!(
                INFO,
                "join check: the game of {key:016x} said a matching hello late; it stays"
            );
            crate::engine::texts::for_player(key, || {
                crate::actions::chat::send(key, &crate::engine::texts::fill("join.admitted", &[]))
            });
        }
        Step::Kick { key } => {
            let (Some(loader), Some(kick)) = (loader, STATE.kick.get().and_then(Option::as_ref))
            else {
                return;
            };
            let host = with_gate(|gate| gate.host);
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

/// Kicks the player `key` through the game, on the game thread, never on
/// the machine of `host`, a hosted game's host, nor on the session's own
/// machine; the player's machine handle, or why not.
fn disconnect(
    loader: &Loader,
    kick: &Kick,
    key: u64,
    host: Option<u64>,
) -> Result<u32, &'static str> {
    let server = crate::engine::world::reader().map_err(|_| "not on the game thread")?;
    let machines =
        crate::engine::world::machines(loader).map_err(|_| "the player slots do not read")?;
    // SAFETY: the session pointer lies at `kick.session` inside the server
    // object, as the table's pattern for that offset reads it from the server
    // in the game's own kick; `world::reader` succeeded, so this runs on the
    // game thread, which alone writes the server.
    let session = unsafe { crate::engine::memory::read_ptr::<u8>(server, kick.session) };
    if session.is_null() {
        return Err("the server has no session");
    }
    let host_machine = kick.host_machine.map(|offset| {
        // SAFETY: `session` is non-null, checked just before, and holds a
        // `u32` machine handle at `offset`, as the table's pattern for it finds
        // the game's own kick compare it with the machine to kick; read on
        // the game thread, which alone writes the session.
        unsafe { crate::engine::memory::read::<u32>(session.expose_provenance(), offset) }
    });
    let host_machine = host_machine.and_then(super::own_machine);
    let machine = super::machine_to_kick(&machines, key, host, host_machine)?;
    // SAFETY: `kick.call` is the table's kick, typed `KickFn` on this build,
    // called with the server's non-null session and the machine of one of
    // its player slots, on the game thread, as the server's own account
    // actions call it.
    unsafe { (kick.call)(session, machine, false) };
    Ok(machine)
}
