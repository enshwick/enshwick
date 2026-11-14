//! The mods' holds on the world's saves: the game's save block byte set
//! while a running mod holds them, the warning while they stay held long,
//! and every hold let go as the hosted game shuts down.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use enshwick_core::abi::Status;

use super::{Layout, layout};
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// How long saves stay held before the loader warns, and how often the
/// warning repeats while they stay held.
const LONG_HOLD: Duration = Duration::from_mins(10);

/// A mod's hold on the world's saves.
#[derive(Debug, Clone, Copy)]
struct Hold {
    owner: ModIndex,
    since: Instant,
}

/// Where the hosted game's shutdown is, for the mods' holds on its saves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shutdown {
    /// No shutdown began.
    Not,
    /// The mods hear `Event::SHUTDOWN_STARTED`: a hold now refuses the
    /// final save.
    Announcing,
    /// The mods heard it: a new hold waits for the next world.
    Over,
}

/// What a mod's call to hold or let go of saves did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// The mod holds saves now.
    Held,
    /// The mod holds saves from its shutdown handler, refusing the final save.
    Refused,
    /// The mod asked to hold after the shutdown was announced; not held.
    Late,
    /// The mod let go.
    LetGo,
    /// Nothing changed.
    Same,
}

/// The mods' holds on the world's saves.
#[derive(Debug)]
struct Saving {
    /// The mods that hold saves now, in the order they asked.
    holds: Vec<Hold>,
    /// The game's save block byte from before the loader set it, while mods
    /// hold saves.
    held: Option<u8>,
    /// When the next warning about saves held without a break is due.
    warn_at: Option<Instant>,
    shutdown: Shutdown,
}

impl Saving {
    const fn new() -> Self {
        Self {
            holds: Vec::new(),
            held: None,
            warn_at: None,
            shutdown: Shutdown::Not,
        }
    }

    /// Mod `owner` holds saves (`hold`) or lets them go at `now`.
    fn change(&mut self, owner: ModIndex, hold: bool, now: Instant) -> Change {
        let at = self.holds.iter().position(|h| h.owner == owner);
        match (hold, at, self.shutdown) {
            (true, None, Shutdown::Over) => Change::Late,
            (true, None, shutdown) => {
                self.holds.push(Hold { owner, since: now });
                self.warn_at.get_or_insert(now + LONG_HOLD);
                if shutdown == Shutdown::Announcing {
                    Change::Refused
                } else {
                    Change::Held
                }
            }
            (false, Some(at), _) => {
                self.holds.remove(at);
                if self.holds.is_empty() {
                    self.warn_at = None;
                }
                Change::LetGo
            }
            _ => Change::Same,
        }
    }

    /// The shutdown is about to be announced: every hold lets go, returned.
    fn shut_down(&mut self) -> Vec<Hold> {
        self.shutdown = Shutdown::Announcing;
        self.warn_at = None;
        core::mem::take(&mut self.holds)
    }
}

/// What the save holds keep while a world runs.
struct State {
    saving: Mutex<Saving>,
    /// Set while a mod holds saves or the loader still has the block byte
    /// set, so a step without holds takes no lock.
    holding: AtomicBool,
}

impl State {
    const fn new() -> Self {
        Self {
            saving: Mutex::new(Saving::new()),
            holding: AtomicBool::new(false),
        }
    }

    /// Forgets every mod's hold: a hold is on its world's saves, and carried
    /// on, it would stop the next world's.
    fn forget(&self) {
        *self.saving.relock() = Saving::new();
        self.holding.store(false, Ordering::Release);
    }
}

static STATE: State = State::new();

/// What the game's block byte, now `byte`, needs while mods hold saves
/// (`holding`), with the byte the game had before the hold (`held`): the
/// byte to write, if any, and the game's byte to keep. A hold sets the byte
/// on every step, so the game clearing it does not end the hold; the last
/// mod letting go gives the game its byte back.
const fn block(holding: bool, held: Option<u8>, byte: u8) -> (Option<u8>, Option<u8>) {
    match (holding, held) {
        (true, None) => (Some(1), Some(byte)),
        (true, Some(before)) => (if byte == 1 { None } else { Some(1) }, Some(before)),
        (false, Some(before)) => (Some(before), None),
        (false, None) => (None, None),
    }
}

/// Mod `owner` holds the world's saves (`hold`) or lets them go: the game
/// saves only while no mod holds them, and a save asked for meanwhile is
/// refused.
pub(crate) fn hold(loader: &Loader, owner: ModIndex, hold: bool) -> Status {
    if layout(loader).is_none() {
        return Status::MISSING;
    }
    if hold && HOLDS_FUSE.is_off() {
        return Status::UNAVAILABLE;
    }
    let id = mod_id(loader, owner);
    let mut saving = STATE.saving.relock();
    match saving.change(owner, hold, Instant::now()) {
        Change::Held => {
            STATE.holding.store(true, Ordering::Release);
            crate::log!(INFO, "world saves held by {id}");
        }
        Change::Refused => {
            STATE.holding.store(true, Ordering::Release);
            crate::log!(
                WARN,
                "world saves held by {id} as the game shuts down, so its final save does not run"
            );
        }
        Change::Late => crate::log!(
            WARN,
            "world saves not held by {id}: the shutdown was announced, so its final save runs"
        ),
        Change::LetGo => crate::log!(INFO, "world saves let go by {id}"),
        Change::Same => {}
    }
    Status::OK
}

/// The id of mod `owner`, for the log.
fn mod_id(loader: &Loader, owner: ModIndex) -> &str {
    loader.mods.get(owner.get()).map_or("?", |m| m.id.as_str())
}

/// After a step on the server's game thread: sets the game's save block
/// while a running mod holds saves, gives the byte back once none does, and
/// warns while saves stay held long.
///
/// # Safety
///
/// - `server` is the live server, read and written on its game thread
///   outside a step, where nothing else writes it.
pub(super) unsafe fn after_step(loader: &Loader, layout: &Layout, server: usize) {
    if !STATE.holding.load(Ordering::Acquire) {
        return;
    }
    // SAFETY: `server` is the live server on its game thread outside a
    // step, by this function's `# Safety`.
    if let Some(long) = unsafe { enforce(layout, server) } {
        warn_long(loader, &long);
    }
}

/// The switch of [`enforce`]'s writes, off once the live server's save
/// block does not hold its found offset; saves then run as vanilla runs
/// them.
static HOLDS_FUSE: crate::engine::fuse::Fuse = crate::engine::fuse::Fuse::new("save holds");

/// Checks the live `server` before the first write to its save block: the
/// byte at `blocked` reads and is a flag, 0 or 1, as [`block`] writes it.
fn blocked_holds(server: usize, blocked: usize) -> Result<(), crate::engine::live::Broken> {
    let entry = crate::engine::pinned::server::SAVE_BLOCKED.id();
    let [byte] = crate::engine::live::bytes::<1>(entry, server, blocked)?;
    crate::engine::live::holds(entry, byte <= 1, "a flag, 0 or 1")
}

/// Sets the game's save block while a running mod holds saves and gives the
/// byte back once none does; the holds and how long each has lasted when a
/// warning about saves held long is due.
///
/// # Safety
///
/// - `server` is the live server, read and written on its game thread
///   outside a step, where nothing else writes it.
unsafe fn enforce(layout: &Layout, server: usize) -> Option<Vec<(ModIndex, Duration)>> {
    let now = Instant::now();
    let mut saving = STATE.saving.relock();
    saving
        .holds
        .retain(|h| !crate::engine::loader::is_off(h.owner));
    if saving.holds.is_empty() {
        saving.warn_at = None;
    }
    let build = crate::engine::loader::loader().map_or("", |loader| loader.build.name.as_str());
    if !HOLDS_FUSE.verify(build, || blocked_holds(server, layout.blocked)) {
        return None;
    }
    // SAFETY: `server` is the live server on its game thread outside a step
    // by this function's `# Safety`; its byte that refuses saves sits at the
    // table's `server.save_blocked`.
    let byte: u8 = unsafe { crate::engine::memory::read(server, layout.blocked) };
    let (write, held) = block(!saving.holds.is_empty(), saving.held, byte);
    if let Some(write) = write {
        // SAFETY: the same byte, by the read just before.
        unsafe { crate::engine::memory::write(server, layout.blocked, write) };
    }
    saving.held = held;
    if saving.holds.is_empty() && held.is_none() {
        STATE.holding.store(false, Ordering::Release);
    }
    due(&mut saving.warn_at, now).then(|| {
        saving
            .holds
            .iter()
            .map(|h| (h.owner, now.duration_since(h.since)))
            .collect()
    })
}

/// Whether a warning about saves held long is due at `now`, the next due at
/// `next`; a due warning moves `next` on by [`LONG_HOLD`].
fn due(next: &mut Option<Instant>, now: Instant) -> bool {
    match *next {
        Some(at) if now >= at => {
            *next = Some(now + LONG_HOLD);
            true
        }
        _ => false,
    }
}

/// `held` as the log reads it: each mod's id with the whole minutes it has
/// held saves.
fn held_by(loader: &Loader, held: &[(ModIndex, Duration)]) -> String {
    held.iter()
        .map(|&(owner, long)| format!("{} ({} min)", mod_id(loader, owner), long.as_secs() / 60))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Warns in the log and tells each administrator online that saves are
/// held long, by the mods of `held`.
fn warn_long(loader: &Loader, held: &[(ModIndex, Duration)]) {
    crate::log!(
        WARN,
        "world saves are held long, by {}: the game saves only once every mod lets go",
        held_by(loader, held)
    );
    let names = held
        .iter()
        .filter_map(|&(owner, _)| loader.mods.get(owner.get()))
        .map(crate::engine::loader::ModRecord::label)
        .collect::<Vec<_>>()
        .join(", ");
    tell_admins(loader, &names);
}

/// Tells each administrator online that saves are held long, by the mods
/// named `mods`.
fn tell_admins(loader: &Loader, mods: &str) {
    let Ok(players) = crate::engine::world::players(loader) else {
        return;
    };
    for player in players.iter().filter(|p| p.is_admin()) {
        let key = player.key.get();
        let text = crate::engine::texts::for_player(key, || {
            crate::engine::texts::fill("saves.held-long", &[("mods", mods)])
        });
        crate::actions::reply::tell(loader, key, &text);
    }
}

/// The hosted game's shutdown begins, on its game thread: every mod's hold
/// on saves lets go and the game gets its save block byte back, and the log
/// names each mod that held saves and for how long; then `announce` tells
/// the mods. A hold set while they hear it refuses the final save and
/// applies at once, before the game asks for that save; a hold asked for
/// after it is not taken.
pub(crate) fn shut_down(loader: &Loader, announce: impl FnOnce()) {
    let Some(layout) = layout(loader) else {
        announce();
        return;
    };
    let now = Instant::now();
    for hold in STATE.saving.relock().shut_down() {
        crate::log!(
            WARN,
            "world saves held by {} for {} s are let go as the game shuts down",
            mod_id(loader, hold.owner),
            now.duration_since(hold.since).as_secs()
        );
    }
    let server = crate::engine::world::server();
    match server {
        // SAFETY: `world::server` returned the live server on its game
        // thread outside a step; the game reads it until its teardown.
        Ok(server) => drop(unsafe { enforce(layout, server) }),
        Err(status) => crate::log!(
            WARN,
            "world saves keep the game's block as it is at shutdown: the server is out of reach here ({status:?})"
        ),
    }
    announce();
    STATE.saving.relock().shutdown = Shutdown::Over;
    if let Ok(server) = server
        && STATE.holding.load(Ordering::Acquire)
    {
        // SAFETY: `server` is `world::server`'s live server on its game
        // thread outside a step, and the mods heard of the shutdown on this
        // same thread, which the game frees the server on only later.
        drop(unsafe { enforce(layout, server) });
    }
}

/// Whether the hosted game's shutdown began, as [`shut_down`] marks it.
pub(crate) fn shutting_down() -> bool {
    STATE.saving.relock().shutdown != Shutdown::Not
}

/// Forgets every mod's hold, when a hosted game ends.
pub(super) fn forget() {
    STATE.forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shutdown_lets_every_hold_go_and_only_a_hold_while_it_is_announced_refuses_the_final_save()
     {
        let (a, b) = (ModIndex::new(0), ModIndex::new(1));
        let now = Instant::now();
        let mut saving = Saving::new();
        assert_eq!(saving.change(a, true, now), Change::Held);
        assert_eq!(saving.change(a, true, now), Change::Same);
        assert_eq!(saving.change(b, true, now), Change::Held);
        let released: Vec<_> = saving.shut_down().iter().map(|h| h.owner).collect();
        assert_eq!(released, [a, b]);
        assert!(saving.holds.is_empty() && saving.warn_at.is_none());
        assert_eq!(saving.change(a, true, now), Change::Refused);
        assert_eq!(saving.warn_at, Some(now + LONG_HOLD));
        saving.shutdown = Shutdown::Over;
        assert_eq!(
            saving.change(b, true, now),
            Change::Late,
            "a hold re-asserted after the announcement is not taken"
        );
        assert_eq!(saving.change(a, false, now), Change::LetGo);
        assert!(saving.holds.is_empty() && saving.warn_at.is_none());
        assert_eq!(saving.change(a, false, now), Change::Same);
    }

    #[test]
    fn a_hosted_game_s_end_lets_every_hold_go_and_ends_its_shutdown() {
        let state = State::new();
        {
            let mut saving = state.saving.relock();
            let _held = saving.change(ModIndex::new(0), true, Instant::now());
            saving.shutdown = Shutdown::Over;
        }
        state.holding.store(true, Ordering::Release);
        state.forget();
        let saving = state.saving.relock();
        assert!(saving.holds.is_empty() && saving.warn_at.is_none());
        assert!(saving.held.is_none() && saving.shutdown == Shutdown::Not);
        assert!(!state.holding.load(Ordering::Acquire));
    }

    #[test]
    fn a_live_save_block_holds_only_while_it_reads_as_a_flag() {
        let server = [7u8, 0, 1, 2];
        let at = server.as_ptr().addr();
        assert_eq!(blocked_holds(at, 1), Ok(()));
        assert_eq!(blocked_holds(at, 2), Ok(()));
        let why = blocked_holds(at, 3).unwrap_err();
        assert_eq!(why.entry, crate::engine::pinned::server::SAVE_BLOCKED.id());
        assert!(
            blocked_holds(0x10, 0).is_err(),
            "a server that does not read"
        );
    }

    #[test]
    fn saves_held_long_warn_once_every_ten_minutes_and_not_before() {
        let start = Instant::now();
        let mut next = Some(start + LONG_HOLD);
        let at = |mins: u64, secs: u64| start + Duration::from_secs(mins * 60 + secs);
        assert!(!due(&mut next, at(9, 59)), "not before ten minutes");
        assert!(due(&mut next, at(10, 0)));
        assert!(!due(&mut next, at(10, 1)), "once");
        assert!(!due(&mut next, at(19, 59)));
        assert!(due(&mut next, at(20, 0)), "again ten minutes on");
        let mut none = None;
        assert!(!due(&mut none, at(60, 0)), "never while nothing is held");
    }

    #[test]
    fn a_hold_sets_the_block_on_every_step_and_the_last_release_gives_the_game_its_byte() {
        assert_eq!(block(true, None, 0), (Some(1), Some(0)));
        assert_eq!(block(true, Some(0), 1), (None, Some(0)), "already set");
        assert_eq!(
            block(true, Some(0), 0),
            (Some(1), Some(0)),
            "the game cleared it, and the hold sets it again"
        );
        assert_eq!(block(false, Some(0), 1), (Some(0), None));
        assert_eq!(
            block(true, None, 1),
            (Some(1), Some(1)),
            "the game had it set"
        );
        assert_eq!(block(false, Some(1), 1), (Some(1), None));
        assert_eq!(block(false, None, 0), (None, None), "no hold, no write");
    }
}
