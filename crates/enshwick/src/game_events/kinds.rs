//! Typed game events: a struct per event with its fields read for the
//! running build, for [`Load::on_game`](crate::Load::on_game).

use crate::ecs::Entity;
use crate::engine_enum::{Bits, Values};
use crate::engine_ids as ids;
use crate::game_events::values::{
    AttackCategoryMask, DamageTypeMask, EquipmentSlot, ExperienceSourceType, FlameAltarActionType,
};
use crate::game_events::{EventType, Field, RawGameEvent};
use crate::raw::read_at;
use crate::world::{ComponentType, World};
use crate::{Load, Off};

pub(super) mod sealed {
    pub trait Sealed {}
}

/// A game event with a typed form; implemented by the types in this
/// module only.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::{EntityDiedEvent, Kind};
///
/// fn watch<E: Kind + core::fmt::Debug>(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<E>(|event| enshwick::debug!("{event:?}"))
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     watch::<EntityDiedEvent>(load)
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

/// A hit the server resolved, damage or healing, written where health
/// changes. Fall and shroud damage change health without a hit.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::HitEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<HitEvent>(|hit| {
///         if hit.health_change < 0 {
///             enshwick::debug!("{:?} took {} from {:?}", hit.target_id, -hit.health_change, hit.root_source_id);
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct HitEvent {
    /// Who the hit is credited to: a character or enemy, not its arrow.
    pub root_source_id: Option<Entity>,
    /// What dealt it: the attacker itself, a projectile or an effect.
    pub source_id: Option<Entity>,
    /// The entity hit.
    pub target_id: Entity,
    /// The root of what was hit: the actor a hit body part belongs to.
    pub root_target_id: Option<Entity>,
    /// The weapon's item entity.
    pub weapon_pide_id: Option<Entity>,
    /// The change of the target's health: below 0 for damage.
    pub health_change: i32,
    /// The change of the target's stamina.
    pub stamina_change: i32,
    /// The damage types.
    pub damage_types: DamageTypeMask,
    /// The attack categories.
    pub attack_category: AttackCategoryMask,
    /// The target's faction, the engine's `FactionType`.
    pub target_faction: u8,
}

impl Kind for HitEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::HIT_EVENT)?;
        let attacker: Field<Entity> = ty.field_by_id(load, ids::HIT_EVENT_ROOT_SOURCE_ID)?;
        let target: Field<Entity> = ty.field_by_id(load, ids::HIT_EVENT_TARGET_ID)?;
        let source: Field<Entity> = ty.field_by_id(load, ids::HIT_EVENT_SOURCE_ID)?;
        let root_target: Field<Entity> = ty.field_by_id(load, ids::HIT_EVENT_ROOT_TARGET_ID)?;
        let weapon: Field<Entity> = ty.field_by_id(load, ids::HIT_EVENT_WEAPON_PIDE_ID)?;
        let health: Field<i32> = ty.field_by_id(load, ids::HIT_HEALTH_CHANGE)?;
        let stamina: Field<i32> = ty.field_by_id(load, ids::HIT_STAMINA_CHANGE)?;
        let damage: Field<u16> = ty.field_by_id(load, ids::HIT_DAMAGE_TYPES)?;
        let attack: Field<u8> = ty.field_by_id(load, ids::HIT_EVENT_ATTACK_CATEGORY)?;
        let faction: Field<u8> = ty.field_by_id(load, ids::HIT_TARGET_FACTION)?;
        let damage_bits = Bits::resolve(load, DamageTypeMask::IDS.map(|(t, id)| (t.bits(), id)));
        let attack_bits = Bits::resolve(
            load,
            AttackCategoryMask::IDS.map(|(c, id)| (u16::from(c.bits()), id)),
        );
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    root_source_id: some(e.get(attacker)?),
                    source_id: some(e.get(source)?),
                    target_id: e.get(target)?,
                    root_target_id: some(e.get(root_target)?),
                    weapon_pide_id: some(e.get(weapon)?),
                    health_change: e.get(health)?,
                    stamina_change: e.get(stamina)?,
                    damage_types: DamageTypeMask::from_bits(damage_bits.map(e.get(damage)?)),
                    attack_category: AttackCategoryMask::from_bits(
                        u8::try_from(attack_bits.map(u16::from(e.get(attack)?))).ok()?,
                    ),
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
/// use enshwick::game_events::EntityDiedEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let world = load.world()?;
///     load.on_game::<EntityDiedEvent>(move |died| {
///         if let Some(player) = died.killer_player.and_then(|p| world.player(p)) {
///             enshwick::info!("{} killed entity {}", player.name, died.dead_entity_id.0);
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct EntityDiedEvent {
    /// The entity that died.
    pub dead_entity_id: Entity,
    /// The attacker of the last hit it took, from the hits it keeps; the
    /// name is enshwick's, not the game's.
    pub killer: Option<Entity>,
    /// The last player character that hit it, from its kill credit:
    /// the one the game credits with the experience; the name is
    /// enshwick's, not the game's.
    pub killer_player: Option<Entity>,
}

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
            entry_attacker: offset(ids::HIT_EVENT_ROOT_SOURCE_ID)?,
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
        // SAFETY: `component` returned the entity's live ExperienceSource component,
        // valid until the game thread moves on; `resolve` checked that the u32
        // at `lastHitPlayerId` lies inside its reflected size.
        some(Entity(unsafe { read_at(source, self.last_player) }))
    }
}

impl Kind for EntityDiedEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::ENTITY_DIED_EVENT)?;
        let dead: Field<Entity> = ty.field_by_id(load, ids::ENTITY_DIED_EVENT_DEAD_ENTITY_ID)?;
        let credit = Credit::resolve(load)
            .inspect_err(|off| crate::debug!("kill credit off: {off}"))
            .ok();
        on(
            load,
            ty,
            move |e| {
                let entity = e.get(dead)?;
                Some(Self {
                    dead_entity_id: entity,
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
/// use enshwick::game_events::LevelUpEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<LevelUpEvent>(|up| enshwick::info!("entity {} reached level {}", up.player_entity_id.0, up.level))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct LevelUpEvent {
    /// The player's character.
    pub player_entity_id: Entity,
    /// The level reached.
    pub level: u32,
}

impl Kind for LevelUpEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::LEVEL_UP_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::LEVEL_UP_EVENT_PLAYER_ENTITY_ID)?;
        let level: Field<i32> = ty.field_by_id(load, ids::LEVEL_UP_EVENT_LEVEL)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player_entity_id: e.get(player)?,
                    level: u32::try_from(e.get(level)?).ok()?,
                })
            },
            handler,
        )
    }
}

/// A character gained experience.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::{XpGainedEvent, ExperienceSourceType};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<XpGainedEvent>(|gained| {
///         if gained.source == ExperienceSourceType::Combat {
///             enshwick::debug!("{} experience for a kill", gained.amount);
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct XpGainedEvent {
    /// The player's character.
    pub player_entity_id: Entity,
    /// How much.
    pub amount: u32,
    /// Where from.
    pub source: ExperienceSourceType,
}

impl Kind for XpGainedEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::XP_GAINED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::XP_GAINED_EVENT_PLAYER_ENTITY_ID)?;
        let amount: Field<u32> = ty.field_by_id(load, ids::XP_GAINED_EVENT_AMOUNT)?;
        let source: Field<u8> = ty.field_by_id(load, ids::XP_GAINED_EVENT_SOURCE)?;
        let sources = Values::resolve(load, ExperienceSourceType::IDS);
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player_entity_id: e.get(player)?,
                    amount: e.get(amount)?,
                    source: {
                        let raw = e.get(source)?;
                        sources
                            .variant(u64::from(raw))
                            .unwrap_or(ExperienceSourceType::Other(raw))
                    },
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
/// use enshwick::game_events::CraftRecipeEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<CraftRecipeEvent>(|craft| enshwick::debug!("recipe {:#x} x{}", craft.recipe_id, craft.recipe_amount))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct CraftRecipeEvent {
    /// The crafting player's character.
    pub crafting_operator_id: Entity,
    /// The station, or none for crafting by hand.
    pub crafting_station_id: Option<Entity>,
    /// The recipe id.
    pub recipe_id: u32,
    /// How many.
    pub recipe_amount: u16,
}

impl Kind for CraftRecipeEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::CRAFT_RECIPE_EVENT)?;
        let player: Field<Entity> =
            ty.field_by_id(load, ids::CRAFT_RECIPE_EVENT_CRAFTING_OPERATOR_ID)?;
        let station: Field<Entity> =
            ty.field_by_id(load, ids::CRAFT_RECIPE_EVENT_CRAFTING_STATION_ID)?;
        let recipe: Field<u32> = ty.field_by_id(load, ids::CRAFT_RECIPE_EVENT_RECIPE_ID)?;
        let amount: Field<u16> = ty.field_by_id(load, ids::CRAFT_RECIPE_EVENT_RECIPE_AMOUNT)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    crafting_operator_id: e.get(player)?,
                    crafting_station_id: some(e.get(station)?),
                    recipe_id: e.get(recipe)?,
                    recipe_amount: e.get(amount)?,
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
/// use enshwick::game_events::CraftRecipeSuccessEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<CraftRecipeSuccessEvent>(|done| enshwick::debug!("workshop {:#x} made {:#x}", done.workshop_id, done.recipe_id))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct CraftRecipeSuccessEvent {
    /// The workshop id.
    pub workshop_id: u32,
    /// The recipe id.
    pub recipe_id: u32,
}

impl Kind for CraftRecipeSuccessEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::CRAFT_RECIPE_SUCCESS_EVENT)?;
        let workshop: Field<u32> =
            ty.field_by_id(load, ids::CRAFT_RECIPE_SUCCESS_EVENT_WORKSHOP_ID)?;
        let recipe: Field<u32> = ty.field_by_id(load, ids::CRAFT_RECIPE_SUCCESS_EVENT_RECIPE_ID)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    workshop_id: e.get(workshop)?,
                    recipe_id: e.get(recipe)?,
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
/// use enshwick::game_events::AcceptOfferedInteractionEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<AcceptOfferedInteractionEvent>(|used| enshwick::debug!("{:?} used {:?}", used.guest_entity_id, used.host_entity_id))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
#[expect(
    clippy::struct_field_names,
    reason = "the fields take the game's names, which all end in Id"
)]
pub struct AcceptOfferedInteractionEvent {
    /// The player's character.
    pub guest_entity_id: Entity,
    /// The entity that offered the interaction.
    pub host_entity_id: Entity,
    /// Which of the target's offers.
    pub offer_id: u32,
}

impl Kind for AcceptOfferedInteractionEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::ACCEPT_OFFERED_INTERACTION_EVENT)?;
        let player: Field<Entity> =
            ty.field_by_id(load, ids::ACCEPT_OFFERED_INTERACTION_EVENT_GUEST_ENTITY_ID)?;
        let target: Field<Entity> =
            ty.field_by_id(load, ids::ACCEPT_OFFERED_INTERACTION_EVENT_HOST_ENTITY_ID)?;
        let offer: Field<u32> =
            ty.field_by_id(load, ids::ACCEPT_OFFERED_INTERACTION_EVENT_OFFER_ID)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    guest_entity_id: e.get(player)?,
                    host_entity_id: e.get(target)?,
                    offer_id: e.get(offer)?,
                })
            },
            handler,
        )
    }
}

/// What a player did to a building block, the fields the engine's building
/// events share; the name is enshwick's, not the game's.
///
/// # Examples
///
/// See [`BuildingPlaceEvent`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct Block {
    /// The player's character.
    pub owner_id: Entity,
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
    owner_id: &'static str,
    position: &'static str,
    min: &'static str,
    max: &'static str,
    material: &'static str,
}

/// The fields every building event shares.
#[derive(Clone, Copy, Debug)]
struct BlockFields {
    owner_id: Field<Entity>,
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
            owner_id: ty.field_by_id(load, ids.owner_id)?,
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
            owner_id: e.get(self.owner_id)?,
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
/// use enshwick::game_events::BuildingPlaceEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<BuildingPlaceEvent>(|built| enshwick::debug!("{:?} built at {:?}", built.block.owner_id, built.block.position))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct BuildingPlaceEvent {
    /// The block.
    pub block: Block,
    /// The item the block was built from.
    pub tracking_item_id: u32,
}

impl Kind for BuildingPlaceEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::BUILDING_PLACE_EVENT)?;
        let block = BlockFields::resolve(
            load,
            ty,
            BlockIds {
                owner_id: ids::BUILDING_PLACE_EVENT_OWNER_ID,
                position: ids::BUILDING_PLACE_EVENT_POSITION,
                min: ids::BUILDING_PLACE_EVENT_VOLUME_MIN,
                max: ids::BUILDING_PLACE_EVENT_VOLUME_MAX,
                material: ids::BUILDING_PLACE_EVENT_MATERIAL,
            },
        )?;
        let item: Field<u32> = ty.field_by_id(load, ids::BUILDING_PLACE_EVENT_TRACKING_ITEM_ID)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    block: block.read(e)?,
                    tracking_item_id: e.get(item)?,
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
/// use enshwick::game_events::BuildingTearDownEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<BuildingTearDownEvent>(|torn| enshwick::debug!("torn down at {:?}", torn.block.position))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct BuildingTearDownEvent {
    /// The block.
    pub block: Block,
}

impl Kind for BuildingTearDownEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::BUILDING_TEAR_DOWN_EVENT)?;
        let block = BlockFields::resolve(
            load,
            ty,
            BlockIds {
                owner_id: ids::BUILDING_TEAR_DOWN_EVENT_OWNER_ID,
                position: ids::BUILDING_TEAR_DOWN_EVENT_POSITION,
                min: ids::BUILDING_TEAR_DOWN_EVENT_VOLUME_MIN,
                max: ids::BUILDING_TEAR_DOWN_EVENT_VOLUME_MAX,
                material: ids::BUILDING_TEAR_DOWN_EVENT_MATERIAL,
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
/// use enshwick::game_events::BuildingPickUpEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<BuildingPickUpEvent>(|picked| enshwick::debug!("picked up {:?}", picked.picked_entity_id))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct BuildingPickUpEvent {
    /// The block.
    pub block: Block,
    /// The entity picked up.
    pub picked_entity_id: Option<Entity>,
}

impl Kind for BuildingPickUpEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::BUILDING_PICK_UP_EVENT)?;
        let block = BlockFields::resolve(
            load,
            ty,
            BlockIds {
                owner_id: ids::BUILDING_PICK_UP_EVENT_OWNER_ID,
                position: ids::BUILDING_PICK_UP_EVENT_POSITION,
                min: ids::BUILDING_PICK_UP_EVENT_VOLUME_MIN,
                max: ids::BUILDING_PICK_UP_EVENT_VOLUME_MAX,
                material: ids::BUILDING_PICK_UP_EVENT_MATERIAL,
            },
        )?;
        let picked: Field<Entity> =
            ty.field_by_id(load, ids::BUILDING_PICK_UP_EVENT_PICKED_ENTITY_ID)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    block: block.read(e)?,
                    picked_entity_id: some(e.get(picked)?),
                })
            },
            handler,
        )
    }
}

/// A player acted at a Flame Altar.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::FlameAltarEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<FlameAltarEvent>(|used| enshwick::info!("{:?} at altar {:?}", used.r#type, used.base_entity_id))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct FlameAltarEvent {
    /// The player's character.
    pub player_entity_id: Entity,
    /// The altar's base entity.
    pub base_entity_id: Entity,
    /// What was done.
    pub r#type: FlameAltarActionType,
}

impl Kind for FlameAltarEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::FLAME_ALTAR_EVENT)?;
        let player: Field<Entity> =
            ty.field_by_id(load, ids::FLAME_ALTAR_EVENT_PLAYER_ENTITY_ID)?;
        let altar: Field<Entity> = ty.field_by_id(load, ids::FLAME_ALTAR_EVENT_BASE_ENTITY_ID)?;
        let kind: Field<u8> = ty.field_by_id(load, ids::FLAME_ALTAR_EVENT_TYPE)?;
        let actions = Values::resolve(load, FlameAltarActionType::IDS);
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player_entity_id: e.get(player)?,
                    base_entity_id: e.get(altar)?,
                    r#type: {
                        let raw = e.get(kind)?;
                        actions
                            .variant(u64::from(raw))
                            .unwrap_or(FlameAltarActionType::Other(raw))
                    },
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
/// use enshwick::game_events::LootInteractionEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<LootInteractionEvent>(|loot| enshwick::debug!("{:?} looted {:?}", loot.guest_id, loot.host_id))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct LootInteractionEvent {
    /// The player's character.
    pub guest_id: Entity,
    /// The container.
    pub host_id: Entity,
    /// Whether the player took everything at once.
    pub loot_all: bool,
}

impl Kind for LootInteractionEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::LOOT_INTERACTION_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(load, ids::LOOT_INTERACTION_EVENT_GUEST_ID)?;
        let container: Field<Entity> = ty.field_by_id(load, ids::LOOT_INTERACTION_EVENT_HOST_ID)?;
        let all: Field<bool> = ty.field_by_id(load, ids::LOOT_INTERACTION_EVENT_LOOT_ALL)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    guest_id: e.get(player)?,
                    host_id: e.get(container)?,
                    loot_all: e.get(all)?,
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
/// use enshwick::game_events::HandleEquipmentSlotChangedEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<HandleEquipmentSlotChangedEvent>(|changed| enshwick::debug!("{:?} of {:?}", changed.equipment_slot, changed.player_entity_id))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct HandleEquipmentSlotChangedEvent {
    /// The player's character.
    pub player_entity_id: Entity,
    /// The slot.
    pub equipment_slot: EquipmentSlot,
}

impl Kind for HandleEquipmentSlotChangedEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT)?;
        let player: Field<Entity> = ty.field_by_id(
            load,
            ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_PLAYER_ENTITY_ID,
        )?;
        let slot: Field<u8> = ty.field_by_id(
            load,
            ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_EQUIPMENT_SLOT,
        )?;
        let slots = Values::resolve(load, EquipmentSlot::IDS);
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player_entity_id: e.get(player)?,
                    equipment_slot: {
                        let raw = e.get(slot)?;
                        slots
                            .variant(u64::from(raw))
                            .unwrap_or(EquipmentSlot::Other(raw))
                    },
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
/// use enshwick::game_events::RevivedPlayerEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_game::<RevivedPlayerEvent>(|revived| enshwick::info!("{:?} revived by {:?}", revived.target_entity_id, revived.source_entity_id))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct RevivedPlayerEvent {
    /// The revived player's character.
    pub target_entity_id: Entity,
    /// Who revived them.
    pub source_entity_id: Option<Entity>,
}

impl Kind for RevivedPlayerEvent {
    fn subscribe<F>(load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let ty = load.event_type(ids::REVIVED_PLAYER_EVENT)?;
        let player: Field<Entity> =
            ty.field_by_id(load, ids::REVIVED_PLAYER_EVENT_TARGET_ENTITY_ID)?;
        let by: Field<Entity> = ty.field_by_id(load, ids::REVIVED_PLAYER_EVENT_SOURCE_ENTITY_ID)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    target_entity_id: e.get(player)?,
                    source_entity_id: some(e.get(by)?),
                })
            },
            handler,
        )
    }
}
