//! What core calls inside this crate and a mod must not: the objects core's
//! `Load` hands a script's handlers, and the crate-private parts the
//! domains above build on, such as a data pack's rewards.

/// Starts counting the objectives of the quest steps the mod added since
/// the last call, as `Load::add_quest` asks.
///
/// The content domain has given each step its counters and the quest is in
/// the journal by then, so a kind of objective that cannot count is logged,
/// and only it is off.
#[cfg(feature = "typed")]
pub fn count_objectives(load: &mut crate::BaseLoad) {
    let steps = enshwick_content::__internal::take_counted_steps(load);
    let quest = steps.first().map(|step| step.quest);
    if let (Err(off), Some(quest)) = (crate::quests::objectives::count(load, steps), quest) {
        crate::warn!("quest {quest}: some objectives do not count: {off}");
    }
}

/// The per-event check of quest objectives, for the benchmark.
///
/// The table holds `objectives` build objectives on items 1 to
/// `objectives`; the closure answers how many count the item it gets.
#[cfg(feature = "typed")]
pub fn objective_lookup(objectives: u32) -> impl Fn(crate::ItemId) -> usize {
    crate::quests::objectives::lookup(objectives)
}

/// The quests of the journal the game serves now, read through `host`, for
/// the loader's quest events.
///
/// # Errors
///
/// [`Off`](crate::Off) when the journal does not read.
#[cfg(feature = "typed")]
pub fn journal_quests(host: crate::Host) -> Result<Vec<crate::quests::QuestInfo>, crate::Off> {
    crate::quests::journal_quests(host)
}

/// The level table a script's filter gets from `Load::on_experience_script`.
pub use crate::progress::script::ScriptLevels;

/// Puts `reward` in place of mod `me`'s declared reward of its name, for a
/// pack whose numbers follow the world's settings.
#[cfg(feature = "unstable")]
pub fn replace_reward(me: crate::abi::ModHandle, reward: crate::rewards::Reward) {
    crate::rewards::replace(me, reward);
}
