//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::Guid;
use crate::content::enemies::{ArsenalChange, ArsenalId, NewArsenal, NewEnemy};
use crate::content::templates::TemplateId;
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
    /// Unstable: needs the `typed` feature and may change in any release.
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
    ///     let mut enemy = NewEnemy::new(id!("elder-wolf"), WOLF);
    ///     enemy.level = Some(20);
    ///     enemy.damage = Some(1.5);
    ///     let elder = load.add_enemy(enemy)?;
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
    /// Unstable: needs the `typed` feature and may change in any release.
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
    ///     let mut arsenal_change = ArsenalChange::default();
    ///     arsenal_change.attacks = Some(vec![bite]);
    ///     let biter = load.add_arsenal(NewArsenal::new(id!("biter"), wolf_arsenal, arsenal_change))?;
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
    /// Unstable: needs the `typed` feature and may change in any release.
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
    ///     let mut arsenal_change = ArsenalChange::default();
    ///     let mut attack_change = AttackChange::new(bite);
    ///     attack_change.selection_chance = Some(0.1);
    ///     arsenal_change.tune = vec![attack_change];
    ///     load.change_arsenal(wolf_arsenal, arsenal_change)
    /// }
    /// ```
    #[script]
    pub fn change_arsenal(&mut self, arsenal: Guid, change: ArsenalChange) -> Result<(), Off>;
}
