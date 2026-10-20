//! Character stats: health, stamina, mana, level and experience, read from
//! their attribute set components. A set keeps the id of its first
//! attribute, the values' offset, flags and count, then the values in the
//! order of the game's attribute container.

use crate::ecs::Entity;
use crate::world::{ComponentType, World};
use crate::{Load, Off};

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
}

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
        let at = |offset: usize| data.wrapping_add(offset);
        // SAFETY: the entity's live attribute set, read on the game thread,
        // starts with the id it keeps.
        let root = unsafe { at(0).cast::<u32>().read_unaligned() };
        // SAFETY: as above; the values' offset.
        let offset =
            usize::from(unsafe { at(self.layout.data_offset).cast::<u16>().read_unaligned() });
        // SAFETY: as above; the flags.
        let flags = unsafe { at(self.layout.flags).read() };
        // SAFETY: as above; how many values it holds.
        let count = usize::from(unsafe { at(self.layout.count).read() });
        if root != set.root || flags & 1 == 0 {
            return None;
        }
        let mut values = [0u32; N];
        for (value, &slot) in values.iter_mut().zip(&set.slots) {
            if slot >= count {
                return None;
            }
            // SAFETY: as above; the set holds `count` values from `offset`.
            *value = unsafe { at(offset + slot * 4).cast::<u32>().read_unaligned() };
        }
        Some(values)
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
