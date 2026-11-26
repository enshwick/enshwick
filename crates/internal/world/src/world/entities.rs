use crate::Entity;
use crate::abi::EntityPlace;

use super::{ComponentType, ContainerKinds, Placed, World, WorldError};

#[crate::script(entities)]
impl World {
    /// Every entity with a component of type `ty` and a place in the world,
    /// such as every bed or every chest; with [`World::transform_type`],
    /// every placed entity. The engine walks every such archetype, so call
    /// it now and then, not every step.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Unavailable`] where no server runs.
    ///
    /// # Examples
    ///
    /// ```
    /// fn count(load: &enshwick::Load<'_>, world: &enshwick::world::World) -> Result<usize, enshwick::Off> {
    ///     let beds = enshwick::ecs::ComponentType::resolve(load, "keen::ecs::NpcBed")?;
    ///     Ok(world.entities(beds)?.len())
    /// }
    /// ```
    #[script]
    pub fn entities(self, ty: ComponentType) -> Result<Vec<Placed>, WorldError> {
        Ok(self
            .placed_with(ty.hash())?
            .iter()
            .map(Placed::from_abi)
            .collect())
    }

    /// The entities with a component of type `ty` within `radius` metres of
    /// `point`, nearest first; with [`World::transform_type`], every placed
    /// entity there.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Unavailable`] where no server runs.
    ///
    /// # Examples
    ///
    /// ```
    /// fn nearby(world: &enshwick::world::World, player: enshwick::Entity) -> Result<usize, enshwick::WorldError> {
    ///     let Some(at) = world.position(player) else { return Ok(0) };
    ///     Ok(world.near(at, 20.0, world.transform_type())?.len())
    /// }
    /// ```
    #[script]
    pub fn near(
        self,
        point: [f64; 3],
        radius: f64,
        ty: ComponentType,
    ) -> Result<Vec<Placed>, WorldError> {
        let mut found: Vec<(f64, Placed)> = self
            .entities(ty)?
            .into_iter()
            .map(|placed| (placed.distance(point), placed))
            .filter(|&(distance, _)| distance <= radius)
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(found.into_iter().map(|(_, placed)| placed).collect())
    }

    /// The component type every placed entity has, `CurrentTransform`.
    ///
    /// # Examples
    ///
    /// See [`World::near`].
    #[must_use]
    #[script]
    pub const fn transform_type(self) -> ComponentType {
        self.0.transform
    }

    /// The world's containers of a kind in `kinds`, with their places: by
    /// default what players build, such as storage chests; see
    /// [`ContainerKinds`] for the rest. Each kind is told apart by a
    /// component the game gives it. Call it now and then, as
    /// [`World::entities`].
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Unavailable`] where no server runs or on a build that
    /// lacks any of those components.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::ContainerKinds;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     // Run on the game thread now and then: counts the built containers.
    ///     let _built = move || world.containers(ContainerKinds::default()).map(|found| found.len());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn containers(self, kinds: ContainerKinds) -> Result<Vec<Placed>, WorldError> {
        self.0
            .containers
            .as_ref()
            .ok_or(WorldError::Unavailable)?
            .of(self, kinds)
    }

    /// Asks the server to destroy `entity` after the systems of the next
    /// step, as the game removes things itself: no loot and no death.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a player's character,
    /// [`WorldError::Gone`] for an entity not in the world,
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Unavailable`] where no server runs, on a build without the
    /// command, or on any other refusal.
    ///
    /// # Examples
    ///
    /// ```
    /// fn clear(world: &enshwick::world::World, drop: enshwick::Entity) {
    ///     if let Err(why) = world.destroy(drop) {
    ///         enshwick::warn!("entity {} stays: {why}", drop);
    ///     }
    /// }
    /// ```
    #[script]
    pub fn destroy(self, entity: Entity) -> Result<(), WorldError> {
        WorldError::check(
            (self.0.host.api().destroy)(self.0.host.handle(), entity),
            WorldError::Gone,
        )
    }
}

impl World {
    /// Every entity with a component of type `hash` and a place in the
    /// world, as the loader answers.
    pub(crate) fn placed_with(self, hash: u32) -> Result<Vec<EntityPlace>, WorldError> {
        let entities = self.0.host.api().entities;
        let mut capacity = 1024u32;
        loop {
            let mut places = vec![EntityPlace::default(); capacity as usize];
            let mut count = 0u32;
            let status = entities(
                self.0.host.handle(),
                hash,
                places.as_mut_ptr(),
                capacity,
                &raw mut count,
            );
            WorldError::from_status(status)?;
            if count <= capacity {
                places.truncate(count as usize);
                return Ok(places);
            }
            capacity = count;
        }
    }
}
