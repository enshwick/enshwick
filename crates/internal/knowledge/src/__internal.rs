//! What core calls inside this crate and a mod must not: the objects core's
//! `Load` hands a script's handlers, and the crate-private parts the
//! domains above build on, such as a data pack's rewards.

/// Starts counting the objectives of the quest steps the mod added since
/// the last call, which `Load::add_quest` calls once the content domain has
/// given each its counters.
///
/// # Errors
///
/// [`Off`](crate::Off) when an event the objectives count cannot be
/// subscribed to.
#[cfg(feature = "typed")]
pub fn count_objectives(load: &mut crate::BaseLoad) -> Result<(), crate::Off> {
    let steps = enshwick_content::__internal::take_counted_steps(load);
    crate::quests::objectives::count(load, steps)
}

/// The per-event check of quest objectives, for the benchmark.
///
/// The table holds `objectives` build objectives on items 1 to
/// `objectives`; the closure answers how many count the item it gets.
#[cfg(feature = "typed")]
pub fn objective_lookup(objectives: u32) -> impl Fn(crate::ItemId) -> usize {
    crate::quests::objectives::lookup(objectives)
}

/// The level table a script's filter gets from `Load::on_experience_script`.
pub use crate::progress::script::ScriptLevels;

/// Puts `reward` in place of mod `me`'s declared reward of its name, for a
/// pack whose numbers follow the world's settings.
#[cfg(feature = "unstable")]
pub fn replace_reward(me: crate::abi::ModHandle, reward: crate::rewards::Reward) {
    crate::rewards::replace(me, reward);
}
