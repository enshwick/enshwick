//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::progress::{Comfort, Experience, Levels, Skill, SkillCheck};
use crate::{Load, Off};
use enshwick_knowledge::__load::*;

mod script;

#[enshwick_macros::forward(base)]
#[crate::script(progress)]
impl Load<'_> {
    /// Answers the game's checks of `skill`: `Some(true)` unlocks it for
    /// that check, `Some(false)` locks it, `None` keeps the game's answer. It
    /// runs on both sides, on the checking thread, and only for `skill`.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, on a build whose table lacks the skill's node, or
    /// on a build that lacks the check.
    ///
    /// # Examples
    ///
    /// See the [module](crate::progress).
    #[script]
    pub fn on_skill<F>(&mut self, skill: Skill, handler: F) -> Result<(), Off>
    where
        F: Fn(&SkillCheck) -> Option<bool> + Send + Sync + 'static;

    /// Changes the comfort each provider registers in a base: `filter`
    /// returns the value to register. Server's worker threads, on the hot
    /// path of every base: keep it to arithmetic.
    ///
    /// # Errors
    ///
    /// As for [`Load::on_skill`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_comfort(|comfort| comfort.value.saturating_mul(2))
    /// }
    /// ```
    pub fn on_comfort<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Comfort) -> i32 + Send + Sync + 'static;

    /// Runs `filter` on every spawned character's experience before the
    /// server applies it; changes to `gained` are written back and sent to
    /// the players. Server's worker threads, once per batch.
    ///
    /// The engine runs its experience system several times a tick, one batch
    /// of characters each, so `filter` runs on every tick, most often with
    /// nothing gained: return early when no row gained anything, and never
    /// allocate or lock on the way.
    ///
    /// # Errors
    ///
    /// As for [`Load::on_skill`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Double every gain, never past the end of the level.
    ///     load.on_experience(|rows, _levels| {
    ///         for row in rows {
    ///             let room = row.need.saturating_sub(row.progress);
    ///             row.gained = row.gained.saturating_mul(2).min(room.max(row.gained));
    ///         }
    ///     })
    /// }
    /// ```
    pub fn on_experience<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut [Experience], &Levels) + Send + Sync + 'static;
}
