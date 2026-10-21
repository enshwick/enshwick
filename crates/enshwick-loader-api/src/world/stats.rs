//! Character stats: health, stamina, mana, level and experience, the time a
//! character holds out in the shroud, and the meter of an item's own entity, read from their attribute set components
//! and written to them. A set keeps the id
//! of its first attribute, the values' offset, flags and count, then the
//! values in the order of the game's attribute container.

use crate::ecs::Entity;
use crate::raw::read_at;
use crate::world::{ComponentType, World, WorldError};
use crate::{Load, Off};

/// The attribute set's flag bit that marks it initialised.
const SET_INITIALISED: u8 = 1;
/// The skill nodes a character keeps.
const SKILL_SLOTS: usize = 256;
/// A node's level is the top three bits of its byte; 1.0 build 1269579.
const SKILL_LEVEL_SHIFT: u8 = 5;

/// A value with its maximum, such as health.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::world::Meter;
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
/// # Examples
///
/// See [`World::skills`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Skill {
    /// The node's id in the game's skill tree.
    pub node: u32,
    /// Its level, from 1, for nodes that level up; 0 for the others.
    pub level: u8,
}

/// A character's level and experience, from [`World::progress`].
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

/// What the stat reads need on the running build.
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
static SKILLS: std::sync::OnceLock<(ComponentType, usize, usize)> = std::sync::OnceLock::new();

impl Stats {
    /// The stat reads for the running build.
    pub(crate) fn resolve(load: &Load<'_>, world: &World) -> Result<Self, Off> {
        let value = |name: &str| {
            load.constant(name).and_then(|v| {
                u32::try_from(v).map_err(|_| Off::new(format!("constant {name}: out of range")))
            })
        };
        let slot = |name: &str| value(name).map(|v| v as usize);
        let set = |ty: &str| world.component_type(load, ty);
        let skills = (|| {
            let ty = "keen::ecs::UnlockedSkillNodes";
            let at = |field: &str| {
                load.field_offset(ty, field).and_then(|v| {
                    usize::try_from(v).map_err(|_| Off::new(format!("field {ty}.{field}")))
                })
            };
            let size = usize::try_from(load.type_size(ty)?).unwrap_or(0);
            let (ids, levels) = (at("knownSkillNodes")?, at("unlockLevel")?);
            if ids + SKILL_SLOTS * 4 > size || levels + SKILL_SLOTS > size {
                return Err(Off::new(format!("{ty}: smaller than this API reads")));
            }
            Ok::<_, Off>((set(ty)?, ids, levels))
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
                ty: set("keen::ecs::Health")?,
                root: value("attribute.health")?,
                slots: [0, slot("attribute.health_max_slot")?],
            },
            stamina: Set {
                ty: set("keen::ecs::Stamina")?,
                root: value("attribute.stamina")?,
                slots: [0, slot("attribute.stamina_max_slot")?],
            },
            mana: Set {
                ty: set("keen::ecs::Mana")?,
                root: value("attribute.mana")?,
                slots: [0, slot("attribute.mana_max_slot")?],
            },
            level: Set {
                ty: set("keen::ecs::Level")?,
                root: value("attribute.level")?,
                slots: [0],
            },
            experience: Set {
                ty: set("keen::ecs::Experience")?,
                root: value("attribute.experience")?,
                slots: [0, slot("attribute.experience_needed_slot")?],
            },
            meter: (|| {
                Ok::<_, Off>(Set {
                    ty: set("keen::ecs::Fuel")?,
                    root: value("attribute.fuel")?,
                    slots: [0, slot("attribute.fuel_max_slot")?],
                })
            })()
            .ok(),
            shroud: (|| {
                Ok::<_, Off>(Set {
                    ty: set("keen::ecs::FogResistance")?,
                    root: value("attribute.shroud")?,
                    slots: [0, slot("attribute.shroud_max_slot")?],
                })
            })()
            .ok(),
            in_shroud: (|| {
                let field = world
                    .writable_type(load, "keen::ecs::BaseActor")?
                    .field::<u64>(load, "currentState")?;
                Ok::<_, Off>((field, load.constant("actor.state_in_shroud")?))
            })()
            .ok(),
        })
    }

    /// The values in `set`'s slots on `entity`, when its set is initialised
    /// and is the one expected.
    fn read<const N: usize>(
        &self,
        world: &World,
        entity: Entity,
        set: &Set<N>,
    ) -> Option<[u32; N]> {
        let data = world.component(entity, set.ty)?.cast::<u8>();
        // SAFETY: the entity's live attribute set, on the game thread, starts
        // with the id it keeps.
        let root: u32 = unsafe { read_at(data, 0) };
        // SAFETY: as above; the set's header holds the values' offset, its
        // flags and its count at the build's offsets.
        let offset = usize::from(unsafe { read_at::<u16>(data, self.layout.data_offset) });
        // SAFETY: as above.
        let flags: u8 = unsafe { read_at(data, self.layout.flags) };
        // SAFETY: as above.
        let count = usize::from(unsafe { read_at::<u8>(data, self.layout.count) });
        if root != set.root || flags & SET_INITIALISED == 0 {
            return None;
        }
        let mut values = [0u32; N];
        for (value, &slot) in values.iter_mut().zip(&set.slots) {
            if slot >= count {
                return None;
            }
            // SAFETY: as above; the engine keeps `count` values from `offset`.
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
        world: &World,
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
        // SAFETY: the entity's live attribute set, handed out for writing on
        // the game thread, starts with the id it keeps and holds its header
        // at the build's offsets.
        let root: u32 = unsafe { read_at(data, 0) };
        // SAFETY: as above.
        let offset = usize::from(unsafe { read_at::<u16>(data, self.layout.data_offset) });
        // SAFETY: as above.
        let flags: u8 = unsafe { read_at(data, self.layout.flags) };
        // SAFETY: as above.
        let count = usize::from(unsafe { read_at::<u8>(data, self.layout.count) });
        let [current_slot, max_slot] = set.slots;
        if root != set.root || flags & SET_INITIALISED == 0 || max_slot >= count {
            return Err(WorldError::Lacks);
        }
        let value = |slot: usize| data.wrapping_add(offset + slot * size_of::<u32>());
        // SAFETY: the engine keeps `count` values from `offset`.
        let current = unsafe { value(current_slot).cast::<i32>().read_unaligned() };
        // SAFETY: as above.
        let max = unsafe { value(max_slot).cast::<i32>().read_unaligned() };
        if matches!(pool, Pool::Health) && current <= 0 {
            return Err(WorldError::Refused);
        }
        let new = change(current).clamp(floor, max.max(floor));
        // SAFETY: as above; the set was handed out for writing.
        unsafe { value(current_slot).cast::<i32>().write_unaligned(new) };
        Ok(Meter { current: new, max })
    }

    pub(crate) fn vitals(&self, world: &World, entity: Entity) -> Option<Vitals> {
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

    pub(crate) fn shroud(&self, world: &World, entity: Entity) -> Option<Meter> {
        let [current, max] = self.read(world, entity, self.shroud.as_ref()?)?;
        Some(Meter {
            current: current.cast_signed(),
            max: max.cast_signed(),
        })
    }

    pub(crate) fn skills(world: &World, entity: Entity) -> Option<Vec<Skill>> {
        let &(ty, ids, levels) = SKILLS.get()?;
        let data = world.component(entity, ty)?.cast::<u8>();
        Some(
            (0..SKILL_SLOTS)
                .filter_map(|slot| {
                    // SAFETY: the character's live unlocked skill nodes, on
                    // the game thread: `SKILL_SLOTS` node ids and as many
                    // level bytes at their reflected offsets.
                    let node: u32 = unsafe { read_at(data, ids + slot * 4) };
                    // SAFETY: as above.
                    let level: u8 = unsafe { read_at(data, levels + slot) };
                    (node != 0).then_some(Skill {
                        node,
                        level: level >> SKILL_LEVEL_SHIFT,
                    })
                })
                .collect(),
        )
    }

    pub(crate) fn in_shroud(&self, world: &World, entity: Entity) -> Option<bool> {
        let (field, bit) = self.in_shroud?;
        world.get(entity, field).map(|state| state & bit != 0)
    }

    pub(crate) fn meter(&self, world: &World, pide: Entity) -> Option<Meter> {
        let [current, max] = self.read(world, pide, self.meter.as_ref()?)?;
        Some(Meter {
            current: current.cast_signed(),
            max: max.cast_signed(),
        })
    }

    pub(crate) fn progress(&self, world: &World, entity: Entity) -> Option<Progress> {
        let [level] = self.read(world, entity, &self.level)?;
        let [experience, experience_needed] = self.read(world, entity, &self.experience)?;
        Some(Progress {
            level,
            experience,
            experience_needed,
        })
    }
}
