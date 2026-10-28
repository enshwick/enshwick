//! Character stats: health, stamina, mana, level and experience, the time a
//! character holds out in the shroud, and the meter of an item's own entity, read from their attribute set components
//! and written to them. A set keeps the id
//! of its first attribute, the values' offset, flags and count, then the
//! values in the order of the game's attribute container.

use crate::Entity;
pub use crate::abi::SkillNode;
use crate::engine_ids as ids;
use crate::raw::{read_at, write_at};
use crate::world::{ComponentType, World, WorldError, WritableType};
use crate::{Host, Load, Off};

use crate::__private::ATTRIBUTE_SET_INITIALISED as SET_INITIALISED;
/// The skill nodes a character keeps.
const SKILL_SLOTS: usize = 256;
/// A node's level is the top three bits of its byte; 1.0 build 1269579.
const SKILL_LEVEL_SHIFT: u8 = 5;

/// A value with its maximum, such as health.
///
/// The name is enshwick's: one attribute's current and maximum value.
///
/// # Examples
///
/// ```
/// use enshwick::world::Meter;
///
/// let health = Meter { current: 150, max: 200 };
/// assert_eq!(health.fraction(), 0.75);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Meter {
    /// The value now.
    pub current: i32,
    /// The most it holds now, buffs and gear included.
    pub max: i32,
}

impl Meter {
    /// `current` over `max`, 0 when `max` is not above 0.
    ///
    /// # Examples
    ///
    /// See [`Meter`].
    #[must_use]
    pub fn fraction(self) -> f32 {
        if self.max <= 0 {
            return 0.0;
        }
        #[expect(clippy::cast_precision_loss, reason = "stats are far below 2^24")]
        let fraction = self.current as f32 / self.max as f32;
        fraction
    }
}

/// An entity's health, stamina and mana, from [`World::vitals`].
///
/// The name is enshwick's: read from the game's attribute sets.
///
/// # Examples
///
/// See [`World::vitals`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Vitals {
    /// Health; every entity with vitals has it.
    pub health: Meter,
    /// Stamina, for characters and the actors that have it.
    pub stamina: Option<Meter>,
    /// Mana, for characters and the actors that have it.
    pub mana: Option<Meter>,
}

/// Which current value a write sets.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Pool {
    Health,
    Stamina,
    Mana,
    /// An item entity's meter, the game's fuel.
    Meter,
    /// The time a character holds out in the shroud.
    Shroud,
}

/// A skill node a character unlocked, from [`World::skills`].
///
/// The name is enshwick's: one entry of the game's unlocked skill nodes and
/// their levels.
///
/// # Examples
///
/// See [`World::skills`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Skill {
    /// The node in the game's skill tree.
    pub node: SkillNode,
    /// Its level, from 1, for nodes that level up; 0 for the others.
    pub level: u8,
}

/// A character's level and experience, from [`World::progress`].
///
/// The name is enshwick's: read from the game's attribute sets.
///
/// # Examples
///
/// See [`World::progress`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Progress {
    /// The level.
    pub level: u32,
    /// Experience gained toward the next level.
    pub experience: u32,
    /// Experience the next level needs in all.
    pub experience_needed: u32,
}

/// One attribute set: its component type, the id it keeps, and the slots
/// read from it.
#[derive(Clone, Copy, Debug)]
struct Set<const N: usize> {
    ty: ComponentType,
    root: u32,
    slots: [usize; N],
}

/// The layout of an attribute set component.
#[derive(Clone, Copy, Debug)]
struct SetLayout {
    data_offset: usize,
    flags: usize,
    count: usize,
}

/// What the stat reads need on the running build. The attribute sets are
/// the game's own fixed set, the sets its attribute container lists; each
/// is a component type the catalogue names.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stats {
    layout: SetLayout,
    health: Set<2>,
    stamina: Set<2>,
    mana: Set<2>,
    level: Set<1>,
    experience: Set<2>,
    /// `Fuel` on an item's own entity; `None` on a build without its rows.
    meter: Option<Set<2>>,
    shroud: Option<Set<2>>,
    /// The actor component's state mask and its bit for standing in the
    /// shroud.
    in_shroud: Option<(crate::world::Field<u64>, u64)>,
}

/// The unlocked skill nodes' component, and where it keeps their ids and
/// their levels; kept apart, since [`World`] is copied by value.
static SKILLS: crate::fresh::Fresh<(ComponentType, usize, usize)> = crate::fresh::Fresh::new();

impl Stats {
    /// The stat reads for the running build.
    pub(crate) fn resolve(load: &Load<'_>, host: Host) -> Result<Self, Off> {
        let value = |name: &str| {
            load.constant(name).and_then(|v| {
                u32::try_from(v).map_err(|_| Off::new(format!("constant {name}: out of range")))
            })
        };
        let slot = |name: &str| value(name).map(|v| v as usize);
        let set = |ty: &str| ComponentType::named(host, ty);
        let skills = (|| {
            let ty = ids::SKILL_NODES;
            let at = |id: &str| {
                load.constant(id)
                    .and_then(|v| usize::try_from(v).map_err(|_| Off::new(format!("field {id}"))))
            };
            let size = usize::try_from(load.type_size(ty)?).unwrap_or(0);
            let (nodes, levels) = (at(ids::SKILL_NODES_IDS)?, at(ids::SKILL_NODES_LEVELS)?);
            if nodes + SKILL_SLOTS * 4 > size || levels + SKILL_SLOTS > size {
                return Err(Off::new(format!("{ty}: smaller than this API reads")));
            }
            Ok::<_, Off>((set(ty)?, nodes, levels))
        })();
        if let Ok(found) = skills {
            let _first = SKILLS.set(found);
        }
        Ok(Self {
            layout: SetLayout {
                data_offset: slot("ecs.level_data_offset")?,
                flags: slot("ecs.level_flags")?,
                count: slot("ecs.attribute_count")?,
            },
            health: Set {
                ty: set(ids::ATTRIBUTE_SET_HEALTH)?,
                root: value("attribute.health")?,
                slots: [0, slot("attribute.health_max_slot")?],
            },
            stamina: Set {
                ty: set(ids::ATTRIBUTE_SET_STAMINA)?,
                root: value("attribute.stamina")?,
                slots: [0, slot("attribute.stamina_max_slot")?],
            },
            mana: Set {
                ty: set(ids::ATTRIBUTE_SET_MANA)?,
                root: value("attribute.mana")?,
                slots: [0, slot("attribute.mana_max_slot")?],
            },
            level: Set {
                ty: set(ids::ATTRIBUTE_SET_LEVEL)?,
                root: value("attribute.level")?,
                slots: [0],
            },
            experience: Set {
                ty: set(ids::ATTRIBUTE_SET_EXPERIENCE)?,
                root: value("attribute.experience")?,
                slots: [0, slot("attribute.experience_needed_slot")?],
            },
            meter: (|| {
                Ok::<_, Off>(Set {
                    ty: set(ids::ATTRIBUTE_SET_FUEL)?,
                    root: value("attribute.fuel")?,
                    slots: [0, slot("attribute.fuel_max_slot")?],
                })
            })()
            .ok(),
            shroud: (|| {
                Ok::<_, Off>(Set {
                    ty: set(ids::ATTRIBUTE_SET_SHROUD)?,
                    root: value("attribute.shroud")?,
                    slots: [0, slot("attribute.shroud_max_slot")?],
                })
            })()
            .ok(),
            in_shroud: (|| {
                let actor = WritableType {
                    ty: ComponentType::named(host, ids::BASE_ACTOR)?,
                    size: load.type_size(ids::BASE_ACTOR)?,
                    name: ids::BASE_ACTOR,
                };
                let field = actor.field_by_id::<u64>(load, ids::BASE_ACTOR_STATE)?;
                Ok::<_, Off>((field, load.constant("actor.state_in_shroud")?))
            })()
            .ok(),
        })
    }

    /// The values in `set`'s slots on `entity`, when its set is initialised
    /// and is the one expected.
    fn read<const N: usize>(&self, world: World, entity: Entity, set: &Set<N>) -> Option<[u32; N]> {
        let data = world.component(entity, set.ty)?.cast::<u8>();
        // SAFETY: `component` returned the entity's live attribute set, valid
        // until the game thread moves on; the set starts with the u32 id of
        // its first attribute.
        let root: u32 = unsafe { read_at(data, 0) };
        // SAFETY: `data` is the live attribute set; its header keeps the u16
        // values' offset at the table constant `ecs.level_data_offset`.
        let offset = usize::from(unsafe { read_at::<u16>(data, self.layout.data_offset) });
        // SAFETY: `data` is the live attribute set; its header keeps the flag
        // byte at the table constant `ecs.level_flags`.
        let flags: u8 = unsafe { read_at(data, self.layout.flags) };
        // SAFETY: `data` is the live attribute set; its header keeps the count
        // byte at the table constant `ecs.attribute_count`.
        let count = usize::from(unsafe { read_at::<u8>(data, self.layout.count) });
        if root != set.root || flags & SET_INITIALISED == 0 {
            return None;
        }
        let mut values = [0u32; N];
        for (value, &slot) in values.iter_mut().zip(&set.slots) {
            if slot >= count {
                return None;
            }
            // SAFETY: `data` is the live, initialised attribute set (flag
            // checked above), which keeps `count` u32 values from `offset`,
            // and `slot < count` was checked above.
            *value = unsafe { read_at(data, offset + slot * size_of::<u32>()) };
        }
        Some(values)
    }

    /// Sets the current value of `pool` on `entity` to what `change` makes
    /// of it, kept between the pool's floor and its maximum: 1 for health,
    /// so a write never kills, 0 for stamina and mana. Health of an entity
    /// already dead is refused: death and revival stay the game's.
    pub(crate) fn write(
        &self,
        world: World,
        entity: Entity,
        pool: Pool,
        change: impl FnOnce(i32) -> i32,
    ) -> Result<Meter, WorldError> {
        let (set, floor) = match pool {
            Pool::Health => (&self.health, 1),
            Pool::Stamina => (&self.stamina, 0),
            Pool::Mana => (&self.mana, 0),
            Pool::Meter => (self.meter.as_ref().ok_or(WorldError::Missing)?, 0),
            Pool::Shroud => (self.shroud.as_ref().ok_or(WorldError::Missing)?, 0),
        };
        let data = world.component_mut(entity, set.ty)?.cast::<u8>();
        // SAFETY: `component_mut` returned the entity's live attribute set,
        // handed out for writing on the game thread; the set starts with the
        // u32 id of its first attribute.
        let root: u32 = unsafe { read_at(data, 0) };
        // SAFETY: `data` is the live attribute set; its header keeps the u16
        // values' offset at the table constant `ecs.level_data_offset`.
        let offset = usize::from(unsafe { read_at::<u16>(data, self.layout.data_offset) });
        // SAFETY: `data` is the live attribute set; its header keeps the flag
        // byte at the table constant `ecs.level_flags`.
        let flags: u8 = unsafe { read_at(data, self.layout.flags) };
        // SAFETY: `data` is the live attribute set; its header keeps the count
        // byte at the table constant `ecs.attribute_count`.
        let count = usize::from(unsafe { read_at::<u8>(data, self.layout.count) });
        let [current_slot, max_slot] = set.slots;
        if root != set.root || flags & SET_INITIALISED == 0 || max_slot >= count {
            return Err(WorldError::Lacks);
        }
        let value = |slot: usize| offset + slot * size_of::<u32>();
        // SAFETY: `data` is the live, initialised attribute set (flag checked
        // above), which keeps `count` 32-bit values from `offset`;
        // `Stats::resolve` puts every current value in slot 0, and
        // `max_slot < count` checked above makes `count` at least 1.
        let current: i32 = unsafe { read_at(data, value(current_slot)) };
        // SAFETY: `data` is the live, initialised attribute set, which keeps
        // `count` 32-bit values from `offset`, and `max_slot < count` was
        // checked above.
        let max: i32 = unsafe { read_at(data, value(max_slot)) };
        if matches!(pool, Pool::Health) && current <= 0 {
            return Err(WorldError::Refused);
        }
        let new = change(current).clamp(floor, max.max(floor));
        // SAFETY: `component_mut` handed the attribute set out for writing on
        // the game thread; slot 0 lies inside its `count` 32-bit values, as
        // checked for the read above, and the engine keeps an i32 there.
        unsafe { write_at(data, value(current_slot), new) };
        Ok(Meter { current: new, max })
    }

    pub(crate) fn vitals(&self, world: World, entity: Entity) -> Option<Vitals> {
        let meter = |[current, max]: [u32; 2]| Meter {
            current: current.cast_signed(),
            max: max.cast_signed(),
        };
        Some(Vitals {
            health: meter(self.read(world, entity, &self.health)?),
            stamina: self.read(world, entity, &self.stamina).map(meter),
            mana: self.read(world, entity, &self.mana).map(meter),
        })
    }

    pub(crate) fn shroud(&self, world: World, entity: Entity) -> Option<Meter> {
        let [current, max] = self.read(world, entity, self.shroud.as_ref()?)?;
        Some(Meter {
            current: current.cast_signed(),
            max: max.cast_signed(),
        })
    }

    pub(crate) fn skills(world: World, entity: Entity) -> Option<Vec<Skill>> {
        let &(ty, ids, levels) = SKILLS.get()?;
        let data = world.component(entity, ty)?.cast::<u8>();
        Some(
            (0..SKILL_SLOTS)
                .filter_map(|slot| {
                    // SAFETY: `component` returned the character's live
                    // UnlockedSkillNodes, valid until the game thread moves
                    // on; `Stats::resolve` checked that `SKILL_SLOTS` u32 ids
                    // from the reflected `ids` lie inside the type's size.
                    let node: u32 = unsafe { read_at(data, ids + slot * 4) };
                    // SAFETY: `data` is the live UnlockedSkillNodes;
                    // `Stats::resolve` checked that `SKILL_SLOTS` level bytes
                    // from the reflected `levels` lie inside the type's size.
                    let level: u8 = unsafe { read_at(data, levels + slot) };
                    (node != 0).then_some(Skill {
                        node: SkillNode(node),
                        level: level >> SKILL_LEVEL_SHIFT,
                    })
                })
                .collect(),
        )
    }

    pub(crate) fn in_shroud(&self, world: World, entity: Entity) -> Option<bool> {
        let (field, bit) = self.in_shroud?;
        world.get(entity, field).map(|state| state & bit != 0)
    }

    pub(crate) fn meter(&self, world: World, pide: Entity) -> Option<Meter> {
        let [current, max] = self.read(world, pide, self.meter.as_ref()?)?;
        Some(Meter {
            current: current.cast_signed(),
            max: max.cast_signed(),
        })
    }

    pub(crate) fn progress(&self, world: World, entity: Entity) -> Option<Progress> {
        let [level] = self.read(world, entity, &self.level)?;
        let [experience, experience_needed] = self.read(world, entity, &self.experience)?;
        Some(Progress {
            level,
            experience,
            experience_needed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SkillNode;

    #[test]
    fn a_skill_node_converts_to_and_from_its_number() {
        let node = SkillNode::from(0x2a_u32);
        assert_eq!(u32::from(node), 0x2a);
        assert_eq!(node, SkillNode::new(0x2a));
        assert_eq!(node.get(), 0x2a);
    }
}
