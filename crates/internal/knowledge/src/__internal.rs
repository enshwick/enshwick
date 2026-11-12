//! What core calls inside this crate and a mod must not: the objects core's
//! `Load` hands a script's handlers, and the crate-private parts the
//! domains above build on, such as a data pack's rewards.

/// The level table a script's filter gets from `Load::on_experience_script`.
pub use crate::progress::script::ScriptLevels;

/// Puts `reward` in place of mod `me`'s declared reward of its name, for a
/// pack whose numbers follow the world's settings.
#[cfg(feature = "unstable")]
pub fn replace_reward(me: crate::abi::ModHandle, reward: crate::rewards::Reward) {
    crate::rewards::replace(me, reward);
}
