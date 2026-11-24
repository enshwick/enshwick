//! Level and experience writes, applied in the fake's level step as the
//! loader applies them: each player's level, the experience inside it and
//! the gain the step applies, and what each writing mod hears.

use enshwick_core::Entity;
use enshwick_core::abi::{
    Event, Header, ModHandle, PROGRESS_SET_EXPERIENCE, PROGRESS_SET_LEVEL, ProgressWriteData,
};
use rustc_hash::FxHashMap;

use crate::test::{FakeHost, table};

/// The writes the loader holds for its next level step at most.
pub(crate) const MOST_WRITES: usize = 256;

/// The game's level cap and what each level needs, as the fake plays them.
pub(crate) type LevelTable = (u32, fn(u32) -> u32);

/// A level or experience write that waits for the level step.
#[derive(Clone, Copy, Debug)]
struct Waiting {
    owner: ModHandle,
    entity: Entity,
    what: u32,
    value: u32,
    /// The fake step it was asked in: the level step of a later one takes
    /// it.
    asked: u64,
    /// A step's end passed since it was asked: the next end drops it.
    seen: bool,
}

impl Waiting {
    /// What its mod hears: `outcome`, with what an applied write `left`.
    fn heard(self, outcome: u32, left: Progress) -> (ModHandle, ProgressWriteData) {
        let data = ProgressWriteData {
            header: Header::of::<ProgressWriteData>(),
            entity: self.entity,
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

/// A character's level, the experience inside it and the gain the step
/// applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Progress {
    level: u32,
    experience: u32,
    gained: u32,
}

impl Progress {
    /// What a character starts with: level 1, nothing gained.
    const START: Self = Self {
        level: 1,
        experience: 0,
        gained: 0,
    };

    /// What nothing left: an outcome that applied no write.
    const NONE: Self = Self {
        level: 0,
        experience: 0,
        gained: 0,
    };

    /// `w` written on the character, as the loader's level step writes it.
    fn written(&mut self, w: &Waiting, (cap, need_for): LevelTable) {
        match w.what {
            PROGRESS_SET_LEVEL => {
                *self = Self {
                    level: w.value.clamp(1, cap.max(1)),
                    ..Self::START
                };
            }
            PROGRESS_SET_EXPERIENCE => {
                self.experience = w.value.min(need_for(self.level).saturating_sub(1));
                self.gained = 0;
            }
            _ => self.gained = self.gained.saturating_add(w.value),
        }
    }
}

/// The level step's state: the writes that wait and each written
/// character's progress.
#[derive(Debug, Default)]
pub(crate) struct ProgressState {
    waiting: Vec<Waiting>,
    characters: FxHashMap<u32, Progress>,
}

impl ProgressState {
    /// Queues a write of mod `owner` asked in fake step `asked`, while
    /// fewer than [`MOST_WRITES`] wait; `false` when full.
    pub(crate) fn push(
        &mut self,
        owner: ModHandle,
        (entity, what, value): (Entity, u32, u32),
        asked: u64,
    ) -> bool {
        if self.waiting.len() >= MOST_WRITES {
            return false;
        }
        self.waiting.push(Waiting {
            owner,
            entity,
            what,
            value,
            asked,
            seen: false,
        });
        true
    }
}

impl FakeHost {
    /// The game's level cap and what each level needs, which the fake's
    /// level step writes with: a level written past `cap` is `cap`, and the
    /// experience written inside a level stays below `need(level)`. Without
    /// it no level is capped and no level needs a limit.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .level_table(25, |level| level * 1000)
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn level_table(mut self, cap: u32, need: fn(u32) -> u32) -> Self {
        self.world.level_table = Some((cap, need));
        self
    }
}

/// Writes each waiting write whose character is in the step, at the end of
/// each fake step, then tells the writing mod what became of it, as the
/// loader's level step does: those applied first, then those whose
/// character was in no level step since the step before.
pub(crate) fn after_step() {
    let heard = table::with(Vec::new(), |s| {
        let table = s.host.world.level_table.unwrap_or((u32::MAX, |_| u32::MAX));
        let in_step: Vec<Entity> = s.host.base.players.iter().map(|p| p.entity).collect();
        let step = s.base.ticks.step;
        let ProgressState {
            waiting,
            characters,
        } = &mut s.world.progress;
        let (mut applied, mut dropped) = (Vec::new(), Vec::new());
        waiting.retain_mut(|w| {
            if w.asked >= step {
                true
            } else if in_step.contains(&w.entity) {
                let progress = characters.entry(w.entity.0).or_insert(Progress::START);
                progress.written(w, table);
                applied.push(w.heard(ProgressWriteData::APPLIED, *progress));
                false
            } else if w.seen {
                dropped.push(w.heard(ProgressWriteData::NO_LEVEL_STEP, Progress::NONE));
                false
            } else {
                w.seen = true;
                true
            }
        });
        // The game applies the gains after the level step, so the next step
        // starts with none waiting.
        // shortcut: the fake plays no game rule that folds a gain into the
        // experience; upgrade when a mod test needs the level a gain reaches.
        for progress in characters.values_mut() {
            progress.gained = 0;
        }
        applied.extend(dropped);
        applied
    });
    tell(heard);
}

/// Tells each writing mod of every write still waiting as a hosted game
/// ends, since its character goes with the game, as the loader does: what
/// the handlers ask for while hearing it goes with the game too, and the
/// next game's characters start afresh.
pub(crate) fn game_ended() {
    let heard = table::with(Vec::new(), |s| {
        s.world.progress.characters.clear();
        s.world
            .progress
            .waiting
            .drain(..)
            .map(|w| w.heard(ProgressWriteData::NO_LEVEL_STEP, Progress::NONE))
            .collect()
    });
    tell(heard);
    table::with((), |s| s.world.progress.waiting.clear());
}

/// Raises each outcome for the mod it belongs to alone.
fn tell(heard: Vec<(ModHandle, ProgressWriteData)>) {
    for (owner, data) in heard {
        // SAFETY: `data` is a `ProgressWriteData`, the event's data, alive
        // for the call.
        let _ran = unsafe {
            crate::test::base::events::dispatch_to(
                Event::PROGRESS_WRITTEN,
                (&raw const data).cast(),
                Some(owner),
            )
        };
    }
}
