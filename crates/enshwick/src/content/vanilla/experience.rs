//! The game's own experience values, read from its balancing table.

use crate::content::EngineFacts as _;
use crate::content::knowledge::{
    ExperienceValue, balance_parts, balancing_table, experience_values as values_of, source_of,
};
use crate::game_events::ExperienceSourceType;
use crate::resources::{Registry, Value};

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
/// table does not read.
///
/// # Examples
///
/// ```
/// use enshwick::content::vanilla;
/// use enshwick::game_events::ExperienceSourceType;
///
/// if let Ok(values) = vanilla::experience_values() {
///     let quests = values.iter().filter(|value| value.source == ExperienceSourceType::Quest);
///     enshwick::info!("{} quest rewards of experience", quests.count());
/// }
/// ```
pub fn experience_values() -> Result<Vec<ExperienceBalanceValue>, VanillaError> {
    let unreadable = |_| VanillaError::Unreadable("balancing table");
    let host = crate::host().ok_or(VanillaError::Unreadable("data"))?;
    let table = host
        .game_value(balancing_table(host).map_err(unreadable)?)
        .map_err(unreadable)?;
    let sources = host
        .hash(crate::engine_ids::EXPERIENCE_SOURCE)
        .ok()
        .and_then(|hash| Registry::game().ok()?.hashed(hash));
    let raw = |source: &Value| match source {
        Value::Uint(raw) => u8::try_from(*raw).ok(),
        Value::Name(name) => sources?
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
            let (id, source, gain) = balance_parts(value)?;
            Some(ExperienceBalanceValue {
                value: ExperienceValue::new(id),
                source: source_of(&host, raw(source)?),
                gain,
            })
        })
        .collect::<Option<_>>()
        .ok_or(VanillaError::Unreadable("balancing table"))
}
