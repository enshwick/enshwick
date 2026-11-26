//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
use crate::quests::Quests;
use enshwick_knowledge::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The [`Quests`] of the running game; every call of one mod gives the
    /// same, which reads the journal once.
    ///
    /// Unstable: needs the `typed` feature and may change in any release.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> enshwick::quests::Quests {
    ///     load.quests()
    /// }
    /// ```
    #[must_use]
    pub fn quests(&mut self) -> Quests;
}

#[enshwick_macros::forward(base)]
#[crate::script(quest_rewards)]
impl Load<'_> {
    /// Runs `filter` before the game pays a player for a quest, on the
    /// thread of the game's quest reward system, in load order after the
    /// filters of the mods before: it sees the quest's experience values as
    /// they left them, and keeps them, pays others or refuses, which pays
    /// nothing and still completes the quest. A payment the game puts off
    /// comes again, and the filter sees it again.
    ///
    /// Unstable: needs the `typed` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) after init, and on a build where the loader does
    /// not find the game's payment.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::quests::QuestPay;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Every quest pays its experience twice.
    ///     load.on_quest_reward(|paid| {
    ///         QuestPay::Pay(paid.experience.iter().flat_map(|&value| [value, value]).collect())
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_quest_reward<F>(&mut self, filter: F) -> Result<(), crate::Off>
    where
        F: Fn(&crate::quests::QuestPayment) -> crate::quests::QuestPay + Send + Sync + 'static;
}
