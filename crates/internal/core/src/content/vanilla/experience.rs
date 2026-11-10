//! The game's own experience values, read from its balancing table.

use crate::Off;
use crate::content::knowledge::{
    ExperienceValue, balance_parts, balancing_table, source_type, values_of,
};
use crate::engine_enum::Values;
use crate::game_events::ExperienceSourceType;
use crate::resources::Value;

use super::VanillaError;

/// A vanilla experience value, from [`experience_values`]: the experience of
/// one source that a knowledge entry or a quest reward names.
///
/// # Examples
///
/// See [`experience_values`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct ExperienceBalanceValue {
    /// The value, as a knowledge entry or a quest reward names it.
    pub value: ExperienceValue,
    /// The source the game counts it under.
    pub source: ExperienceSourceType,
    /// The experience it gives.
    pub gain: u32,
}

/// The running game's vanilla experience values, in the balancing table's
/// order: what a mod's knowledge entry or quest reward may name in place of
/// a value of its own from [`crate::Load::add_experience`].
///
/// # Errors
///
/// [`VanillaError::Unreadable`] outside the game and when the balancing
/// table or one of its rows does not read; the log names why.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::vanilla;
/// use enshwick::game_events::ExperienceSourceType;
///
/// if let Ok(values) = vanilla::experience_values() {
///     let quests = values.iter().filter(|value| value.source == ExperienceSourceType::Quest);
///     enshwick::info!("{} quest rewards of experience", quests.count());
/// }
/// ```
pub fn experience_values() -> Result<Vec<ExperienceBalanceValue>, VanillaError> {
    let host = crate::host().ok_or(VanillaError::Unreadable("data"))?;
    let unreadable = |off: Off| {
        crate::warn!("vanilla experience values: {off}");
        VanillaError::Unreadable("balancing table")
    };
    let table = host
        .game_value(balancing_table(host).map_err(unreadable)?)
        .map_err(unreadable)?;
    let sources = Values::resolve(&host, ExperienceSourceType::IDS);
    let named = source_type(host);
    let raw = |source: &Value| match source {
        Value::Uint(raw) => u8::try_from(*raw).ok(),
        Value::Name(name) => named?
            .constants()
            .iter()
            .find(|constant| constant.name() == name)
            .and_then(|constant| u8::try_from(constant.value()).ok()),
        _ => None,
    };
    values_of(&table)
        .map_err(unreadable)?
        .iter()
        .map(|value| {
            let (id, source, gain) = balance_parts(value)
                .ok_or_else(|| Off::new(format!("a malformed row {value:?}")))?;
            let raw =
                raw(source).ok_or_else(|| Off::new(format!("an unknown source {source:?}")))?;
            Ok(ExperienceBalanceValue {
                value: ExperienceValue::new(id),
                source: sources
                    .variant(raw.into())
                    .unwrap_or(ExperienceSourceType::Other(raw)),
                gain,
            })
        })
        .collect::<Result<_, Off>>()
        .map_err(unreadable)
}
