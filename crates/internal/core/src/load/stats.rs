//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::buffs::BuffTypeId;
use crate::content::Id;
use crate::stats::{Modifier, Stat, StatChange, StatDef, Stats};
use crate::{Load, Off};
use enshwick_inventory::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(stats)]
impl Load<'_> {
    /// Defines the mod's stat kind `id`, named with
    /// [`id!`](crate::content::id). Its text is `<name>.name` in the mod's
    /// texts ([`name_key`]). Call it during `init`, on both sides.
    ///
    /// # Errors
    ///
    /// [`Off`] for an id of another mod, a malformed definition, a kind
    /// defined twice, or after init.
    ///
    /// # Examples
    ///
    /// See the [module](crate::stats).
    #[script]
    pub fn define_stat(&mut self, id: Id, def: &StatDef) -> Result<Stat, Off>;

    /// Opens stat kind `id`: a built-in one from [`builtin`], or another
    /// mod's through [`deps!`](crate::deps). A kind no mod defines keeps 0
    /// and reads nothing. Call it during `init`, on both sides.
    ///
    /// # Errors
    ///
    /// [`Off`] for a kind of a loaded mod that does not export it, or after
    /// init.
    ///
    /// # Examples
    ///
    /// See the [module](crate::stats).
    #[script]
    pub fn open_stat(&mut self, id: Id) -> Result<Stat, Off>;

    /// The stat values and modifiers. Call it during `init`.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// See the [module](crate::stats).
    pub fn stats(&mut self) -> Result<Stats, Off>;

    /// Gives every entity that has buff `buff` the `modifiers`, while it has
    /// it: a mod's own buff type, or a vanilla one. Buffs count on watched
    /// entities (see [`Stats::watch`]). Call it during `init`.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::buffs::BuffTypeId;
    /// use enshwick::stats::{Modifier, Op, builtin};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let fire = load.open_stat(builtin::RESISTANCE_FIRE)?;
    ///     load.add_buff_modifiers(BuffTypeId(0x89cb_0b34), &[Modifier::new(fire, Op::Add, 0.2)])
    /// }
    /// ```
    #[script]
    pub fn add_buff_modifiers(
        &mut self,
        buff: BuffTypeId,
        modifiers: &[Modifier],
    ) -> Result<(), Off>;

    /// Runs `handler` for every stat value that changed on a watched entity,
    /// on the server's game thread after the step that changed it.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// ```
    /// enshwick::deps!();
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let luck = load.open_stat(deps::lockers::stat("luck"))?;
    ///     load.on_stats_changed(move |change| {
    ///         if change.stat == luck {
    ///             enshwick::info!("{:?} has luck {}", change.entity, change.new);
    ///         }
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_stats_changed<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&StatChange) + Send + Sync + 'static;
}
