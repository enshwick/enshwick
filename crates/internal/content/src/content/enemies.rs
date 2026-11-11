//! Enemies from mods: copies of vanilla enemies with their own level,
//! health, damage, faction and arsenal, and arsenals of vanilla attacks.
//!
//! An enemy is an entity [template](super::templates) whose components hold
//! its rules; [`Load::add_enemy`] copies a vanilla one and sets the common
//! values, which the server reads when it spawns one. What an enemy attacks
//! with is its arsenal: [`Load::add_arsenal`] makes one from a vanilla
//! arsenal and a pick of vanilla attacks and behaviours, and
//! [`Load::change_arsenal`] changes a vanilla one. Models, animations and
//! attacks stay the game's: the client has to have them.
//!
//! ```
//! use enshwick::content::{Guid, id};
//! use enshwick::content::enemies::NewEnemy;
//! use enshwick::typed::FactionType;
//!
//! const WOLF: Guid = Guid::parse("0d75b9a9-7e48-4f3a-bf2a-19bf6021f531");
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.add_enemy(NewEnemy {
//!         level: Some(30),
//!         health: Some(2.0),
//!         faction: Some(FactionType::Vukah),
//!         ..NewEnemy::new(id!("pack-leader"), WOLF)
//!     })?;
//!     Ok(())
//! }
//! ```

use crate::resources::Value;
use crate::typed::ecs::{Boss, DamageMod, EnemyArsenalReference, EnemyScaling, Faction, Level};
use crate::typed::enemy::EnemyArsenalRegistryResource;
use crate::typed::{FactionType, Named as _, Reflected as _};
use crate::{Load, Off};

use super::templates::{ComponentChange, NewTemplate, TemplateId};
use super::{EngineFacts, Guid, Id};

/// Whether the vanilla template `template` holds a [`Boss`]; `true` when
/// it does not read, so the template's own refusal names why.
fn holds_boss(load: &Load<'_>, template: Guid) -> bool {
    let host = load.host();
    let read = || {
        let registry = crate::resources::Registry::game().ok()?;
        let boss = registry.hashed(host.hash(Boss::ID).ok()?)?.qualified_name();
        let hash = host.hash(super::types::TEMPLATE_RESOURCE.id()).ok()?;
        let value = host.game_value(template.key_by_hash(hash)).ok()?;
        Some(
            super::templates::components_of(&value)
                .iter()
                .any(|held| held == boss),
        )
    };
    read().unwrap_or(true)
}

/// The game's list of every arsenal.
pub const ARSENAL_REGISTRY: Guid = Guid::parse("cd861d95-a79c-4c11-8b48-8fae34a4156c");

/// Where `DamageMod` keeps the factor its owner's damage is multiplied by;
/// the game makes slot 0 of it and the base in slot 1 (server 399, read
/// live on a server copy).
const DAMAGE_FACTOR: usize = 2;

/// The game's names of what an arsenal holds: the registry's arsenals, an
/// arsenal's GUID, attacks and behaviours, an attack's description and the
/// values a change sets in it.
const ARSENALS: &str = "arsenals";
const GUID: &str = "guid";
const ATTACKS: &str = "attacks";
const BEHAVIORS: &str = "behaviors";
const DESCRIPTION: &str = "description";
const SELECTION_CHANCE: &str = "selectionChance";
const THREAT: &str = "threat";
const MIN_RANGE: &str = "minRange";
const MAX_RANGE: &str = "maxRange";

/// An arsenal: the attacks and behaviours an enemy picks from.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::content::enemies::ArsenalId;
///
/// let id = id!("pack");
/// assert_eq!(ArsenalId::new(id.guid()).guid(), id.guid());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct ArsenalId(Guid);

impl ArsenalId {
    /// The arsenal with GUID `guid`.
    ///
    /// # Examples
    ///
    /// See [`ArsenalId`].
    #[must_use]
    pub const fn new(guid: Guid) -> Self {
        Self(guid)
    }

    /// The arsenal's GUID.
    ///
    /// # Examples
    ///
    /// See [`ArsenalId`].
    #[must_use]
    pub const fn guid(self) -> Guid {
        self.0
    }
}

/// A new enemy for [`Load::add_enemy`]: a copy of the vanilla enemy
/// template `like` under `id`, with the values it gives set, then `changes`
/// applied in order.
///
/// # Examples
///
/// See [`Load::add_enemy`].
#[derive(Clone, Debug, crate::Script)]
pub struct NewEnemy {
    /// The new enemy's content id, its template's.
    pub id: Id,
    /// The vanilla enemy template copied.
    pub like: Guid,
    /// Its level, from 1.
    pub level: Option<u32>,
    /// Its health at each level, about this times the vanilla enemy's.
    pub health: Option<f32>,
    /// The damage it deals, this times the vanilla enemy's.
    pub damage: Option<f32>,
    /// Its faction, which decides whom it fights.
    pub faction: Option<FactionType>,
    /// Its arsenal: a vanilla one or one from [`Load::add_arsenal`].
    pub arsenal: Option<ArsenalId>,
    /// Whether it is a boss.
    pub boss: Option<bool>,
    /// Further changes of its components, applied after the values above.
    #[script(default)]
    pub changes: Vec<ComponentChange>,
}

impl NewEnemy {
    /// A copy of `like` under `id`, unchanged.
    ///
    /// # Examples
    ///
    /// See [`Load::add_enemy`].
    #[must_use]
    pub const fn new(id: Id, like: Guid) -> Self {
        Self {
            id,
            like,
            level: None,
            health: None,
            damage: None,
            faction: None,
            arsenal: None,
            boss: None,
            changes: Vec::new(),
        }
    }

    /// The component changes its values stand for, then its own changes.
    fn into_changes(self) -> Result<Vec<ComponentChange>, Off> {
        let mut changes = Vec::new();
        if let Some(level) = self.level {
            if level == 0 {
                return Err(Off::new(format!("enemy {}: levels start at 1", self.id)));
            }
            changes.push(ComponentChange::edit(move |held: &mut Level| {
                if let Some(slot) = held.data_storage.first_mut() {
                    *slot = level;
                }
            }));
        }
        if let Some(health) = self.health {
            changes.push(ComponentChange::edit(move |scaling: &mut EnemyScaling| {
                scaling.enemy_level_scaling.health_scale_factor *= health;
            }));
        }
        if let Some(damage) = self.damage {
            changes.push(ComponentChange::edit(move |held: &mut DamageMod| {
                if let Some(slot) = held.data_storage.get_mut(DAMAGE_FACTOR) {
                    *slot *= damage;
                }
            }));
        }
        if let Some(faction) = self.faction {
            changes.push(ComponentChange::edit(move |held: &mut Faction| {
                held.faction = faction.clone();
            }));
        }
        if let Some(arsenal) = self.arsenal {
            changes.push(ComponentChange::edit(
                move |held: &mut EnemyArsenalReference| held.arsenal = arsenal.guid(),
            ));
        }
        match self.boss {
            Some(true) => changes.push(ComponentChange::set(&Boss::from_value(&Value::Struct(
                Vec::new(),
            ))?)),
            Some(false) => changes.push(ComponentChange::remove::<Boss>()),
            None => {}
        }
        changes.extend(self.changes);
        Ok(changes)
    }
}

/// A change of an arsenal: the attacks and behaviours it picks from, and
/// the values of some of its attacks.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::enemies::{ArsenalChange, AttackChange};
///
/// let bite = Guid::parse("6f6144ed-5d06-463b-bba0-3a8eb157cb1a");
/// let change = ArsenalChange {
///     attacks: Some(vec![bite]),
///     tune: vec![AttackChange { threat: Some(20.0), ..AttackChange::new(bite) }],
///     ..ArsenalChange::default()
/// };
/// assert!(change.behaviors.is_none());
/// ```
#[derive(Clone, Debug, Default, PartialEq, crate::Script)]
pub struct ArsenalChange {
    /// The attacks, by GUID, each from any vanilla arsenal, in place of the
    /// arsenal's; `None` keeps them.
    pub attacks: Option<Vec<Guid>>,
    /// The behaviours, by GUID, each from any vanilla arsenal, in place of
    /// the arsenal's; `None` keeps them.
    pub behaviors: Option<Vec<Guid>>,
    /// Values of attacks the arsenal holds after the picks above.
    #[script(default)]
    pub tune: Vec<AttackChange>,
}

/// New values of one attack of an arsenal; `None` keeps a value.
///
/// # Examples
///
/// See [`ArsenalChange`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
pub struct AttackChange {
    /// The attack, by GUID.
    pub attack: Guid,
    /// How likely an enemy picks it among the attacks that fit.
    pub selection_chance: Option<f32>,
    /// How much threat it causes.
    pub threat: Option<f32>,
    /// The nearest target it is used on, in metres.
    pub min_range: Option<f32>,
    /// The farthest target it is used on, in metres.
    pub max_range: Option<f32>,
}

impl AttackChange {
    /// A change of `attack` that keeps every value.
    ///
    /// # Examples
    ///
    /// See [`ArsenalChange`].
    #[must_use]
    pub const fn new(attack: Guid) -> Self {
        Self {
            attack,
            selection_chance: None,
            threat: None,
            min_range: None,
            max_range: None,
        }
    }
}

/// A new arsenal for [`Load::add_arsenal`]: a copy of the vanilla arsenal
/// `like` under `id`, with `change` applied.
///
/// # Examples
///
/// See [`Load::add_arsenal`].
#[derive(Clone, Debug, PartialEq, crate::Script)]
pub struct NewArsenal {
    /// The new arsenal's content id.
    pub id: Id,
    /// The vanilla arsenal copied.
    pub like: Guid,
    /// What the copy changes.
    #[script(default)]
    pub change: ArsenalChange,
}

/// The GUID a value holds in `field`, when it holds one.
fn guid_of(value: &Value, field: &str) -> Option<Guid> {
    match value.field(field) {
        Some(Value::Guid(guid)) => Some(*guid),
        _ => None,
    }
}

/// Every attack and every behaviour the registry's arsenals hold, by GUID.
fn parts(arsenals: &[Value]) -> (Vec<&Value>, Vec<&Value>) {
    let list = |field| {
        arsenals
            .iter()
            .filter_map(move |arsenal| match arsenal.field(field) {
                Some(Value::List(items)) => Some(items.iter()),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>()
    };
    (list(ATTACKS), list(BEHAVIORS))
}

/// The GUIDs `change` names that `arsenals` hold no attack or behaviour of.
fn unknown(arsenals: &[Value], change: &ArsenalChange) -> Vec<Guid> {
    let (attacks, behaviors) = parts(arsenals);
    let holds =
        |items: &[&Value], guid: &Guid| items.iter().any(|item| guid_of(item, GUID) == Some(*guid));
    let named = change
        .attacks
        .iter()
        .flatten()
        .chain(change.tune.iter().map(|t| &t.attack));
    named
        .filter(|guid| !holds(&attacks, guid))
        .chain(
            change
                .behaviors
                .iter()
                .flatten()
                .filter(|guid| !holds(&behaviors, guid)),
        )
        .copied()
        .collect()
}

/// `arsenal` with `change` applied, its picks taken from `all`, every
/// arsenal of the registry.
fn changed(mut arsenal: Value, all: &[Value], change: &ArsenalChange) -> Result<Value, Off> {
    let (attacks, behaviors) = parts(all);
    let pick = |items: &[&Value], guids: &[Guid]| -> Result<Vec<Value>, Off> {
        guids
            .iter()
            .map(|guid| {
                items
                    .iter()
                    .find(|item| guid_of(item, GUID) == Some(*guid))
                    .map(|item| (*item).clone())
                    .ok_or_else(|| {
                        Off::new(format!("arsenal: no vanilla attack or behaviour {guid}"))
                    })
            })
            .collect()
    };
    for (field, guids, items) in [
        (ATTACKS, &change.attacks, &attacks),
        (BEHAVIORS, &change.behaviors, &behaviors),
    ] {
        if let Some(guids) = guids {
            let picked = pick(items, guids)?;
            match arsenal.field_mut(field) {
                Some(Value::List(held)) => *held = picked,
                _ => return Err(Off::new(format!("arsenal: no list of {field}"))),
            }
        }
    }
    let Some(Value::List(held)) = arsenal.field_mut(ATTACKS) else {
        return Err(Off::new("arsenal: no list of attacks"));
    };
    for tune in &change.tune {
        let attack = held
            .iter_mut()
            .find(|attack| guid_of(attack, GUID) == Some(tune.attack))
            .and_then(|attack| attack.field_mut(DESCRIPTION))
            .ok_or_else(|| Off::new(format!("arsenal: it holds no attack {}", tune.attack)))?;
        for (field, value) in [
            (SELECTION_CHANCE, tune.selection_chance),
            (THREAT, tune.threat),
            (MIN_RANGE, tune.min_range),
            (MAX_RANGE, tune.max_range),
        ] {
            if let (Some(value), Some(held)) = (value, attack.field_mut(field)) {
                *held = Value::Float(value.into());
            }
        }
    }
    Ok(arsenal)
}

/// The registry `registry` with the arsenal `target` changed by `change`,
/// or with a copy of `like` under `target` appended when `like` is set.
fn serve_arsenal(
    mut registry: Value,
    target: Guid,
    like: Option<Guid>,
    change: &ArsenalChange,
) -> Result<Value, Off> {
    let Some(Value::List(arsenals)) = registry.field_mut(ARSENALS) else {
        return Err(Off::new("arsenal registry: no list of arsenals"));
    };
    let from = like.unwrap_or(target);
    let at = arsenals
        .iter()
        .position(|arsenal| guid_of(arsenal, GUID) == Some(from))
        .ok_or_else(|| Off::new(format!("arsenal registry: no arsenal {from}")))?;
    let mut arsenal = changed(arsenals[at].clone(), arsenals, change)?;
    match like {
        Some(_) => {
            if let Some(guid) = arsenal.field_mut(GUID) {
                *guid = Value::Guid(target);
            }
            arsenals.push(arsenal);
        }
        None => arsenals[at] = arsenal,
    }
    Ok(registry)
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::serve_arsenal`.
    #[doc(hidden)]
    fn serve_arsenal(
        &self,
        target: Guid,
        like: Option<Guid>,
        change: ArsenalChange,
    ) -> Result<(), Off>;

    /// As `Load::add_enemy`.
    #[doc(hidden)]
    fn add_enemy(&mut self, mut new: NewEnemy) -> Result<TemplateId, Off>;

    /// As `Load::add_arsenal`.
    #[doc(hidden)]
    fn add_arsenal(&mut self, new: NewArsenal) -> Result<ArsenalId, Off>;

    /// As `Load::change_arsenal`.
    #[doc(hidden)]
    fn change_arsenal(&mut self, arsenal: Guid, change: ArsenalChange) -> Result<(), Off>;
}

impl LoadMethods for BaseLoad {
    fn serve_arsenal(
        &self,
        target: Guid,
        like: Option<Guid>,
        change: ArsenalChange,
    ) -> Result<(), Off> {
        let key = ARSENAL_REGISTRY.key_by_hash(self.hash(EnemyArsenalRegistryResource::ID)?);
        let host = self.host();
        let vanilla = host.game_value(key)?;
        let Some(Value::List(arsenals)) = vanilla.field(ARSENALS) else {
            return Err(Off::new("arsenal registry: no list of arsenals"));
        };
        let from = like.unwrap_or(target);
        if !arsenals
            .iter()
            .any(|arsenal| guid_of(arsenal, GUID) == Some(from))
        {
            return Err(Off::new(format!("arsenal {from}: no vanilla arsenal")));
        }
        if let Some(guid) = unknown(arsenals, &change).first() {
            return Err(Off::new(format!(
                "arsenal {target}: {guid} is no vanilla attack or behaviour"
            )));
        }
        // A tune names an attack of the arsenal after its picks, which only
        // building it shows.
        serve_arsenal(vanilla.clone(), target, like, &change)
            .map_err(|why| Off::new(format!("arsenal {target}: {why}")))?;
        crate::resources::serve_value(host, key, key, move |value| {
            serve_arsenal(value, target, like, &change)
        })
    }

    fn add_enemy(&mut self, mut new: NewEnemy) -> Result<TemplateId, Off> {
        let (id, like) = (new.id, new.like);
        if new.boss == Some(false) && !holds_boss(self, like) {
            new.boss = None;
        }
        let changes = new.into_changes()?;
        self.add_template(NewTemplate {
            changes,
            ..NewTemplate::new(id, like)
        })
    }

    fn add_arsenal(&mut self, new: NewArsenal) -> Result<ArsenalId, Off> {
        self.declare(new.id)?;
        let guid = new.id.guid();
        self.serve_arsenal(guid, Some(new.like), new.change)?;
        Ok(ArsenalId::new(guid))
    }

    fn change_arsenal(&mut self, arsenal: Guid, change: ArsenalChange) -> Result<(), Off> {
        self.serve_arsenal(arsenal, None, change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attack(guid: u8, threat: f64) -> Value {
        Value::Struct(vec![
            (GUID.into(), Value::Guid(Guid([guid; 16]))),
            (
                DESCRIPTION.into(),
                Value::Struct(vec![
                    (SELECTION_CHANCE.into(), Value::Float(1.0)),
                    (THREAT.into(), Value::Float(threat)),
                    (MIN_RANGE.into(), Value::Float(0.0)),
                    (MAX_RANGE.into(), Value::Float(0.0)),
                ]),
            ),
        ])
    }

    fn arsenal(guid: u8, attacks: Vec<Value>) -> Value {
        Value::Struct(vec![
            (GUID.into(), Value::Guid(Guid([guid; 16]))),
            (ATTACKS.into(), Value::List(attacks)),
            (BEHAVIORS.into(), Value::List(Vec::new())),
        ])
    }

    fn registry() -> Value {
        Value::Struct(vec![(
            ARSENALS.into(),
            Value::List(vec![
                arsenal(1, vec![attack(10, 9.0), attack(11, 9.0)]),
                arsenal(2, vec![attack(20, 5.0)]),
            ]),
        )])
    }

    fn arsenals(registry: &Value) -> &[Value] {
        match registry.field(ARSENALS) {
            Some(Value::List(arsenals)) => arsenals,
            _ => &[],
        }
    }

    fn attacks(arsenal: &Value) -> Vec<Guid> {
        match arsenal.field(ATTACKS) {
            Some(Value::List(attacks)) => attacks.iter().filter_map(|a| guid_of(a, GUID)).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn a_new_arsenal_copies_its_model_and_picks_attacks_from_any_arsenal() {
        let change = ArsenalChange {
            attacks: Some(vec![Guid([20; 16]), Guid([10; 16])]),
            tune: vec![AttackChange {
                threat: Some(20.0),
                ..AttackChange::new(Guid([20; 16]))
            }],
            ..ArsenalChange::default()
        };
        let served =
            serve_arsenal(registry(), Guid([7; 16]), Some(Guid([1; 16])), &change).unwrap();
        let all = arsenals(&served);
        assert_eq!(all.len(), 3);
        assert_eq!(guid_of(&all[2], GUID), Some(Guid([7; 16])));
        assert_eq!(attacks(&all[2]), [Guid([20; 16]), Guid([10; 16])]);
        let Some(Value::List(picked)) = all[2].field(ATTACKS) else {
            panic!("{:?}", all[2])
        };
        assert_eq!(picked[0], attack(20, 20.0), "tuned");
        assert_eq!(
            attacks(&all[0]),
            [Guid([10; 16]), Guid([11; 16])],
            "the model keeps its own"
        );
        assert_eq!(
            attacks(&all[1]),
            [Guid([20; 16])],
            "and so does the source of a pick"
        );
    }

    #[test]
    fn a_change_of_a_vanilla_arsenal_stays_in_its_place_and_two_changes_chain() {
        let first = ArsenalChange {
            attacks: Some(vec![Guid([11; 16])]),
            ..ArsenalChange::default()
        };
        let second = ArsenalChange {
            tune: vec![AttackChange {
                selection_chance: Some(0.25),
                ..AttackChange::new(Guid([11; 16]))
            }],
            ..ArsenalChange::default()
        };
        let once = serve_arsenal(registry(), Guid([1; 16]), None, &first).unwrap();
        let twice = serve_arsenal(once, Guid([1; 16]), None, &second).unwrap();
        let all = arsenals(&twice);
        assert_eq!(all.len(), 2);
        assert_eq!(attacks(&all[0]), [Guid([11; 16])]);
        let chance = all[0]
            .field(ATTACKS)
            .and_then(|a| match a {
                Value::List(a) => a.first(),
                _ => None,
            })
            .and_then(|a| a.field(DESCRIPTION))
            .and_then(|d| d.field(SELECTION_CHANCE));
        assert_eq!(chance, Some(&Value::Float(0.25)));
    }

    #[test]
    fn an_attack_no_arsenal_holds_is_named() {
        let change = ArsenalChange {
            attacks: Some(vec![Guid([10; 16]), Guid([99; 16])]),
            ..ArsenalChange::default()
        };
        assert_eq!(unknown(arsenals(&registry()), &change), [Guid([99; 16])]);
        let behaviors = ArsenalChange {
            behaviors: Some(vec![Guid([98; 16])]),
            ..ArsenalChange::default()
        };
        assert_eq!(unknown(arsenals(&registry()), &behaviors), [Guid([98; 16])]);
        let tune = ArsenalChange {
            tune: vec![AttackChange::new(Guid([20; 16]))],
            ..ArsenalChange::default()
        };
        assert!(
            serve_arsenal(registry(), Guid([1; 16]), None, &tune).is_err(),
            "not in this arsenal"
        );
    }
}
