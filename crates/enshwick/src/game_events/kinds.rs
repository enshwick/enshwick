//! Typed game events: a struct per event with its fields read for the
//! running build, for [`Load::on_game`](crate::Load::on_game).

use crate::ecs::Entity;
use crate::engine_ids as ids;
use crate::game_events::{EventType, Field, RawGameEvent};
use crate::raw::read_at;
use crate::world::{ComponentType, World};
use crate::{Load, Off};

mod sealed {
    pub trait Sealed {}
}

/// A game event with a typed form; implemented by the types in this
/// module only.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::{Died, Kind};
///
/// fn watch<E: Kind + core::fmt::Debug>(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<E>(|event| enshwick::debug!("{event:?}"))
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     watch::<Died>(load)
/// }
/// ```
pub trait Kind: sealed::Sealed + Sized + 'static {
    /// Resolves the event's fields and subscribes `handler`.
    #[doc(hidden)]
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static;
}

/// `Some(entity)` unless it is [`Entity::NONE`].
fn some(entity: Entity) -> Option<Entity> {
    (!entity.is_none()).then_some(entity)
}

/// Subscribes `handler` to the events `decode` turns into an `E`.
fn on<E, D, F>(load: &mut Load<'_>, ty: EventType, decode: D, handler: F) -> Result<(), Off>
where
    D: Fn(&RawGameEvent<'_>) -> Option<E> + Send + Sync + 'static,
    F: Fn(&E) + Send + Sync + 'static,
{
    load.on_raw_game_event(ty, move |event| {
        if let Some(typed) = decode(event) {
            handler(&typed);
        }
    })
}

/// Bits of the damage types of a hit, the engine's `DamageTypeNew` mask.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::DamageTypes;
///
/// let types = DamageTypes(DamageTypes::FIRE.0 | DamageTypes::CUT.0);
/// assert!(types.contains(DamageTypes::FIRE));
/// assert!(!types.contains(DamageTypes::ICE));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DamageTypes(pub u16);

impl DamageTypes {
    /// Physical cut.
    pub const CUT: Self = Self(1);
    /// Physical pierce.
    pub const PIERCE: Self = Self(1 << 1);
    /// Physical blunt.
    pub const BLUNT: Self = Self(1 << 2);
    /// Fire.
    pub const FIRE: Self = Self(1 << 3);
    /// Ice.
    pub const ICE: Self = Self(1 << 4);
    /// Shroud.
    pub const SHROUD: Self = Self(1 << 5);
    /// Lightning.
    pub const LIGHTNING: Self = Self(1 << 6);
    /// Healing.
    pub const HEALING: Self = Self(1 << 7);
    /// Poison.
    pub const POISON: Self = Self(1 << 8);
    /// Wood, against trees and wooden things.
    pub const WOOD: Self = Self(1 << 9);
    /// Stone, against rock.
    pub const STONE: Self = Self(1 << 10);
    /// Metal, against ore.
    pub const METAL: Self = Self(1 << 11);
    /// Explosion.
    pub const EXPLOSION: Self = Self(1 << 12);

    /// Whether every bit of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`DamageTypes`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Bits of the attack categories of a hit, the engine's `AttackCategory`
/// mask.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::AttackCategories;
///
/// assert!(AttackCategories(1).contains(AttackCategories::MELEE));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AttackCategories(pub u8);

impl AttackCategories {
    /// Melee.
    pub const MELEE: Self = Self(1);
    /// Ranged.
    pub const RANGED: Self = Self(1 << 1);
    /// Magic.
    pub const MAGIC: Self = Self(1 << 2);
    /// A tool, such as a pickaxe.
    pub const TOOL: Self = Self(1 << 3);
    /// Anything else.
    pub const OTHER: Self = Self(1 << 4);
    /// Area of effect.
    pub const AREA: Self = Self(1 << 5);
    /// A skill shot.
    pub const SKILL_SHOT: Self = Self(1 << 6);

    /// Whether every bit of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`AttackCategories`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// A hit the server resolved, damage or healing, written where health
/// changes. Fall and shroud damage change health without a hit.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::Hit;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<Hit>(|hit| {
///         if hit.health_change < 0 {
///             enshwick::debug!("{:?} took {} from {:?}", hit.target, -hit.health_change, hit.attacker);
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Hit {
    /// Who the hit is credited to: a character or enemy, not its arrow.
    pub attacker: Option<Entity>,
    /// What dealt it: the attacker itself, a projectile or an effect.
    pub source: Option<Entity>,
    /// The entity hit.
    pub target: Entity,
    /// The root of what was hit: the actor a hit body part belongs to.
    pub root_target: Option<Entity>,
    /// The weapon's item entity.
    pub weapon: Option<Entity>,
    /// The change of the target's health: below 0 for damage.
    pub health_change: i32,
    /// The change of the target's stamina.
    pub stamina_change: i32,
    /// The damage types.
    pub damage_types: DamageTypes,
    /// The attack categories.
    pub attack: AttackCategories,
    /// The target's faction, the engine's `FactionType`.
    pub target_faction: u8,
}

impl sealed::Sealed for Hit {}

impl Kind for Hit {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::HIT_EVENT)?;
        let attacker: Field<Entity> = ty.field_by_id(load, ids::HIT_ATTACKER)?;
        let target: Field<Entity> = ty.field_by_id(load, ids::HIT_TARGET)?;
        let source: Field<Entity> = ty.field_by_id(load, ids::HIT_SOURCE)?;
        let root_target: Field<Entity> = ty.field_by_id(load, ids::HIT_ROOT_TARGET)?;
        let weapon: Field<Entity> = ty.field_by_id(load, ids::HIT_WEAPON)?;
        let health: Field<i32> = ty.field_by_id(load, ids::HIT_HEALTH_CHANGE)?;
        let stamina: Field<i32> = ty.field_by_id(load, ids::HIT_STAMINA_CHANGE)?;
        let damage: Field<u16> = ty.field_by_id(load, ids::HIT_DAMAGE_TYPES)?;
        let attack: Field<u8> = ty.field_by_id(load, ids::HIT_ATTACK)?;
        let faction: Field<u8> = ty.field_by_id(load, ids::HIT_TARGET_FACTION)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    attacker: some(e.get(attacker)?),
                    source: some(e.get(source)?),
                    target: e.get(target)?,
                    root_target: some(e.get(root_target)?),
                    weapon: some(e.get(weapon)?),
                    health_change: e.get(health)?,
                    stamina_change: e.get(stamina)?,
                    damage_types: DamageTypes(e.get(damage)?),
                    attack: AttackCategories(e.get(attack)?),
                    target_faction: e.get(faction)?,
                })
            },
            handler,
        )
    }
}

/// An entity died, with who killed it as far as the engine kept track,
/// read while the dead entity is still in the world.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::Died;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let world = load.world()?;
///     load.on_game::<Died>(move |died| {
///         if let Some(player) = died.killer_player.and_then(|p| world.player(p)) {
///             enshwick::info!("{} killed entity {}", player.name, died.entity.0);
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Died {
    /// The entity that died.
    pub entity: Entity,
    /// The attacker of the last hit it took, from the hits it keeps.
    pub killer: Option<Entity>,
    /// The last player character that hit it, from its kill credit:
    /// the one the game credits with the experience.
    pub killer_player: Option<Entity>,
}

impl sealed::Sealed for Died {}

/// Where the kill credit lives on a dying entity.
#[derive(Clone, Copy, Debug)]
struct Credit {
    world: World,
    history: ComponentType,
    last_index: usize,
    entries: usize,
    slots: usize,
    entry_size: usize,
    entry_attacker: usize,
    experience: ComponentType,
    last_player: usize,
}

impl Credit {
    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        let world = load.world()?;
        let offset = |id: &str| {
            load.constant(id).and_then(|o| {
                usize::try_from(o).map_err(|_| Off::new(format!("{id}: out of range")))
            })
        };
        let size = usize::try_from(load.type_size(ids::HIT_HISTORY)?).unwrap_or(0);
        let source_size = usize::try_from(load.type_size(ids::EXPERIENCE_CREDIT)?).unwrap_or(0);
        let entries = offset(ids::HIT_HISTORY_ENTRIES)?;
        let entry_size = usize::try_from(load.type_size(ids::HIT_EVENT)?)
            .unwrap_or(0)
            .max(1);
        let slots = offset(ids::HIT_HISTORY_END)?.saturating_sub(entries) / entry_size;
        Ok(Self {
            history: world.component_type(load, ids::HIT_HISTORY)?,
            last_index: offset(ids::HIT_HISTORY_LAST_INDEX)?,
            entries,
            slots,
            entry_size,
            entry_attacker: offset(ids::HIT_ATTACKER)?,
            experience: world.component_type(load, ids::EXPERIENCE_CREDIT)?,
            last_player: offset(ids::EXPERIENCE_CREDIT_LAST_PLAYER)?,
            world,
        })
        .and_then(|credit: Self| {
            let id = size_of::<u32>();
            (credit.last_index + id <= size
                && credit.entries + credit.slots * credit.entry_size <= size
                && credit.entry_attacker + id <= credit.entry_size
                && credit.last_player + id <= source_size)
                .then_some(credit)
                .ok_or_else(|| Off::new("kill credit: an unexpected layout"))
        })
    }

    /// The attacker of the last hit `entity` took.
    fn killer(&self, entity: Entity) -> Option<Entity> {
        let history = self.world.component(entity, self.history)?.cast::<u8>();
        // SAFETY: `component` returned the entity's live HitHistory, valid
        // until the game thread moves on; `resolve` checked that the u32 at
        // `lastIndex` lies inside its reflected size.
        let last: u32 = unsafe { read_at(history, self.last_index) };
        let last = usize::try_from(last).ok().filter(|&i| i < self.slots)?;
        let at = self.entries + last * self.entry_size + self.entry_attacker;
        // SAFETY: `history` is the entity's live HitHistory; `last < slots`
        // was checked above, and `resolve` checked that every slot, and the
        // u32 attacker inside it, lies inside its reflected size.
        some(Entity(unsafe { read_at(history, at) }))
    }

    /// The last player character that hit `entity`.
    fn killer_player(&self, entity: Entity) -> Option<Entity> {
        let source = self.world.component(entity, self.experience)?.cast::<u8>();
        // SAFETY: `component` returned the entity's live ExperienceSource,
        // valid until the game thread moves on; `resolve` checked that the u32
        // at `lastHitPlayerId` lies inside its reflected size.
        some(Entity(unsafe { read_at(source, self.last_player) }))
    }
}

impl Kind for Died {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::DIED_EVENT)?;
        let dead: Field<Entity> = ty.field_by_id(load, ids::DIED_ENTITY)?;
        let credit = Credit::resolve(load)
            .inspect_err(|off| crate::debug!("kill credit off: {off}"))
            .ok();
        on(
            load,
            ty,
            move |e| {
                let entity = e.get(dead)?;
                Some(Self {
                    entity,
                    killer: credit.as_ref().and_then(|c| c.killer(entity)),
                    killer_player: credit.as_ref().and_then(|c| c.killer_player(entity)),
                })
            },
            handler,
        )
    }
}

/// A character reached a new level.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::LevelGained;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<LevelGained>(|up| enshwick::info!("entity {} reached level {}", up.player.0, up.level))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LevelGained {
    /// The player's character.
    pub player: Entity,
    /// The level reached.
    pub level: u32,
}

impl sealed::Sealed for LevelGained {}

impl Kind for LevelGained {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::LEVEL_GAINED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::LEVEL_GAINED_PLAYER)?;
        let level: Field<i32> = ty.field_by_id(load, ids::LEVEL_GAINED_LEVEL)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player: e.get(player)?,
                    level: u32::try_from(e.get(level)?).ok()?,
                })
            },
            handler,
        )
    }
}

/// Where experience came from, the engine's `ExperienceSourceType`.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::ExperienceSource;
///
/// assert_eq!(ExperienceSource::from_raw(4), ExperienceSource::Combat);
/// assert_eq!(ExperienceSource::from_raw(40), ExperienceSource::Other(40));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExperienceSource {
    /// No source given.
    None,
    /// Exploring.
    Exploration,
    /// Reading lore.
    Lore,
    /// A quest.
    Quest,
    /// A kill.
    Combat,
    /// Mining.
    Mining,
    /// A completed item set.
    ItemSetCompleted,
    /// A value this crate does not name.
    Other(u8),
}

impl ExperienceSource {
    /// The source the engine's value names.
    ///
    /// # Examples
    ///
    /// See [`ExperienceSource`].
    #[must_use]
    pub const fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Exploration,
            2 => Self::Lore,
            3 => Self::Quest,
            4 => Self::Combat,
            5 => Self::Mining,
            6 => Self::ItemSetCompleted,
            other => Self::Other(other),
        }
    }
}

/// A character gained experience.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::{ExperienceGained, ExperienceSource};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<ExperienceGained>(|gained| {
///         if gained.source == ExperienceSource::Combat {
///             enshwick::debug!("{} experience for a kill", gained.amount);
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExperienceGained {
    /// The player's character.
    pub player: Entity,
    /// How much.
    pub amount: u32,
    /// Where from.
    pub source: ExperienceSource,
}

impl sealed::Sealed for ExperienceGained {}

impl Kind for ExperienceGained {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::EXPERIENCE_GAINED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::EXPERIENCE_GAINED_PLAYER)?;
        let amount: Field<u32> = ty.field_by_id(load, ids::EXPERIENCE_GAINED_AMOUNT)?;
        let source: Field<u8> = ty.field_by_id(load, ids::EXPERIENCE_GAINED_SOURCE)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player: e.get(player)?,
                    amount: e.get(amount)?,
                    source: ExperienceSource::from_raw(e.get(source)?),
                })
            },
            handler,
        )
    }
}

/// A player asked a station to craft, before the server checks
/// ingredients.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::CraftRequested;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<CraftRequested>(|craft| enshwick::debug!("recipe {:#x} x{}", craft.recipe, craft.amount))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CraftRequested {
    /// The crafting player's character.
    pub player: Entity,
    /// The station, or none for crafting by hand.
    pub station: Option<Entity>,
    /// The recipe id.
    pub recipe: u32,
    /// How many.
    pub amount: u16,
}

impl sealed::Sealed for CraftRequested {}

impl Kind for CraftRequested {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::CRAFT_REQUESTED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::CRAFT_REQUESTED_PLAYER)?;
        let station: Field<Entity> = ty.field_by_id(load, ids::CRAFT_REQUESTED_STATION)?;
        let recipe: Field<u32> = ty.field_by_id(load, ids::CRAFT_REQUESTED_RECIPE)?;
        let amount: Field<u16> = ty.field_by_id(load, ids::CRAFT_REQUESTED_AMOUNT)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player: e.get(player)?,
                    station: some(e.get(station)?),
                    recipe: e.get(recipe)?,
                    amount: e.get(amount)?,
                })
            },
            handler,
        )
    }
}

/// A workshop finished a recipe.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::Crafted;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<Crafted>(|done| enshwick::debug!("workshop {:#x} made {:#x}", done.workshop, done.recipe))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Crafted {
    /// The workshop id.
    pub workshop: u32,
    /// The recipe id.
    pub recipe: u32,
}

impl sealed::Sealed for Crafted {}

impl Kind for Crafted {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::CRAFTED_EVENT)?;
        let workshop: Field<u32> = ty.field_by_id(load, ids::CRAFTED_WORKSHOP)?;
        let recipe: Field<u32> = ty.field_by_id(load, ids::CRAFTED_RECIPE)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    workshop: e.get(workshop)?,
                    recipe: e.get(recipe)?,
                })
            },
            handler,
        )
    }
}

/// A player accepted an interaction an entity offered, such as opening a
/// door or talking to a survivor.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::Interacted;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<Interacted>(|used| enshwick::debug!("{:?} used {:?}", used.player, used.target))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Interacted {
    /// The player's character.
    pub player: Entity,
    /// The entity that offered the interaction.
    pub target: Entity,
    /// Which of the target's offers.
    pub offer: u32,
}

impl sealed::Sealed for Interacted {}

impl Kind for Interacted {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::INTERACTED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::INTERACTED_PLAYER)?;
        let target: Field<Entity> = ty.field_by_id(load, ids::INTERACTED_TARGET)?;
        let offer: Field<u32> = ty.field_by_id(load, ids::INTERACTED_OFFER)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player: e.get(player)?,
                    target: e.get(target)?,
                    offer: e.get(offer)?,
                })
            },
            handler,
        )
    }
}

/// What a player did to a building block, from the engine's building
/// events.
///
/// # Examples
///
/// See [`Built`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Block {
    /// The player's character.
    pub player: Entity,
    /// Where, in metres, as the engine records it.
    pub position: [f32; 3],
    /// The block's volume, its lower corner, in metres.
    pub volume_min: [f32; 3],
    /// The block's volume, its upper corner, in metres.
    pub volume_max: [f32; 3],
    /// The material's feedback id.
    pub material: u32,
}

/// The ids of the fields every building event shares, one set per event.
#[derive(Clone, Copy, Debug)]
struct BlockIds {
    player: &'static str,
    position: &'static str,
    min: &'static str,
    max: &'static str,
    material: &'static str,
}

/// The fields every building event shares.
#[derive(Clone, Copy, Debug)]
struct BlockFields {
    player: Field<Entity>,
    position: [Field<f32>; 3],
    min: [Field<f32>; 3],
    max: [Field<f32>; 3],
    material: Field<u32>,
}

impl BlockFields {
    fn resolve(load: &Load<'_>, ty: EventType, ids: BlockIds) -> Result<Self, Off> {
        let vector = |id: &str| -> Result<[Field<f32>; 3], Off> {
            let at: Field<f32> = ty.field_by_id(load, id)?;
            let next = |step: u64| {
                let offset = u64::from(at.offset()) + step;
                ty.field_at::<f32>(offset)
                    .ok_or_else(|| Off::new(format!("{id}: past the end of {}", ty.name())))
            };
            Ok([at, next(4)?, next(8)?])
        };
        Ok(Self {
            player: ty.field_by_id(load, ids.player)?,
            position: vector(ids.position)?,
            min: vector(ids.min)?,
            max: vector(ids.max)?,
            material: ty.field_by_id(load, ids.material)?,
        })
    }

    fn read(&self, e: &RawGameEvent<'_>) -> Option<Block> {
        let vector = |fields: [Field<f32>; 3]| -> Option<[f32; 3]> {
            Some([e.get(fields[0])?, e.get(fields[1])?, e.get(fields[2])?])
        };
        Some(Block {
            player: e.get(self.player)?,
            position: vector(self.position)?,
            volume_min: vector(self.min)?,
            volume_max: vector(self.max)?,
            material: e.get(self.material)?,
        })
    }
}

/// A player placed a building block. Props and
/// furniture come as [`BuildingPlaced`](crate::events::BuildingPlaced) too.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::Built;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<Built>(|built| enshwick::debug!("{:?} built at {:?}", built.block.player, built.block.position))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Built {
    /// The block.
    pub block: Block,
    /// The item the block was built from.
    pub item: u32,
}

impl sealed::Sealed for Built {}

impl Kind for Built {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::BUILT_EVENT)?;
        let block = BlockFields::resolve(
            load,
            ty,
            BlockIds {
                player: ids::BUILT_PLAYER,
                position: ids::BUILT_POSITION,
                min: ids::BUILT_VOLUME_MIN,
                max: ids::BUILT_VOLUME_MAX,
                material: ids::BUILT_MATERIAL,
            },
        )?;
        let item: Field<u32> = ty.field_by_id(load, ids::BUILT_ITEM)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    block: block.read(e)?,
                    item: e.get(item)?,
                })
            },
            handler,
        )
    }
}

/// A player tore down building blocks.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::TornDown;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<TornDown>(|torn| enshwick::debug!("torn down at {:?}", torn.block.position))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct TornDown {
    /// The block.
    pub block: Block,
}

impl sealed::Sealed for TornDown {}

impl Kind for TornDown {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::TORN_DOWN_EVENT)?;
        let block = BlockFields::resolve(
            load,
            ty,
            BlockIds {
                player: ids::TORN_DOWN_PLAYER,
                position: ids::TORN_DOWN_POSITION,
                min: ids::TORN_DOWN_VOLUME_MIN,
                max: ids::TORN_DOWN_VOLUME_MAX,
                material: ids::TORN_DOWN_MATERIAL,
            },
        )?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    block: block.read(e)?,
                })
            },
            handler,
        )
    }
}

/// A player picked up a placed piece.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::PickedUp;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<PickedUp>(|picked| enshwick::debug!("picked up {:?}", picked.entity))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct PickedUp {
    /// The block.
    pub block: Block,
    /// The entity picked up.
    pub entity: Option<Entity>,
}

impl sealed::Sealed for PickedUp {}

impl Kind for PickedUp {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::PICKED_UP_EVENT)?;
        let block = BlockFields::resolve(
            load,
            ty,
            BlockIds {
                player: ids::PICKED_UP_PLAYER,
                position: ids::PICKED_UP_POSITION,
                min: ids::PICKED_UP_VOLUME_MIN,
                max: ids::PICKED_UP_VOLUME_MAX,
                material: ids::PICKED_UP_MATERIAL,
            },
        )?;
        let picked: Field<Entity> = ty.field_by_id(load, ids::PICKED_UP_ENTITY)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    block: block.read(e)?,
                    entity: some(e.get(picked)?),
                })
            },
            handler,
        )
    }
}

/// What a player did at a Flame Altar, the engine's `FlameAltarActionType`.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::AltarAction;
///
/// assert_eq!(AltarAction::from_raw(1), AltarAction::Reclaim);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AltarAction {
    /// Abandoned the base.
    Abandon,
    /// Reclaimed an abandoned base.
    Reclaim,
    /// Removed the water.
    RemoveWater,
    /// Made it the preferred respawn altar.
    SetSpawn,
    /// A value this crate does not name.
    Other(u8),
}

impl AltarAction {
    /// The action the engine's value names.
    ///
    /// # Examples
    ///
    /// See [`AltarAction`].
    #[must_use]
    pub const fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Abandon,
            1 => Self::Reclaim,
            2 => Self::RemoveWater,
            3 => Self::SetSpawn,
            other => Self::Other(other),
        }
    }
}

/// A player acted at a Flame Altar.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::AltarUsed;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<AltarUsed>(|used| enshwick::info!("{:?} at altar {:?}", used.action, used.altar))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AltarUsed {
    /// The player's character.
    pub player: Entity,
    /// The altar's base entity.
    pub altar: Entity,
    /// What was done.
    pub action: AltarAction,
}

impl sealed::Sealed for AltarUsed {}

impl Kind for AltarUsed {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::ALTAR_USED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::ALTAR_USED_PLAYER)?;
        let altar: Field<Entity> = ty.field_by_id(load, ids::ALTAR_USED_ALTAR)?;
        let kind: Field<u8> = ty.field_by_id(load, ids::ALTAR_USED_ACTION)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player: e.get(player)?,
                    altar: e.get(altar)?,
                    action: AltarAction::from_raw(e.get(kind)?),
                })
            },
            handler,
        )
    }
}

/// A player opened loot, a chest or a container.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::LootOpened;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<LootOpened>(|loot| enshwick::debug!("{:?} looted {:?}", loot.player, loot.container))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LootOpened {
    /// The player's character.
    pub player: Entity,
    /// The container.
    pub container: Entity,
    /// Whether the player took everything at once.
    pub take_all: bool,
}

impl sealed::Sealed for LootOpened {}

impl Kind for LootOpened {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::LOOT_OPENED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::LOOT_OPENED_PLAYER)?;
        let container: Field<Entity> = ty.field_by_id(load, ids::LOOT_OPENED_CONTAINER)?;
        let all: Field<bool> = ty.field_by_id(load, ids::LOOT_OPENED_TAKE_ALL)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player: e.get(player)?,
                    container: e.get(container)?,
                    take_all: e.get(all)?,
                })
            },
            handler,
        )
    }
}

/// A player's equipment slot changed.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::EquipmentChanged;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<EquipmentChanged>(|changed| enshwick::debug!("slot {} of {:?}", changed.slot, changed.player))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EquipmentChanged {
    /// The player's character.
    pub player: Entity,
    /// The slot, the engine's `EquipmentSlot`: 5 to 9 armour from head to
    /// feet, 10 and 11 rings, 12 the ranged weapon, 13 the shield.
    pub slot: u8,
}

impl sealed::Sealed for EquipmentChanged {}

impl Kind for EquipmentChanged {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::EQUIPMENT_CHANGED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::EQUIPMENT_CHANGED_PLAYER)?;
        let slot: Field<u8> = ty.field_by_id(load, ids::EQUIPMENT_CHANGED_SLOT)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player: e.get(player)?,
                    slot: e.get(slot)?,
                })
            },
            handler,
        )
    }
}

/// A downed player was revived.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::Revived;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<Revived>(|revived| enshwick::info!("{:?} revived by {:?}", revived.player, revived.by))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Revived {
    /// The revived player's character.
    pub player: Entity,
    /// Who revived them.
    pub by: Option<Entity>,
}

impl sealed::Sealed for Revived {}

impl Kind for Revived {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::REVIVED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::REVIVED_PLAYER)?;
        let by: Field<Entity> = ty.field_by_id(load, ids::REVIVED_BY)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player: e.get(player)?,
                    by: some(e.get(by)?),
                })
            },
            handler,
        )
    }
}
