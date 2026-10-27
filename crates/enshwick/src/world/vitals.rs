use crate::Entity;

use super::{Meter, Progress, Skill, Vitals, World, WorldError, stats};

impl World {
    /// `entity`'s health, stamina and mana; `None` when it has no health,
    /// off the game thread, or on a build without stat reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::events::PlayerJoined;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     load.on::<PlayerJoined>(move |joined| {
    ///         if let Some(vitals) = world.vitals(enshwick::Entity(joined.entity)) {
    ///             enshwick::info!("joined with {} of {} health", vitals.health.current, vitals.health.max);
    ///         }
    ///     })
    /// }
    /// ```
    #[must_use]
    pub fn vitals(self, entity: Entity) -> Option<Vitals> {
        self.0.stats?.vitals(self, entity)
    }

    /// Sets `entity`'s health to `value`, kept between 1 and its maximum, so
    /// a write never kills; returns the health it now has. The game sends it
    /// to clients and regenerates from it as from any other value. On the
    /// game thread only.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an entity already dead: death and revival
    /// stay the game's; otherwise as for [`World::update`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Outcome};
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     let actions = load.actions(enshwick::i18n::Texts::default())?;
    ///     actions.add(Action::new("full", "Restore your health"), move |request| {
    ///         let me = request.player.entity;
    ///         let max = world.vitals(me).map_or(0, |v| v.health.max);
    ///         match world.set_health(me, max) {
    ///             Ok(health) => Outcome::done(format!("{} of {} health", health.current, health.max)),
    ///             Err(why) => Outcome::refused(format!("{why}")),
    ///         }
    ///     })
    /// }
    /// ```
    pub fn set_health(self, entity: Entity, value: i32) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Health, |_| value)
    }

    /// Adds `amount` to `entity`'s health, kept between 1 and its maximum: a
    /// negative amount takes health but never kills. Returns the health it
    /// now has. On the game thread only.
    ///
    /// # Errors
    ///
    /// As for [`World::set_health`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::game_events::LevelUpEvent;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let world = load.world()?;
    ///     load.on::<LevelUpEvent>(move |gained| {
    ///         let _healed = world.heal(gained.player_entity_id, 50);
    ///     })
    /// }
    /// ```
    pub fn heal(self, entity: Entity, amount: i32) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Health, |current| {
            current.saturating_add(amount)
        })
    }

    /// Sets `entity`'s stamina to `value`, kept between 0 and its maximum;
    /// returns the stamina it now has. On the game thread only.
    ///
    /// # Errors
    ///
    /// [`WorldError::Lacks`] for an entity without stamina; otherwise as for
    /// [`World::update`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn refresh(world: &enshwick::world::World, player: &enshwick::world::Player) {
    ///     let max = world.vitals(player.entity).and_then(|v| v.stamina).map_or(0, |s| s.max);
    ///     let _refreshed = world.set_stamina(player.entity, max);
    /// }
    /// ```
    pub fn set_stamina(self, entity: Entity, value: i32) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Stamina, |_| value)
    }

    /// Sets `entity`'s mana to `value`, kept between 0 and its maximum;
    /// returns the mana it now has. On the game thread only.
    ///
    /// # Errors
    ///
    /// As for [`World::set_stamina`], for mana.
    ///
    /// # Examples
    ///
    /// ```
    /// fn drain(world: &enshwick::world::World, player: &enshwick::world::Player) {
    ///     let _drained = world.set_mana(player.entity, 0);
    /// }
    /// ```
    pub fn set_mana(self, entity: Entity, value: i32) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Mana, |_| value)
    }

    /// The meter of an item's own entity, its PIDE (see
    /// [`FuelSetup`](crate::content::items::FuelSetup)): the count it holds and
    /// its maximum. `None` for an entity without one, off the game thread,
    /// or on a build without the reads.
    ///
    /// # Examples
    ///
    /// ```
    /// fn charges(world: &enshwick::world::World, slot: &enshwick::inventory::Slot) -> i32 {
    ///     slot.pide.and_then(|pide| world.item_meter(pide)).map_or(0, |m| m.current)
    /// }
    /// ```
    #[must_use]
    pub fn item_meter(self, pide: Entity) -> Option<Meter> {
        self.0.stats?.meter(self, pide)
    }

    /// Sets the meter of an item's own entity to what `change` makes of it,
    /// kept between 0 and its maximum, and returns it. The game saves it
    /// with the world and shows it on the item's slot. On the game thread
    /// only.
    ///
    /// # Errors
    ///
    /// [`WorldError::Lacks`] for an entity without a meter;
    /// [`WorldError::Missing`] on a build without the writes; otherwise as
    /// for [`World::update`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn spend(world: &enshwick::world::World, pide: enshwick::Entity) -> bool {
    ///     world.change_item_meter(pide, |charges| charges - 1).is_ok()
    /// }
    /// ```
    pub fn change_item_meter(
        self,
        pide: Entity,
        change: impl FnOnce(i32) -> i32,
    ) -> Result<Meter, WorldError> {
        self.write_stat(pide, stats::Pool::Meter, change)
    }

    /// How long `entity`, a character, holds out in the shroud before it
    /// drains health, in seconds, and the most it holds; `None` for an
    /// entity without it, off the game thread, or on a build without it.
    /// It runs down in the shroud and refills outside.
    ///
    /// # Examples
    ///
    /// ```
    /// fn left(world: &enshwick::world::World, player: &enshwick::world::Player) -> i32 {
    ///     world.shroud_time(player.entity).map_or(0, |time| time.current)
    /// }
    /// ```
    #[must_use]
    pub fn shroud_time(self, entity: Entity) -> Option<Meter> {
        self.0.stats?.shroud(self, entity)
    }

    /// Sets how long `entity` holds out in the shroud to what `change` makes
    /// of it, in seconds, kept between 0 and its most, and returns it;
    /// clients see it from the next step. Not saved: the game refills it
    /// outside the shroud. On the game thread only.
    ///
    /// # Errors
    ///
    /// [`WorldError::Lacks`] for an entity without it;
    /// [`WorldError::Missing`] on a build without it; otherwise as for
    /// [`World::update`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn refill(world: &enshwick::world::World, player: &enshwick::world::Player) {
    ///     let _full = world.change_shroud_time(player.entity, |_| i32::MAX);
    /// }
    /// ```
    pub fn change_shroud_time(
        self,
        entity: Entity,
        change: impl FnOnce(i32) -> i32,
    ) -> Result<Meter, WorldError> {
        self.write_stat(entity, stats::Pool::Shroud, change)
    }

    /// Whether `entity`, a character, stands in the shroud where it drains
    /// them, outside the altars' reach; `None` for an entity that is no
    /// actor, off the game thread, or on a build without it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn exposed(world: &enshwick::world::World, player: &enshwick::world::Player) -> bool {
    ///     world.in_shroud(player.entity) == Some(true)
    /// }
    /// ```
    #[must_use]
    pub fn in_shroud(self, entity: Entity) -> Option<bool> {
        self.0.stats?.in_shroud(self, entity)
    }

    /// The skill nodes `entity`, a character, unlocked, with their levels;
    /// `None` for an entity without skills, off the game thread, or on a
    /// build without them. The game saves them with the character.
    ///
    /// # Examples
    ///
    /// ```
    /// fn skilled(world: &enshwick::world::World, player: &enshwick::world::Player) -> usize {
    ///     world.skills(player.entity).map_or(0, |skills| skills.len())
    /// }
    /// ```
    #[must_use]
    pub fn skills(self, entity: Entity) -> Option<Vec<Skill>> {
        self.0.stats?;
        stats::Stats::skills(self, entity)
    }

    pub(super) fn write_stat(
        self,
        entity: Entity,
        pool: stats::Pool,
        change: impl FnOnce(i32) -> i32,
    ) -> Result<Meter, WorldError> {
        self.0
            .stats
            .ok_or(WorldError::Missing)?
            .write(self, entity, pool, change)
    }

    /// `entity`'s level and experience; `None` for an entity without them,
    /// off the game thread, or on a build without stat reads.
    ///
    /// # Examples
    ///
    /// ```
    /// fn level(world: &enshwick::world::World, player: &enshwick::world::Player) -> u32 {
    ///     world.progress(player.entity).map_or(0, |progress| progress.level)
    /// }
    /// ```
    #[must_use]
    pub fn progress(self, entity: Entity) -> Option<Progress> {
        self.0.stats?.progress(self, entity)
    }
}
