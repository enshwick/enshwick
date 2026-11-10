//! The generated values the API crate builds whole: a generated struct is
//! non-exhaustive, so only this crate can write one out field by field.

use crate::content::Guid;
use crate::typed::ecs::{RuntimeSpawnerEntry, SpawnConditionFlags, TemplateReference};
use crate::typed::{Float, Time};

/// The game's `Time` of `nanos` nanoseconds.
#[must_use]
pub const fn time(nanos: i64) -> Time {
    Time { value: nanos }
}

/// An entry of a random spawner for `templates`, never a high-priority one.
#[must_use]
pub const fn spawner_entry(
    templates: Vec<TemplateReference>,
    group: Guid,
    conditions: SpawnConditionFlags,
    weight: Float,
) -> RuntimeSpawnerEntry {
    RuntimeSpawnerEntry {
        entity_templates: templates,
        spawn_group_id: group,
        spawn_conditions: conditions,
        weight,
        is_high_priority_entry: false,
    }
}
