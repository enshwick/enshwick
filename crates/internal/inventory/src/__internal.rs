//! What core calls inside this crate and a mod must not: the objects core's
//! `Load` hands a script's handlers, and the crate-private parts the
//! domains above build on, such as a data pack's loot.

/// The damage a script's filter gets from `Load::on_damage_script`.
#[cfg(feature = "unstable")]
pub use crate::damage::script::ScriptDamage;
/// The lookup a script's handler gets from `Load::on_survivor_lookup_script`.
pub use crate::survivors::script::ScriptLookup;

/// Replaces the drops of `group`, a data pack's or a mod's, with `drops`.
///
/// # Errors
///
/// The [`ReplaceDropsError`](crate::loot::ReplaceDropsError) of a drop
/// that does not resolve.
pub fn replace_loot_group(
    group: u64,
    drops: Vec<crate::loot::Drop>,
) -> Result<(), crate::loot::ReplaceDropsError> {
    crate::loot::replace_group(group, drops)
}
