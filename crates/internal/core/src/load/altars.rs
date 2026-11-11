//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Entity;
use crate::abi::{AltarInfo, Bounds};
use crate::content::Guid;
use crate::{Host, Load, Off};
use enshwick_world::__load::*;

mod kinds;
mod limits;

#[enshwick_macros::forward(base)]
#[crate::script(altars)]
impl Load<'_> {
    /// Asks the loader to follow the Flame Altars, for [`Altars::list`] and
    /// [`AltarsChanged`](crate::events::AltarsChanged).
    ///
    /// # Errors
    ///
    /// [`Off`] after init, or on a game
    /// build that lacks the altar helpers.
    ///
    /// # Examples
    ///
    /// See the [module](crate::altars).
    pub fn altars(&mut self) -> Result<Altars, Off>;

    /// Runs `filter` for every altar of the server's world when the loader
    /// sizes the altar zones: after altars change, and after
    /// [`ZoneSizes::changed`]. Filters of several mods run in load order, each
    /// on the size the last one left; the mod follows the altars too. On the
    /// server's game thread.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, and as for [`Load::altars`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Every altar two cells wider on each side.
    ///     let sizes = load.on_altar_zone(|zone| zone.grow += 2)?;
    ///     let _ = sizes.altars();
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn on_altar_zone<F>(&mut self, filter: F) -> Result<ZoneSizes, Off>
    where
        F: Fn(&mut AltarZone) + Send + Sync + 'static;

    /// Runs `filter` when the game refuses a placement because the object's
    /// box does not lie in a single building zone; returning `true` accepts
    /// it after all. It runs on both sides, on the thread that checks the
    /// placement, every time a player aims a piece: keep it cheap and never
    /// block.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Accept an object whose box lies in two zones' bounding box.
    ///     load.on_building_zone_check(|check| {
    ///         check.zones.len() >= 2
    ///             && (0..3).all(|a| {
    ///                 let low = check.zones.iter().map(|z| z.min[a]).min().unwrap_or(0);
    ///                 let high = check.zones.iter().map(|z| z.max[a]).max().unwrap_or(0);
    ///                 low <= check.object.min[a] && check.object.max[a] < high
    ///             })
    ///     })
    /// }
    /// ```
    pub fn on_building_zone_check<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&BuildingZoneCheck<'_>) -> bool + Send + Sync + 'static;

    /// Runs `filter` when the game refuses a placement for its zones, as
    /// [`Load::on_building_zone_check`] does; `true` accepts it after all,
    /// and nil or an error keeps the game's refusal.
    ///
    /// # Errors
    ///
    /// As [`Load::on_building_zone_check`].
    #[script(name = "on_building_zone_check")]
    pub(crate) fn on_building_zone_check_script<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&BuildingZoneCheck<'_>) -> Option<bool> + Send + Sync + 'static;
}

#[enshwick_macros::forward(base)]
#[crate::script(altar_limit)]
impl Load<'_> {
    /// Runs `filter` when the game asks how many altars a Flame level
    /// allows, on both sides. Filters of several mods run in load order, each
    /// on the count the last one left, the game's own first; the loader keeps
    /// the answer within the zones the game can group ([`base_limit`]).
    ///
    /// # Errors
    ///
    /// [`Off`] after init, or on a build that lacks the lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Two more altars than the game allows at every level.
    ///     load.on_altar_limit(|limit| limit.allowed = limit.allowed.saturating_add(2))
    /// }
    /// ```
    #[script]
    pub fn on_altar_limit<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut AltarLimit) + Send + Sync + 'static;
}
