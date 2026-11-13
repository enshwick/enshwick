//! Level and experience writes mods ask for, applied in the server's level
//! step after the experience filters and before the game applies the gains.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::{
    Event, Header, PROGRESS_ADD_EXPERIENCE, PROGRESS_OPEN, PROGRESS_SET_EXPERIENCE,
    PROGRESS_SET_LEVEL, ProgressWriteData, Status,
};

use super::{
    BALANCING, ENTITY, EXPERIENCE, KIND, LEVEL, LEVEL_TRACKING, REGISTRY, STAMP_A, STAMP_B,
    TRACKER, WORDS, levels,
};
use crate::engine::ecs::Engine;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::registry::Registry;
use enshwick_base::Relock;

/// Level and experience writes that wait for the level step at most.
const MOST_WRITES: usize = 256;

/// A level or experience write a mod asked for.
#[derive(Debug, Clone, Copy)]
struct Write {
    owner: ModIndex,
    entity: u32,
    what: u32,
    value: u32,
    /// A step's end passed since it was asked: the next end drops it, its
    /// character was in no level step.
    seen: bool,
    /// Why it waits, the outcome its mod hears if the next end drops it:
    /// [`ProgressWriteData::NO_LEVEL_STEP`] until a level step held it.
    why: u32,
}

impl Write {
    const fn new(owner: ModIndex, entity: u32, what: u32, value: u32) -> Self {
        Self {
            owner,
            entity,
            what,
            value,
            seen: false,
            why: ProgressWriteData::NO_LEVEL_STEP,
        }
    }

    /// What its mod hears: `outcome`, with what an applied write `left`.
    fn heard(self, outcome: u32, left: Left) -> (ModIndex, ProgressWriteData) {
        let data = ProgressWriteData {
            header: Header::of::<ProgressWriteData>(),
            entity: enshwick_core::Entity(self.entity),
            what: self.what,
            value: self.value,
            outcome,
            level: left.level,
            experience: left.experience,
            gained: left.gained,
        };
        (self.owner, data)
    }
}

/// A character's level and experience as an applied write left them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Left {
    level: u32,
    experience: u32,
    gained: u32,
}

/// The writes that wait for the level step, and the outcomes their mods
/// hear after it.
#[derive(Debug)]
struct Queue {
    waiting: Vec<Write>,
    heard: Vec<(ModIndex, ProgressWriteData)>,
}

/// The level and experience writes mods asked for, which the level step
/// applies.
#[derive(Debug)]
pub(super) struct Writes {
    queue: Mutex<Queue>,
    /// Whether `queue` holds a write, read first by the level step.
    pub(super) any: AtomicBool,
    /// Whether `queue` holds an outcome to tell after the step.
    told: AtomicBool,
    /// Whether the level step is linked.
    pub(super) linked: AtomicBool,
}

impl Writes {
    const fn new() -> Self {
        Self {
            queue: Mutex::new(Queue {
                waiting: Vec::new(),
                heard: Vec::new(),
            }),
            any: AtomicBool::new(false),
            told: AtomicBool::new(false),
            linked: AtomicBool::new(false),
        }
    }

    /// Queues `w` for the level step, while fewer than [`MOST_WRITES`] wait.
    fn push(&self, w: Write) -> Result<(), Status> {
        let mut queue = self.queue.relock();
        if queue.waiting.len() >= MOST_WRITES {
            return Err(Status::REFUSED);
        }
        queue.waiting.push(w);
        self.any.store(true, Ordering::Release);
        Ok(())
    }

    /// The outcomes to tell after a step's end: those the step applied, then
    /// the writes whose character was in no level step since an end before.
    fn step_ended(&self) -> Vec<(ModIndex, ProgressWriteData)> {
        if !self.any.load(Ordering::Acquire) && !self.told.load(Ordering::Acquire) {
            return Vec::new();
        }
        let mut queue = self.queue.relock();
        let mut heard = core::mem::take(&mut queue.heard);
        let applied = heard.len();
        queue.waiting.retain(|w| {
            if w.seen {
                heard.push(w.heard(w.why, Left::default()));
            }
            !w.seen
        });
        let dropped = heard.len() - applied;
        if dropped != 0 {
            crate::log!(
                WARN,
                "{dropped} level or experience writes dropped: no level step could take them; their mods hear why"
            );
        }
        // The next step says anew why a write waits, if it looks at it.
        for w in &mut queue.waiting {
            w.seen = true;
            w.why = ProgressWriteData::NO_LEVEL_STEP;
        }
        self.any.store(!queue.waiting.is_empty(), Ordering::Release);
        self.told.store(false, Ordering::Release);
        heard
    }

    /// Every outcome left when a hosted game ends: those heard, then each
    /// write still waiting, dropped with its reason, since its character
    /// and entity id go with the game.
    fn game_ended(&self) -> Vec<(ModIndex, ProgressWriteData)> {
        let mut queue = self.queue.relock();
        let mut heard = core::mem::take(&mut queue.heard);
        heard.extend(
            queue
                .waiting
                .drain(..)
                .map(|w| w.heard(w.why, Left::default())),
        );
        self.any.store(false, Ordering::Release);
        self.told.store(false, Ordering::Release);
        heard
    }
}

pub(super) static PENDING: Writes = Writes::new();
/// The mods that asked for level and experience writes.
pub(super) static WRITERS: Registry<ModIndex> = Registry::new();
/// The level and experience writes' switch.
pub(super) static WRITES_FUSE: crate::engine::fuse::Fuse =
    crate::engine::fuse::Fuse::new("level writes");

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
        if levels(loader).is_none() || crate::engine::ecs::engine(loader).is_none() {
            return Err(Status::MISSING);
        }
        return WRITERS.add(owner).map_err(Status::from);
    }
    if !PENDING.linked.load(Ordering::Relaxed) || WRITES_FUSE.is_off() || super::LEVEL_FUSE.is_off()
    {
        return Err(Status::MISSING);
    }
    if !WRITERS.sealed().is_some_and(|mods| mods.contains(&owner)) {
        return Err(Status::REFUSED);
    }
    crate::engine::world::server()?;
    if entity == 0 || !crate::engine::world::player_entities(loader)?.contains(&entity) {
        return Err(Status::REFUSED);
    }
    PENDING.push(Write::new(owner, entity, what, value))
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

/// Tells each mod what became of its writes, on the game thread after the
/// step: those the step applied, and those dropped as no level step could
/// take them.
pub(crate) fn after_step(loader: &Loader) {
    tell(loader, PENDING.step_ended());
}

/// Tells each mod what became of its writes as a hosted game ends: none
/// waits into the next game, where its entity id may name another
/// character.
pub(crate) fn game_ended(loader: &Loader) {
    tell(loader, PENDING.game_ended());
    // What the handlers asked for while hearing it goes with the game too.
    let _queued_by_handlers = PENDING.game_ended();
}

/// Raises each outcome for the mod it belongs to.
fn tell(loader: &Loader, heard: Vec<(ModIndex, ProgressWriteData)>) {
    // Raised with the queue unlocked: a handler may write again.
    for (owner, data) in heard {
        loader.emit(
            Event::PROGRESS_WRITTEN,
            Some(owner.get()),
            (&raw const data).cast(),
        );
    }
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
    let mut queue = PENDING.queue.relock();
    let Queue { waiting, heard } = &mut *queue;
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
        waiting.retain_mut(|w| {
            if w.entity != entity {
                return true;
            }
            if held {
                w.why = ProgressWriteData::BEHIND_DROPPED;
                return true;
            }
            // SAFETY: `experience` is the current row's `Experience` set,
            // null or live, and `registry` the same row's attribute registry,
            // on the system's thread.
            let read = |attribute| unsafe { engine.attribute(registry, experience, attribute) };
            let (Some(need), Some(gained)) = (read(levels.need), read(levels.gained)) else {
                // Kept for the next step; `step_ended` drops it with a warning.
                w.why = ProgressWriteData::UNREADABLE;
                held = true;
                return true;
            };
            let change = written(*w, need, gained, cap, need_for);
            if let Some(level) = change.level {
                // SAFETY: `words[LEVEL]` is the current row's `Level` set,
                // which `player_level` writes and nothing else writes while
                // it runs on this thread; `registry` is the row's registry.
                if !unsafe { engine.set_level(registry, words[LEVEL] as *mut u8, level) } {
                    // The old level keeps its experience, as for a read above.
                    w.why = ProgressWriteData::LEVEL_REFUSED;
                    held = true;
                    return true;
                }
                level_written = true;
            }
            let mut took = true;
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
                    let set =
                        unsafe { engine.set_attribute(registry, experience, attribute, value) };
                    experience_written |= set;
                    took &= set;
                }
            }
            // What the character holds now, read back rather than assumed.
            // SAFETY: `words[LEVEL]` is the current row's `Level` set, null
            // or live, and `registry` the same row's attribute registry, on
            // the system's thread.
            let level = unsafe { engine.level(registry, words[LEVEL] as *const u8) };
            let left = match (took, level, read(levels.progress), read(levels.gained)) {
                (true, Some(level), Some(experience), Some(gained)) => Some(Left {
                    level,
                    experience,
                    gained,
                }),
                _ => None,
            };
            heard.push(match left {
                Some(left) => w.heard(ProgressWriteData::APPLIED, left),
                None => w.heard(ProgressWriteData::UNCONFIRMED, Left::default()),
            });
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
    if !heard.is_empty() {
        PENDING.told.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;

    use enshwick_core::abi::ExperiencePass;

    use super::*;
    use crate::engine::loader::Registered;

    fn queued(what: u32, value: u32) -> Write {
        Write::new(ModIndex::new(1), 7, what, value)
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
        let pending = Writes::new();
        for _ in 0..MOST_WRITES {
            assert_eq!(pending.push(queued(PROGRESS_SET_LEVEL, 3)), Ok(()));
        }
        assert_eq!(
            pending.push(queued(PROGRESS_SET_LEVEL, 3)),
            Err(Status::REFUSED)
        );
        assert!(pending.any.load(Ordering::Acquire));
        assert!(pending.step_ended().is_empty(), "kept one end");
        assert_eq!(pending.queue.relock().waiting.len(), MOST_WRITES);
        assert!(pending.any.load(Ordering::Acquire));
        assert_eq!(
            pending.push(queued(PROGRESS_SET_LEVEL, 4)),
            Err(Status::REFUSED)
        );
        let dropped = pending.step_ended();
        assert_eq!(dropped.len(), MOST_WRITES, "dropped at the second");
        assert!(pending.queue.relock().waiting.is_empty());
        assert!(!pending.any.load(Ordering::Acquire));
        assert_eq!(pending.push(queued(PROGRESS_SET_LEVEL, 5)), Ok(()));
        assert!(pending.step_ended().is_empty());
        assert_eq!(
            pending.queue.relock().waiting.len(),
            1,
            "a new one waits again"
        );
    }

    #[test]
    fn a_dropped_write_tells_its_mod_why_and_what_it_asked() {
        let pending = Writes::new();
        assert_eq!(pending.push(queued(PROGRESS_ADD_EXPERIENCE, 50)), Ok(()));
        assert_eq!(pending.push(queued(PROGRESS_SET_LEVEL, 3)), Ok(()));
        assert!(pending.step_ended().is_empty());
        // The second step holds the first write: its experience did not read.
        pending.queue.relock().waiting[0].why = ProgressWriteData::UNREADABLE;
        let heard = pending.step_ended();
        assert_eq!(heard.len(), 2);
        let (owner, first) = heard[0];
        assert_eq!(owner, ModIndex::new(1));
        assert_eq!(
            (first.entity, first.what, first.value, first.outcome),
            (
                enshwick_core::Entity(7),
                PROGRESS_ADD_EXPERIENCE,
                50,
                ProgressWriteData::UNREADABLE
            )
        );
        assert_eq!((first.level, first.experience, first.gained), (0, 0, 0));
        assert_eq!(heard[1].1.outcome, ProgressWriteData::NO_LEVEL_STEP);
    }

    #[test]
    fn the_step_s_end_and_the_game_s_end_tell_what_waits_and_keep_nothing() {
        let loader = crate::start::tests::bare_loader();
        let applied =
            queued(PROGRESS_SET_LEVEL, 12).heard(ProgressWriteData::APPLIED, Left::default());
        PENDING.queue.relock().heard.push(applied);
        PENDING.told.store(true, Ordering::Release);
        after_step(&loader);
        assert!(
            PENDING.queue.relock().heard.is_empty(),
            "told after the step"
        );
        assert!(!PENDING.told.load(Ordering::Acquire));
        assert_eq!(PENDING.push(queued(PROGRESS_SET_LEVEL, 3)), Ok(()));
        game_ended(&loader);
        assert!(
            PENDING.queue.relock().waiting.is_empty(),
            "told at the game's end"
        );
        assert!(!PENDING.any.load(Ordering::Acquire));
    }

    #[test]
    fn a_write_held_once_then_in_no_level_step_is_dropped_for_the_last_step() {
        let pending = Writes::new();
        assert_eq!(pending.push(queued(PROGRESS_ADD_EXPERIENCE, 50)), Ok(()));
        // The first step holds it, the second does not take its character.
        pending.queue.relock().waiting[0].why = ProgressWriteData::UNREADABLE;
        assert!(pending.step_ended().is_empty());
        let heard = pending.step_ended();
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].1.outcome, ProgressWriteData::NO_LEVEL_STEP);
    }

    #[test]
    fn a_game_s_end_tells_every_write_and_keeps_none_for_the_next_game() {
        let pending = Writes::new();
        let applied =
            queued(PROGRESS_SET_LEVEL, 12).heard(ProgressWriteData::APPLIED, Left::default());
        pending.queue.relock().heard.push(applied);
        assert_eq!(pending.push(queued(PROGRESS_ADD_EXPERIENCE, 50)), Ok(()));
        let told = pending.game_ended();
        assert_eq!(
            told.iter().map(|(_, d)| d.outcome).collect::<Vec<_>>(),
            [ProgressWriteData::APPLIED, ProgressWriteData::NO_LEVEL_STEP]
        );
        assert!(!pending.any.load(Ordering::Acquire));
        assert!(pending.step_ended().is_empty());
        assert!(
            pending.step_ended().is_empty(),
            "nothing waits into the next game"
        );
        assert!(pending.queue.relock().waiting.is_empty());
    }

    #[test]
    fn an_applied_write_is_heard_after_the_step_with_what_it_left() {
        let pending = Writes::new();
        let w = queued(PROGRESS_SET_LEVEL, 12);
        let left = Left {
            level: 12,
            experience: 0,
            gained: 0,
        };
        let applied = w.heard(ProgressWriteData::APPLIED, left);
        pending.queue.relock().heard.push(applied);
        pending.told.store(true, Ordering::Release);
        let heard = pending.step_ended();
        assert_eq!(heard.len(), 1);
        let (owner, data) = heard[0];
        assert_eq!(owner, ModIndex::new(1));
        assert_eq!(data.header, Header::of::<ProgressWriteData>());
        assert_eq!(
            (data.outcome, data.what, data.value, data.level),
            (ProgressWriteData::APPLIED, PROGRESS_SET_LEVEL, 12, 12)
        );
        assert!(pending.step_ended().is_empty(), "heard once");
        assert!(!pending.any.load(Ordering::Acquire));
        assert!(!pending.told.load(Ordering::Acquire));
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
    fn writers_are_sealed_with_experience_filters_and_no_write_goes_in_once_experience_is_off() {
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
        let (filter, writer) = (ModIndex::new(0), ModIndex::new(1));
        super::super::EXPERIENCE_FILTERS
            .relock()
            .push(Registered::new(filter, no_pass, core::ptr::null_mut()));
        assert_eq!(WRITERS.add(writer), Ok(()));
        super::super::link(loader);
        assert!(WRITERS.sealed().is_some_and(|mods| mods.contains(&writer)));
        assert!(
            super::super::LEVEL_FUSE.is_off(),
            "a build without the level system lists experience off by its fuse"
        );
        assert_eq!(
            write(loader, writer, 7, PROGRESS_SET_LEVEL, 3),
            Err(Status::MISSING),
            "writes answer missing while the level step is not linked"
        );
        let listed = crate::engine::diag::parts_off();
        assert!(
            listed
                .iter()
                .any(|p| p.part == "experience" && p.reason.starts_with("player_level")),
            "{listed:?}"
        );
        // As if the level step had linked before its switch went off.
        PENDING.linked.store(true, Ordering::Relaxed);
        assert_eq!(
            write(loader, writer, 7, PROGRESS_SET_LEVEL, 3),
            Err(Status::MISSING),
            "no step applies a write once experience is off"
        );
    }

    #[test]
    fn an_unmatched_level_system_turns_experience_off_only_for_filters_not_for_writers() {
        static LEVEL: crate::engine::fuse::Fuse = crate::engine::fuse::Fuse::new("test experience");
        let loader = crate::start::tests::bare_loader();
        super::super::level_unmatched(&loader, &LEVEL, "a field moved", false);
        assert!(
            !LEVEL.is_off(),
            "every mod with a world asks for the writes, so they alone turn nothing off"
        );
        super::super::level_unmatched(&loader, &LEVEL, "a field moved", true);
        assert!(LEVEL.is_off(), "a filter a mod registered does");
    }
}
