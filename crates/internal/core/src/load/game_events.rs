//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::{EventTypeInfo, GameEventData, Header, Str};
use crate::{Entity, ItemId, PlayerKey, RecipeId};
use crate::{Load, Off};
use crate::{Load, Off};
use core::marker::PhantomData;
use kinds::{
    AcceptOfferedInteractionEvent, Block, BuildingPickUpEvent, BuildingPlaceEvent,
    BuildingTearDownEvent, CraftRecipeEvent, CraftRecipeSuccessEvent, EntityDiedEvent,
    FlameAltarEvent, HandleEquipmentSlotChangedEvent, HitEvent, LevelUpEvent, LootInteractionEvent,
    RevivedPlayerEvent, XpGainedEvent,
};
use values::{
    AttackCategoryMask, DamageTypeMask, EquipmentSlot, ExperienceSourceType, FlameAltarActionType,
};

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Every game event type of the running build, read from the game's
    /// reflection, for tools that name what they see: the event types that
    /// exist, not only those the server raises.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader cannot list them.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let types = load.game_event_types()?;
    ///     enshwick::info!("{} game event types on this build", types.len());
    ///     Ok(())
    /// }
    /// ```
    pub fn game_event_types(&self) -> Result<Vec<EventType>, Off>;
}
