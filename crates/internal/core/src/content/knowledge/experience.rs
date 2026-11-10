//! Experience values of a mod's own in the game's balancing table, all of a
//! mod's served by one patch of the table.

use std::sync::{Arc, Mutex};

use crate::abi::ResourceKey;
use crate::engine_enum::Values;
use crate::game_events::ExperienceSourceType;
use crate::locks::Relock as _;
use crate::resources::{Registry, Type, Value};
use crate::{Host, Load, Off};

use super::super::{EngineFacts as _, Id};
use super::{ExperienceValue, hash_key, key_of_hash, serve_list};

const EXPERIENCE_VALUES: &str = "experienceValues";
const SOURCE: &str = "source";
const EXPERIENCE_VALUE_ID: &str = "experienceValueId";
const EXPERIENCE_GAIN: &str = "experienceGain";

/// A new experience value for [`Load::add_experience`]: the experience of
/// one source that a knowledge entry or a quest reward gives.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::id;
/// use enshwick::content::knowledge::{NewExperience, NewKnowledge};
/// use enshwick::game_events::ExperienceSourceType;
/// use enshwick::knowledge::{KnowledgeType, Scope};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let found = load.add_experience(NewExperience::new(id!("shrine-xp"), ExperienceSourceType::Exploration, 250))?;
///     load.add_knowledge(NewKnowledge {
///         experience: Some(found),
///         ..NewKnowledge::new(id!("found-the-shrine"), Scope::Player, KnowledgeType::Bool)
///     })?;
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct NewExperience {
    /// Its content id; its experience value is the FNV-1a of the id's GUID.
    pub id: Id,
    /// The source the game counts it under; the world's experience factor
    /// of the source's [`ExperienceGroup`](crate::rates::ExperienceGroup)
    /// scales it, and a source in no group has none.
    pub source: ExperienceSourceType,
    /// The experience it gives.
    pub gain: u32,
}

impl NewExperience {
    /// The experience value `id` that gives `gain` experience of `source`.
    ///
    /// # Examples
    ///
    /// See [`NewExperience`].
    #[must_use]
    pub const fn new(id: Id, source: ExperienceSourceType, gain: u32) -> Self {
        Self { id, source, gain }
    }

    /// Its experience value.
    pub(super) fn value(self) -> ExperienceValue {
        ExperienceValue(self.id.guid().fnv1a())
    }

    /// The game's balance value of it, with the engine's value `source` of
    /// its source.
    fn to_value(self, source: u64) -> Value {
        Value::Struct(vec![
            (SOURCE.into(), Value::Uint(source)),
            (EXPERIENCE_VALUE_ID.into(), hash_key(self.value().get())),
            (EXPERIENCE_GAIN.into(), Value::Uint(self.gain.into())),
        ])
    }
}

/// The experience values a mod added, which one patch of the balancing
/// table serves after the table's own, read when the mod adds its first.
#[derive(Debug)]
pub(crate) struct AddedExperience {
    /// The ids of the game's own values, sorted.
    vanilla: Box<[u32]>,
    /// The engine value of each named source.
    sources: Values<ExperienceSourceType, 7>,
    /// The values of the build's source enum, for a source the crate does
    /// not name.
    reflected: Box<[u64]>,
    /// The mod's values by id, in the order it added them.
    added: Arc<Mutex<Vec<(u32, Value)>>>,
}

impl AddedExperience {
    /// Reads the game's values and serves the patch that adds the mod's.
    fn serve(host: Host) -> Result<Self, Off> {
        let resource = balancing_table(host)?;
        let mut vanilla: Vec<u32> = values_of(&host.game_value(resource)?)?
            .iter()
            .filter_map(|value| value.field(EXPERIENCE_VALUE_ID).and_then(key_of_hash))
            .collect();
        vanilla.sort_unstable();
        let added = serve_list(host, resource, add_experience_to)?;
        Ok(Self {
            vanilla: vanilla.into(),
            sources: Values::resolve(&host, ExperienceSourceType::IDS),
            reflected: source_type(host)
                .map(|ty| {
                    ty.constants()
                        .iter()
                        .map(crate::resources::Constant::value)
                        .collect()
                })
                .unwrap_or_default(),
            added,
        })
    }

    /// The engine value of `source`: a named source's constant, or the
    /// number of another the build's enum holds; `None` when the build
    /// lacks it.
    fn source_value(&self, source: ExperienceSourceType) -> Option<u64> {
        match source {
            ExperienceSourceType::Other(raw) => {
                let raw = u64::from(raw);
                self.reflected.contains(&raw).then_some(raw)
            }
            named => self.sources.value(named),
        }
    }
}

/// Adds `new` to the experience values the mod `load` serves.
pub(super) fn add(load: &mut Load<'_>, new: NewExperience) -> Result<ExperienceValue, Off> {
    let host = load.host();
    let experience = match &mut load.experience {
        Some(experience) => experience,
        empty => empty.insert(AddedExperience::serve(host)?),
    };
    let source = experience.source_value(new.source).ok_or_else(|| {
        Off::new(format!(
            "experience {}: the build lacks the source {:?}",
            new.id, new.source
        ))
    })?;
    let key = new.value().get();
    if experience.vanilla.binary_search(&key).is_ok() {
        return Err(Off::new(format!(
            "experience {}: the game holds it",
            new.id
        )));
    }
    let added = Arc::clone(&experience.added);
    load.declare(new.id)?;
    let value = new.to_value(source);
    let mut added = added.relock();
    match added.iter_mut().find(|(held, _)| *held == key) {
        Some(held) => held.1 = value,
        None => added.push((key, value)),
    }
    Ok(new.value())
}

/// The reflected enum of experience sources.
pub(crate) fn source_type(host: Host) -> Option<&'static Type<'static>> {
    let hash = host.hash(crate::engine_ids::EXPERIENCE_SOURCE).ok()?;
    Registry::game().ok()?.hashed(hash)
}

/// The key of the game's balancing table, the one resource of its type.
pub(crate) fn balancing_table(host: Host) -> Result<ResourceKey, Off> {
    let hash = host.hash(crate::engine_ids::BALANCING_TABLE)?;
    let registry = Registry::game()?;
    let ty = registry
        .hashed(hash)
        .ok_or_else(|| Off::new("balancing: the table's type is not reflected"))?;
    match host.resource_keys(Some(ty))?.as_slice() {
        [one] => Ok(*one),
        keys => Err(Off::new(format!(
            "balancing: {} tables in the archive, not one",
            keys.len()
        ))),
    }
}

/// The experience values of the balancing table `table`.
pub(crate) fn values_of(table: &Value) -> Result<&[Value], Off> {
    match table.field(EXPERIENCE_VALUES) {
        Some(Value::List(values)) => Ok(values),
        _ => Err(Off::new("balancing: no list of experience values")),
    }
}

/// The value id, source and gain of the balance value `value`, its source
/// as the decoder gives it.
pub(crate) fn balance_parts(value: &Value) -> Option<(u32, &Value, u32)> {
    let id = value.field(EXPERIENCE_VALUE_ID).and_then(key_of_hash)?;
    let source = value.field(SOURCE)?;
    let gain = match value.field(EXPERIENCE_GAIN)? {
        Value::Uint(gain) => u32::try_from(*gain).ok()?,
        _ => return None,
    };
    Some((id, source, gain))
}

/// `table` with the balance values `added`, each by its id, after its
/// experience values, which the game keeps in no order of id; the server
/// builds the table with them (server 224, read on a server copy). An id the
/// table already holds is refused: the game's own are refused at the add,
/// so it is another mod's.
fn add_experience_to(mut table: Value, added: &[(u32, Value)]) -> Result<Value, Off> {
    let Some(Value::List(values)) = table.field_mut(EXPERIENCE_VALUES) else {
        return Err(Off::new("balancing: no list of experience values"));
    };
    let mut held: Vec<u32> = values
        .iter()
        .filter_map(|value| value.field(EXPERIENCE_VALUE_ID).and_then(key_of_hash))
        .collect();
    held.sort_unstable();
    if let Some((key, _)) = added
        .iter()
        .find(|(key, _)| held.binary_search(key).is_ok())
    {
        return Err(Off::new(format!(
            "balancing: experience value {key} is in the table already"
        )));
    }
    values.extend(added.iter().map(|(_, value)| value.clone()));
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn balance(id: u32, gain: u64) -> Value {
        Value::Struct(vec![
            (SOURCE.into(), Value::Name("Mining".into())),
            (EXPERIENCE_VALUE_ID.into(), hash_key(id)),
            (EXPERIENCE_GAIN.into(), Value::Uint(gain)),
        ])
    }

    #[test]
    fn a_mod_s_values_follow_the_table_s_own_in_the_order_added() {
        let new = NewExperience::new(Id::new("you.lore:xp"), ExperienceSourceType::Quest, 1200);
        let table = Value::Struct(vec![(
            EXPERIENCE_VALUES.into(),
            Value::List(vec![balance(5, 10)]),
        )]);
        let key = new.value().get();
        let added =
            add_experience_to(table, &[(key, new.to_value(3)), (9, balance(9, 1))]).unwrap();
        let values = values_of(&added).unwrap();
        let ids: Vec<u32> = values
            .iter()
            .filter_map(|v| balance_parts(v).map(|(id, ..)| id))
            .collect();
        assert_eq!(ids, [5, key, 9]);
        assert_eq!(
            balance_parts(&values[1]),
            Some((key, &Value::Uint(3), 1200))
        );
        assert!(values_of(&Value::Struct(Vec::new())).is_err());
        assert!(add_experience_to(Value::Struct(Vec::new()), &[]).is_err());
    }

    #[test]
    fn a_value_whose_id_the_table_already_holds_is_refused_not_overwritten() {
        let table = Value::Struct(vec![(
            EXPERIENCE_VALUES.into(),
            Value::List(vec![balance(5, 10), balance(7, 3)]),
        )]);
        let why = add_experience_to(table, &[(8, balance(8, 1)), (7, balance(7, 99))])
            .unwrap_err()
            .to_string();
        assert!(why.contains("experience value 7"), "{why}");
    }

    #[test]
    fn an_unnamed_source_counts_only_when_the_build_s_enum_holds_it() {
        let experience = AddedExperience {
            vanilla: Box::new([]),
            sources: Values::new([
                (ExperienceSourceType::None, Some(0)),
                (ExperienceSourceType::Exploration, Some(1)),
                (ExperienceSourceType::Lore, Some(2)),
                (ExperienceSourceType::Quest, Some(7)),
                (ExperienceSourceType::Combat, None),
                (ExperienceSourceType::Mining, Some(5)),
                (ExperienceSourceType::ItemSetCompleted, Some(6)),
            ]),
            reflected: Box::new([0, 1, 2, 5, 6, 7, 8]),
            added: Arc::default(),
        };
        assert_eq!(
            experience.source_value(ExperienceSourceType::Quest),
            Some(7)
        );
        assert_eq!(
            experience.source_value(ExperienceSourceType::Combat),
            None,
            "the build lacks it"
        );
        assert_eq!(
            experience.source_value(ExperienceSourceType::Other(8)),
            Some(8)
        );
        assert_eq!(
            experience.source_value(ExperienceSourceType::Other(200)),
            None
        );
    }
}
