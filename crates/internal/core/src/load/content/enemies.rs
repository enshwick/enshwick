//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::Guid;
use crate::content::enemies::*;
use crate::content::templates::TemplateId;
use crate::resources::Value;
use crate::typed::ecs::{Boss, DamageMod, EnemyArsenalReference, EnemyScaling, Faction, Level};
use crate::typed::enemy::EnemyArsenalRegistryResource;
use crate::typed::{FactionType, Named as _, Reflected as _};
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_enemies)]
impl Load<'_> {
    /// Adds an enemy: a copy of the vanilla enemy template
    /// [`NewEnemy::like`] as [`Load::add_template`] makes it, with its level,
    /// health, damage, faction, arsenal and boss mark set where
    /// [`NewEnemy`] gives them, on the server and the client alike. Spawn it
    /// by its template, as spawners and `effects::spawn` do.
    ///
    /// # Errors
    ///
    /// As [`Load::add_template`], and [`Off`] for level 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::enemies::NewEnemy;
    ///
    /// const WOLF: Guid = Guid::parse("0d75b9a9-7e48-4f3a-bf2a-19bf6021f531");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let elder = load.add_enemy(NewEnemy { level: Some(20), damage: Some(1.5), ..NewEnemy::new(id!("elder-wolf"), WOLF) })?;
    ///     assert_eq!(elder.guid(), id!("elder-wolf").guid());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_enemy(&mut self, mut new: NewEnemy) -> Result<TemplateId, Off>;

    /// Adds an arsenal, listed beside the vanilla ones: a copy of the
    /// vanilla arsenal [`NewArsenal::like`] under the mod's content id, with
    /// its change applied. An enemy uses it through [`NewEnemy::arsenal`].
    ///
    /// # Errors
    ///
    /// [`Off`] when `like` is no vanilla arsenal, the change names an attack
    /// or behaviour no vanilla arsenal holds, or the loader refuses the id
    /// (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::enemies::{ArsenalChange, NewArsenal};
    ///
    /// fn init(load: &mut enshwick::Load<'_>, wolf_arsenal: Guid, bite: Guid) -> Result<(), enshwick::Off> {
    ///     let biter = load.add_arsenal(NewArsenal {
    ///         id: id!("biter"),
    ///         like: wolf_arsenal,
    ///         change: ArsenalChange { attacks: Some(vec![bite]), ..ArsenalChange::default() },
    ///     })?;
    ///     enshwick::info!("biters use {}", biter.guid());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_arsenal(&mut self, new: NewArsenal) -> Result<ArsenalId, Off>;

    /// Changes the vanilla arsenal `arsenal` in memory, on the server and
    /// the client alike; mods that change one arsenal change it in load
    /// order, each on the one before's result.
    ///
    /// # Errors
    ///
    /// [`Off`] when `arsenal` is no vanilla arsenal or the change names an
    /// attack or behaviour no vanilla arsenal holds.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::content::enemies::{ArsenalChange, AttackChange};
    ///
    /// fn init(load: &mut enshwick::Load<'_>, wolf_arsenal: Guid, bite: Guid) -> Result<(), enshwick::Off> {
    ///     load.change_arsenal(wolf_arsenal, ArsenalChange {
    ///         tune: vec![AttackChange { selection_chance: Some(0.1), ..AttackChange::new(bite) }],
    ///         ..ArsenalChange::default()
    ///     })
    /// }
    /// ```
    #[script]
    pub fn change_arsenal(&mut self, arsenal: Guid, change: ArsenalChange) -> Result<(), Off>;
}
