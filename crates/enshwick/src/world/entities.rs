use crate::ecs::Entity;

use super::{ComponentType, ContainerKinds, Placed, World, WorldError};

impl World {
    /// Every entity with a component of type `ty` and a place in the world,
    /// such as every bed or every chest; with [`World::transform_type`],
    /// every placed entity. The engine walks every such archetype, so call
    /// it now and then, not every step. On the game thread only; empty
    /// elsewhere.
    ///
    /// # Examples
    ///
    /// ```
    /// fn count(load: &enshwick::Load<'_>, world: &enshwick::world::World) -> Result<usize, enshwick::Off> {
    ///     let beds = world.component_type(load, "keen::ecs::NpcBed")?;
    ///     Ok(world.entities(beds).len())
    /// }
    /// ```
    #[must_use]
    pub fn entities(self, ty: ComponentType) -> Vec<Placed> {
        self.placed_with(ty.0)
            .unwrap_or_default()
            .iter()
            .map(Placed::from_abi)
            .collect()
    }

    /// The entities with a component of type `ty` within `radius` metres of
    /// `point`, nearest first; with [`World::transform_type`], every placed
    /// entity there. On the game thread only; empty elsewhere.
    ///
    /// # Examples
    ///
    /// ```
    /// fn nearby(world: &enshwick::world::World, player: enshwick::ecs::Entity) -> usize {
    ///     let Some(at) = world.position(player) else { return 0 };
    ///     world.near(at, 20.0, world.transform_type()).len()
    /// }
    /// ```
    #[must_use]
    pub fn near(self, point: [f64; 3], radius: f64, ty: ComponentType) -> Vec<Placed> {
        let mut found: Vec<(f64, Placed)> = self
            .entities(ty)
            .into_iter()
            .map(|placed| (placed.distance(point), placed))
            .filter(|&(distance, _)| distance <= radius)
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.into_iter().map(|(_, placed)| placed).collect()
    }

    /// The component type every placed entity has, `CurrentTransform`.
    ///
    /// # Examples
    ///
    /// See [`World::near`].
    #[must_use]
    pub const fn transform_type(self) -> ComponentType {
        self.0.transform
    }

    /// The world's containers of a kind in `kinds`, with their places: by
    /// default what players build, such as storage chests; see
    /// [`ContainerKinds`] for the rest. Each kind is told apart by a
    /// component the game gives it. Call it now and then, as
    /// [`World::entities`]. On the game thread only; empty elsewhere and on
    /// a build that lacks any of those components.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Outcome};
    /// use enshwick::world::ContainerKinds;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     let actions = load.actions(enshwick::i18n::Texts::default())?;
    ///     actions.add(Action::new("chests", "Count the built chests"), move |_request| {
    ///         let built = world.containers(ContainerKinds::default()).len();
    ///         Outcome::done(format!("{built} built containers"))
    ///     })
    /// }
    /// ```
    #[must_use]
    pub fn containers(self, kinds: ContainerKinds) -> Vec<Placed> {
        self.0
            .containers
            .map(|containers| containers.of(self, kinds))
            .unwrap_or_default()
    }

    /// Asks the server to destroy `entity` after the systems of the next
    /// step, as the game removes things itself: no loot and no death.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a player's character,
    /// [`WorldError::Gone`] for an entity not in the world,
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Missing`] on any other refusal.
    ///
    /// # Examples
    ///
    /// ```
    /// fn clear(world: &enshwick::world::World, drop: enshwick::ecs::Entity) {
    ///     if let Err(why) = world.destroy(drop) {
    ///         enshwick::warn!("entity {} stays: {why}", drop.0);
    ///     }
    /// }
    /// ```
    pub fn destroy(self, entity: Entity) -> Result<(), WorldError> {
        WorldError::check(
            (self.0.host.api().destroy)(self.0.host.handle(), entity.0),
            WorldError::Gone,
        )
    }
}
