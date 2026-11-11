//! Spawns from mods: enemies added to the game's random spawners, and the
//! spawners' counts and timeouts changed.
//!
//! A random spawner is a template whose spawner component lists the
//! templates it spawns, each with a weight. [`Load::add_spawn_entry`] adds
//! one, a vanilla enemy or a mod's own from
//! [`Load::add_enemy`](crate::Load::add_enemy), and
//! [`Load::change_spawner`] changes how many it spawns and how often. Both are
//! [template changes](super::templates): in memory, in load order. Spawns
//! are the server's: a mod that adds only vanilla enemies to spawners is a
//! server mod.
//!
//! ```
//! use enshwick::content::Guid;
//! use enshwick::content::spawns::{SpawnEntry, SpawnerPick};
//!
//! const WOLF: Guid = Guid::parse("0d75b9a9-7e48-4f3a-bf2a-19bf6021f531");
//!
//! fn init(load: &mut enshwick::Load<'_>, elder: Guid) -> Result<(), enshwick::Off> {
//!     // Elder wolves now and then where the game spawns wolves.
//!     load.add_spawn_entry(SpawnEntry { weight: 0.25, ..SpawnEntry::new(SpawnerPick::Listing(WOLF), elder) })
//! }
//! ```

use crate::resources::Value;
use crate::typed::ecs::{RandomSpawner, SpawnConditionFlags};
use crate::typed::{Flags, Named as _};
use crate::{Load, Off};

use super::templates::ComponentChange;
use super::{Blob, EngineFacts, Guid, types};

/// A `BlobVariant` in a blob: type hash, offset, size.
const VARIANT: usize = 12;

/// Nanoseconds in a second, the unit of the game's `Time`.
const NANOS: f64 = 1e9;

/// The random spawners a change reaches.
///
/// # Examples
///
/// See [`SpawnEntry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub enum SpawnerPick {
    /// The vanilla spawner template with this GUID, such as one
    /// [`vanilla::template`](super::vanilla::template) names.
    Spawner(Guid),
    /// Every vanilla spawner template that lists this template, so a mod
    /// spawns its enemy where the vanilla one spawns.
    Listing(Guid),
}

/// A template a spawner spawns, for [`Load::add_spawn_entry`].
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::spawns::{SpawnEntry, SpawnerPick};
///
/// let entry = SpawnEntry::new(SpawnerPick::Spawner(Guid([1; 16])), Guid([2; 16]));
/// assert!((entry.weight - 1.0).abs() < f32::EPSILON && entry.group.is_none());
/// ```
#[derive(Clone, Debug, PartialEq, crate::Script)]
pub struct SpawnEntry {
    /// The spawners that get the entry.
    pub spawner: SpawnerPick,
    /// The template spawned.
    pub template: Guid,
    /// How likely the spawner picks it, against the weights of its other
    /// entries, which are 1 in most vanilla spawners.
    pub weight: f32,
    /// The spawn group it belongs to; `None` for none.
    pub group: Option<Guid>,
    /// The conditions it spawns under, such as only at night; `None` for
    /// any.
    pub conditions: Option<SpawnConditionFlags>,
}

impl SpawnEntry {
    /// An entry of `template` in `spawner`, of weight 1, under any
    /// condition.
    ///
    /// # Examples
    ///
    /// See [`SpawnEntry`].
    #[must_use]
    pub const fn new(spawner: SpawnerPick, template: Guid) -> Self {
        Self {
            spawner,
            template,
            weight: 1.0,
            group: None,
            conditions: None,
        }
    }
}

/// New counts and timeouts of a spawner, for [`Load::change_spawner`];
/// `None` keeps a value.
///
/// # Examples
///
/// ```
/// use enshwick::content::spawns::SpawnerChange;
///
/// let crowded = SpawnerChange { max_entities_alive: Some(12), ..SpawnerChange::default() };
/// assert!(crowded.max_spawn_amount.is_none());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, crate::Script)]
pub struct SpawnerChange {
    /// The fewest it spawns at once.
    pub min_spawn_amount: Option<u8>,
    /// The most it spawns at once.
    pub max_spawn_amount: Option<u8>,
    /// The most of its spawns alive at a time.
    pub max_entities_alive: Option<u8>,
    /// The most it spawns in all.
    pub max_total_spawn_count: Option<u8>,
    /// The shortest wait between spawns, in seconds.
    pub min_spawn_timeout: Option<f64>,
    /// The longest wait between spawns, in seconds.
    pub max_spawn_timeout: Option<f64>,
}

impl SpawnerChange {
    /// The spawner value `spawner` with the change made, by the game's
    /// field names.
    fn apply(&self, spawner: &mut Value) {
        for (field, count) in [
            (MIN_AMOUNT, self.min_spawn_amount),
            (MAX_AMOUNT, self.max_spawn_amount),
            (MAX_ALIVE, self.max_entities_alive),
            (MAX_TOTAL, self.max_total_spawn_count),
        ] {
            if let (Some(count), Some(held)) = (count, spawner.field_mut(field)) {
                *held = Value::Uint(count.into());
            }
        }
        for (field, seconds) in [
            (MIN_TIMEOUT, self.min_spawn_timeout),
            (MAX_TIMEOUT, self.max_spawn_timeout),
        ] {
            if let (Some(seconds), Some(held)) = (seconds, spawner.field_mut(field)) {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a wait of seconds fits a 64-bit count of nanoseconds"
                )]
                let nanos = (seconds.max(0.0) * NANOS) as i64;
                *held =
                    crate::typed::Reflected::to_value(&crate::typed::runtime::made::time(nanos));
            }
        }
    }
}

fn lists(template: &Value, spawner: &str, listed: Guid) -> bool {
    let Some(Value::List(components)) = template.field("components") else {
        return false;
    };
    components.iter().any(|held| match held {
        Value::Variant(held) if held.type_name() == spawner => match held.value().field(ENTRIES) {
            Some(Value::List(entries)) => entries.iter().any(|entry| {
                matches!(entry.field(ENTRY_TEMPLATES), Some(Value::List(templates))
                        if templates.contains(&Value::Guid(listed)))
            }),
            _ => false,
        },
        _ => false,
    })
}

/// The spawner value `spawner` with `entry` after its entries.
fn add_entry(spawner: &mut Value, entry: &Value) {
    if let Some(Value::List(entries)) = spawner.field_mut(ENTRIES) {
        entries.push(entry.clone());
    }
}

/// The game's names of the spawner fields a change touches: its entries,
/// an entry's templates, and its counts and timeouts.
const ENTRIES: &str = "spawnEntries";
const ENTRY_TEMPLATES: &str = "entityTemplates";
const MIN_AMOUNT: &str = "minSpawnAmount";
const MAX_AMOUNT: &str = "maxSpawnAmount";
const MAX_ALIVE: &str = "maxEntitiesAlive";
const MAX_TOTAL: &str = "maxTotalSpawnCount";
const MIN_TIMEOUT: &str = "minSpawnTimeout";
const MAX_TIMEOUT: &str = "maxSpawnTimeout";

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::spawners`.
    #[doc(hidden)]
    fn spawners(&self, pick: SpawnerPick) -> Result<Vec<Guid>, Off>;

    /// As `Load::add_spawn_entry`.
    #[doc(hidden)]
    fn add_spawn_entry(&mut self, entry: SpawnEntry) -> Result<(), Off>;

    /// As `Load::change_spawner`.
    #[doc(hidden)]
    fn change_spawner(&mut self, spawner: SpawnerPick, change: SpawnerChange) -> Result<(), Off>;
}

impl LoadMethods for BaseLoad {
    fn spawners(&self, pick: SpawnerPick) -> Result<Vec<Guid>, Off> {
        let listed = match pick {
            SpawnerPick::Spawner(spawner) => return Ok(vec![spawner]),
            SpawnerPick::Listing(listed) => listed,
        };
        let host = self.host();
        let registry = crate::resources::Registry::game()?;
        let hash = self.hash(RandomSpawner::ID)?;
        let spawner = registry
            .hashed(hash)
            .ok_or_else(|| Off::new("spawners: the random spawner is not reflected"))?
            .qualified_name();
        let template = registry
            .hashed(self.hash(types::TEMPLATE_RESOURCE.id())?)
            .ok_or_else(|| Off::new("spawners: the template is not reflected"))?;
        let components = self.offset(crate::engine_ids::TEMPLATE_COMPONENTS)?;
        let mut found = Vec::new();
        for key in host.resource_keys(Some(template))? {
            let Some(bytes) = host.game_resource(key) else {
                continue;
            };
            // Cheap first: a spawner component and the listed GUID somewhere.
            let blob = Blob::new(&bytes);
            let has_spawner = blob.array(components).is_some_and(|(start, count)| {
                (0..usize::try_from(count).unwrap_or(0))
                    .any(|i| blob.read_u32(start + i * VARIANT) == Some(hash))
            });
            if !has_spawner || !bytes.windows(16).any(|window| window == listed.0) {
                continue;
            }
            let value = registry.decode(template.qualified_name(), &bytes)?;
            if lists(&value, spawner, listed) {
                found.push(Guid(key.guid));
            }
        }
        if found.is_empty() {
            return Err(Off::new(format!(
                "spawners: no vanilla random spawner lists {listed}"
            )));
        }
        Ok(found)
    }

    fn add_spawn_entry(&mut self, entry: SpawnEntry) -> Result<(), Off> {
        let added = crate::typed::runtime::made::spawner_entry(
            vec![entry.template],
            entry.group.unwrap_or(Guid([0; 16])),
            entry.conditions.unwrap_or(Flags(Vec::new())),
            entry.weight,
        );
        let added = crate::typed::Reflected::to_value(&added);
        for spawner in self.spawners(entry.spawner)? {
            let added = added.clone();
            self.change_template(
                spawner,
                vec![ComponentChange::edit_value(
                    RandomSpawner::ID,
                    move |held| {
                        add_entry(held, &added);
                    },
                )],
            )?;
        }
        Ok(())
    }

    fn change_spawner(&mut self, spawner: SpawnerPick, change: SpawnerChange) -> Result<(), Off> {
        for spawner in self.spawners(spawner)? {
            self.change_template(
                spawner,
                vec![ComponentChange::edit_value(
                    RandomSpawner::ID,
                    move |held| {
                        change.apply(held);
                    },
                )],
            )?;
        }
        Ok(())
    }
}
