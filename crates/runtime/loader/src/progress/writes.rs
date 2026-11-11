//! Level and experience writes mods ask for, applied in the server's level
//! step after the experience filters and before the game applies the gains.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::{
    PROGRESS_ADD_EXPERIENCE, PROGRESS_OPEN, PROGRESS_SET_EXPERIENCE, PROGRESS_SET_LEVEL, Status,
};

use super::{
    BALANCING, ENTITY, EXPERIENCE, KIND, LEVEL, LEVEL_TRACKING, REGISTRY, STAMP_A, STAMP_B,
    TRACKER, WORDS, levels,
};
use crate::ecs::Engine;
use crate::host::Loader;
use crate::mod_index::ModIndex;
use crate::registry::Registry;
use enshwick_base::Relock;

/// Level and experience writes that wait for the level step at most.
const MOST_WRITES: usize = 256;

/// A level or experience write a mod asked for.
#[derive(Debug, Clone, Copy)]
struct Write {
    entity: u32,
    what: u32,
    value: u32,
    /// A step's end passed since it was asked: the next end drops it, its
    /// character was in no level step.
    seen: bool,
}

/// The level and experience writes mods asked for, which the level step
/// applies.
#[derive(Debug)]
pub(super) struct Writes {
    waiting: Mutex<Vec<Write>>,
    /// Whether `waiting` holds any, read first by the level step.
    pub(super) any: AtomicBool,
    /// Whether the level step is linked.
    pub(super) linked: AtomicBool,
}

pub(super) static PENDING: Writes = Writes {
    waiting: Mutex::new(Vec::new()),
    any: AtomicBool::new(false),
    linked: AtomicBool::new(false),
};
/// The mods that asked for level and experience writes.
pub(super) static WRITERS: Registry<ModIndex> = Registry::new();
/// The level and experience writes' switch.
pub(super) static WRITES_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("level writes");

/// Asks for level and experience writes for mod `owner` during `init`
/// ([`PROGRESS_OPEN`]), or queues one for the next level step.
pub(crate) fn write(
    loader: &Loader,
    owner: ModIndex,
    entity: u32,
    what: u32,
    value: u32,
) -> Result<(), Status> {
    if asks(entity, what)? {
        if levels(loader).is_none() || crate::ecs::engine(loader).is_none() {
            return Err(Status::MISSING);
        }
        return WRITERS.add(owner).map_err(Status::from);
    }
    if !PENDING.linked.load(Ordering::Relaxed) || WRITES_FUSE.is_off() {
        return Err(Status::MISSING);
    }
    if !WRITERS.sealed().is_some_and(|mods| mods.contains(&owner)) {
        return Err(Status::REFUSED);
    }
    crate::world::server()?;
    if entity == 0 || !crate::world::player_entities(loader)?.contains(&entity) {
        return Err(Status::REFUSED);
    }
    queue(Write {
        entity,
        what,
        value,
        seen: false,
    })
}

/// Whether `what` asks for the writes, which only `Entity(0)` does, rather
/// than being one.
fn asks(entity: u32, what: u32) -> Result<bool, Status> {
    match what {
        PROGRESS_OPEN if entity == 0 => Ok(true),
        PROGRESS_SET_LEVEL | PROGRESS_SET_EXPERIENCE | PROGRESS_ADD_EXPERIENCE => Ok(false),
        _ => Err(Status::BAD_ARGS),
    }
}

/// Queues `w` for the level step, while fewer than [`MOST_WRITES`] wait.
fn queue(w: Write) -> Result<(), Status> {
    let mut waiting = PENDING.waiting.relock();
    if waiting.len() >= MOST_WRITES {
        return Err(Status::REFUSED);
    }
    waiting.push(w);
    PENDING.any.store(true, Ordering::Release);
    Ok(())
}

/// Drops the writes whose character was in no level step since a step's
/// end passed, on the game thread after the step.
pub(crate) fn after_step() {
    if !PENDING.any.load(Ordering::Acquire) {
        return;
    }
    let mut waiting = PENDING.waiting.relock();
    let before = waiting.len();
    waiting.retain(|w| !w.seen);
    let dropped = before - waiting.len();
    if dropped != 0 {
        crate::log!(
            WARN,
            "{dropped} level or experience writes dropped: their characters were in no level step that could take them"
        );
    }
    for w in waiting.iter_mut() {
        w.seen = true;
    }
    PENDING.any.store(!waiting.is_empty(), Ordering::Release);
}

/// What `write` makes of a character's level and experience, `None` for
/// what it leaves: the level, then the experience inside it, what the
/// level needs and the gain the step applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Written {
    level: Option<u32>,
    progress: Option<u32>,
    need: Option<u32>,
    gained: Option<u32>,
}

/// One write on a character at `need` with `gained` waiting, with the
/// game's level cap and level table.
fn written(w: Write, need: u32, gained: u32, cap: u32, need_for: impl Fn(u32) -> u32) -> Written {
    match w.what {
        PROGRESS_SET_LEVEL => {
            let level = w.value.clamp(1, cap.max(1));
            Written {
                level: Some(level),
                progress: Some(0),
                need: Some(need_for(level)),
                gained: Some(0),
            }
        }
        PROGRESS_SET_EXPERIENCE => Written {
            progress: Some(w.value.min(need.saturating_sub(1))),
            gained: Some(0),
            ..Written::default()
        },
        _ => Written {
            gained: Some(gained.saturating_add(w.value)),
            ..Written::default()
        },
    }
}

/// Applies the waiting writes to the characters of this level step, after
/// the filters and before the game applies the gains.
///
/// # Safety
///
/// - `ctx` is the live query context the game passed to `player_level`,
///   used on that system's thread while it runs, before vanilla runs.
#[cold]
pub(super) unsafe fn apply_writes(loader: &Loader, engine: &Engine, ctx: *mut u64) {
    let Some(levels) = levels(loader) else {
        return;
    };
    // The step's batches run on several threads at once; a write waits
    // rarely, and only then does a batch take the lock.
    let mut waiting = PENDING.waiting.relock();
    // SAFETY: `ctx` is the live query context of `player_level`, which only
    // this thread writes while the system runs (the caller's contract).
    unsafe { engine.set_row(ctx, 0) };
    // SAFETY: `ctx` is the live query context of `player_level` on its
    // thread (the caller's contract); `WORDS` is that system's iterator size
    // on the layout `link` checks against its reflected components.
    let mut query = unsafe { engine.query::<WORDS>(ctx) };
    while let Some(words) = query.next() {
        // SAFETY: `ctx` is the live query context of `player_level`, used on
        // its thread while it runs (the caller's contract).
        let entity = unsafe { engine.entity(ctx) };
        if !waiting.iter().any(|w| w.entity == entity) || words[BALANCING] == 0 {
            continue;
        }
        let experience = words[EXPERIENCE] as *mut u8;
        let (registry, balancing) = (words[REGISTRY], words[BALANCING]);
        // SAFETY: `level_cap` is the table's `experience.level_cap`, called
        // with the non-zero balancing registry of a live `player_level` row,
        // on that system's thread while it runs.
        let cap = unsafe { (levels.level_cap)(balancing) };
        // SAFETY: `need_for` is the table's `experience.need_for_level`,
        // called by `written` with a level of 1 or more and the balancing
        // registry of a live `player_level` row, on that system's thread.
        let need_for = |level: u32| unsafe { (levels.need_for)(level, balancing) };
        let (mut level_written, mut experience_written) = (false, false);
        // Once a write waits, the character's later ones wait behind it, in order.
        let mut held = false;
        waiting.retain(|&w| {
            if w.entity != entity || held {
                return true;
            }
            // SAFETY: `experience` is the current row's `Experience` set,
            // null or live, and `registry` the same row's attribute registry,
            // on the system's thread.
            let read = |attribute| unsafe { engine.attribute(registry, experience, attribute) };
            let (Some(need), Some(gained)) = (read(levels.need), read(levels.gained)) else {
                // Kept for the next step; `after_step` drops it with a warning.
                held = true;
                return true;
            };
            let change = written(w, need, gained, cap, need_for);
            if let Some(level) = change.level {
                // SAFETY: `words[LEVEL]` is the current row's `Level` set,
                // which `player_level` writes and nothing else writes while
                // it runs on this thread; `registry` is the row's registry.
                if !unsafe { engine.set_level(registry, words[LEVEL] as *mut u8, level) } {
                    // The old level keeps its experience, as for a read above.
                    held = true;
                    return true;
                }
                level_written = true;
            }
            for (value, attribute) in [
                (change.progress, levels.progress),
                (change.need, levels.need),
                (change.gained, levels.gained),
            ] {
                if let Some(value) = value {
                    // SAFETY: `experience` is the current row's `Experience`
                    // set, which `player_level` writes and nothing else
                    // writes while it runs on this thread; `registry` is the
                    // same row's attribute registry.
                    experience_written |=
                        unsafe { engine.set_attribute(registry, experience, attribute, value) };
                }
            }
            false
        });
        for (shift, wanted) in [(0, experience_written), (LEVEL_TRACKING, level_written)] {
            if wanted {
                // SAFETY: the words are the change tracking of the current
                // row's `Experience` or, `LEVEL_TRACKING` on, `Level` set of
                // the live `player_level` query, which writes both
                // (`WRITES`), on that system's thread while it runs.
                unsafe {
                    engine.mark_changed(
                        words[TRACKER + shift],
                        words[STAMP_A + shift],
                        words[STAMP_B + shift],
                        words[ENTITY + shift],
                        words[KIND + shift],
                    );
                }
            }
        }
    }
    PENDING.any.store(!waiting.is_empty(), Ordering::Release);
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;

    use enshwick_core::abi::ExperiencePass;

    use super::*;
    use crate::host::Registered;

    fn queued(what: u32, value: u32) -> Write {
        Write {
            entity: 7,
            what,
            value,
            seen: false,
        }
    }

    /// The game's table on the 1.0 builds: 100 a level.
    fn need_for(level: u32) -> u32 {
        level * 100
    }

    #[test]
    fn a_set_level_stays_within_the_cap_and_needs_what_the_game_gives_that_level() {
        let set = |level| written(queued(PROGRESS_SET_LEVEL, level), 900, 40, 25, need_for);
        assert_eq!(
            set(10),
            Written {
                level: Some(10),
                progress: Some(0),
                need: Some(1000),
                gained: Some(0),
            }
        );
        assert_eq!(set(0).level, Some(1), "never below level 1");
        assert_eq!(set(0).need, Some(100));
        assert_eq!(set(99).level, Some(25), "never past the cap");
        assert_eq!(
            written(queued(PROGRESS_SET_LEVEL, 5), 0, 0, 0, need_for).level,
            Some(1),
            "a cap of 0 still leaves level 1"
        );
    }

    #[test]
    fn only_entity_0_asks_for_the_writes_and_another_what_is_refused() {
        assert_eq!(asks(0, PROGRESS_OPEN), Ok(true));
        assert_eq!(asks(7, PROGRESS_OPEN), Err(Status::BAD_ARGS));
        for what in [
            PROGRESS_SET_LEVEL,
            PROGRESS_SET_EXPERIENCE,
            PROGRESS_ADD_EXPERIENCE,
        ] {
            assert_eq!(asks(7, what), Ok(false), "{what}");
        }
        assert_eq!(asks(7, 4), Err(Status::BAD_ARGS));
    }

    #[test]
    fn writes_wait_up_to_the_limit_and_go_after_a_second_step_s_end() {
        for _ in 0..MOST_WRITES {
            assert_eq!(queue(queued(PROGRESS_SET_LEVEL, 3)), Ok(()));
        }
        assert_eq!(queue(queued(PROGRESS_SET_LEVEL, 3)), Err(Status::REFUSED));
        assert!(PENDING.any.load(Ordering::Acquire));
        after_step();
        assert_eq!(PENDING.waiting.relock().len(), MOST_WRITES, "kept one end");
        assert!(PENDING.any.load(Ordering::Acquire));
        assert_eq!(queue(queued(PROGRESS_SET_LEVEL, 4)), Err(Status::REFUSED));
        after_step();
        assert!(PENDING.waiting.relock().is_empty(), "dropped at the second");
        assert!(!PENDING.any.load(Ordering::Acquire));
        assert_eq!(queue(queued(PROGRESS_SET_LEVEL, 5)), Ok(()));
        after_step();
        assert_eq!(PENDING.waiting.relock().len(), 1, "a new one waits again");
    }

    #[test]
    fn set_experience_stays_below_the_need_and_add_experience_adds_to_the_gain() {
        let set = |value, need| {
            written(
                queued(PROGRESS_SET_EXPERIENCE, value),
                need,
                40,
                25,
                need_for,
            )
        };
        assert_eq!(
            set(300, 900),
            Written {
                progress: Some(300),
                gained: Some(0),
                ..Written::default()
            }
        );
        assert_eq!(set(5000, 900).progress, Some(899));
        assert_eq!(set(5, 0).progress, Some(0));
        let add = |value, gained| {
            written(
                queued(PROGRESS_ADD_EXPERIENCE, value),
                900,
                gained,
                25,
                need_for,
            )
        };
        assert_eq!(
            add(500, 40),
            Written {
                gained: Some(540),
                ..Written::default()
            }
        );
        assert_eq!(add(u32::MAX, 40).gained, Some(u32::MAX));
    }

    unsafe extern "system" fn no_pass(_pass: *mut ExperiencePass, _user: *mut c_void) {}

    #[test]
    fn writers_are_sealed_when_experience_filters_link_too_and_only_they_may_write() {
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
        let (filter, writer) = (ModIndex::new(0), ModIndex::new(1));
        super::super::EXPERIENCE_FILTERS
            .relock()
            .push(Registered::new(filter, no_pass, core::ptr::null_mut()));
        assert_eq!(WRITERS.add(writer), Ok(()));
        super::super::link(loader);
        assert!(WRITERS.sealed().is_some_and(|mods| mods.contains(&writer)));
        // As if the level step had linked, which needs a game.
        PENDING.linked.store(true, Ordering::Relaxed);
        assert_eq!(
            write(loader, filter, 7, PROGRESS_SET_LEVEL, 3),
            Err(Status::REFUSED),
            "a mod that did not ask"
        );
        assert_ne!(
            write(loader, writer, 7, PROGRESS_SET_LEVEL, 3),
            Err(Status::REFUSED),
            "the mod that asked gets past the seal to the world checks"
        );
    }
}
