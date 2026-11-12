//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Entity;
use crate::abi::{
    Comfort as RawComfort, ExperiencePass, ExperienceRow, SkillCheck as RawSkillCheck,
};
use crate::{Load, Off};
use core::ffi::c_void;
use enshwick_knowledge::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(progress_hooks)]
impl Load<'_> {
    /// Runs `filter` on each comfort a provider registers, as
    /// `Load::on_comfort` does: the comfort it answers registers; nil keeps
    /// the comfort as it was.
    ///
    /// # Errors
    ///
    /// As [`Load::on_comfort`].
    #[script(name = "on_comfort")]
    pub(crate) fn on_comfort_script(
        &mut self,
        filter: impl Fn(Comfort) -> Option<i32> + Send + Sync + 'static,
    ) -> Result<(), Off>;

    /// Runs `filter` on each tick's experience with the level table, as
    /// `Load::on_experience` does: it returns the changed list, or nil to
    /// keep it.
    ///
    /// # Errors
    ///
    /// As [`Load::on_experience`].
    #[script(name = "on_experience")]
    pub(crate) fn on_experience_script(
        &mut self,
        filter: impl Fn(Vec<Experience>, ScriptLevels) -> Option<Vec<Experience>>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), Off>;
}

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
