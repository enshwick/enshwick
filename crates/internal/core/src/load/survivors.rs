//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::ItemId;
use crate::survivors::{Answer, BedCheck, Lookup, NpcId, Summon, SummonVerdict};
use crate::{Load, Off};
use enshwick_inventory::__load::*;

mod script;

#[enshwick_macros::forward(base)]
#[crate::script(survivors)]
impl Load<'_> {
    /// Makes `handler` the answer to the game's survivor lookups; one mod at
    /// a time. It runs on the server's worker threads, in the dedicated
    /// server or a hosting client, and for [`Purpose::Cursor`] on a client
    /// alone too: keep it quick and never block.
    ///
    /// Several engine systems look survivors up for every survivor on every
    /// tick, so `handler` runs many times a frame: answer from state read
    /// without contention, such as a read lock or an atomic snapshot, and
    /// never allocate on the common path.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, when another mod answers already, or on a build
    /// that lacks a piece.
    ///
    /// # Examples
    ///
    /// See the [module](crate::survivors).
    pub fn on_survivor_lookup<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&Lookup) -> Answer + Send + Sync + 'static;

    /// Runs `filter` after a survivor's own check kept its bed; returning
    /// `false` makes the game clear the bed. Server's worker threads, for
    /// every survivor with a bed on every tick: keep it quick and never
    /// block.
    ///
    /// # Errors
    ///
    /// As for [`Load::on_survivor_lookup`], without the one-mod rule.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Survivors keep beds within 100 m of home.
    ///     load.on_survivor_bed_check(|check| (0..3).all(|a| (check.home[a] - check.bed_at[a]).abs() < 200))
    /// }
    /// ```
    #[script]
    pub fn on_survivor_bed_check<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&BedCheck) -> bool + Send + Sync + 'static;

    /// Runs `filter` before a summoning staff spawns or moves a survivor:
    /// [`SummonVerdict::Take`] makes the use take the item, in the same
    /// transaction and only when the spawn or move works;
    /// [`SummonVerdict::Refuse`] refuses the use, and nothing spawns, moves
    /// or is taken. On the thread of the use, inside the game's step.
    ///
    /// # Errors
    ///
    /// As for [`Load::on_survivor_bed_check`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::survivors::SummonVerdict;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     const MY_STAFF: enshwick::ItemId = enshwick::ItemId::new(0x5e01);
    ///     load.on_summon(|summon| {
    ///         if summon.item == MY_STAFF && summon.ammo.is_none() {
    ///             SummonVerdict::Take
    ///         } else {
    ///             SummonVerdict::Allow
    ///         }
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_summon<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Summon) -> SummonVerdict + Send + Sync + 'static;

    /// Runs `handler` after a summoning staff's use spawned or moved a
    /// survivor, with [`Summon::consumed`] saying whether it took the item:
    /// the place to spend a charge of a staff with a meter. On the thread
    /// of the use, inside the game's step: hand game work to the
    /// [`Scheduler`](crate::schedule::Scheduler).
    ///
    /// # Errors
    ///
    /// As for [`Load::on_survivor_bed_check`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_summoned(|summon| {
    ///         if let (Some(pide), Some(meter), false) = (summon.pide, summon.meter, summon.consumed) {
    ///             enshwick::debug!("item {pide} summoned with {meter:?} left");
    ///         }
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_summoned<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&Summon) + Send + Sync + 'static;

    /// Files item `item` as a summoning staff of `survivor`, so the game
    /// treats it as that survivor's staff on both sides: the cursor, the
    /// unlock check and the placement.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build that lacks the registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::survivors::NpcId;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_summon_item(enshwick::ItemId::new(0x5e01), NpcId(1_201_801_546))
    /// }
    /// ```
    #[script]
    pub fn add_summon_item(&mut self, item: ItemId, survivor: NpcId) -> Result<(), Off>;

    /// Makes `merge` decide what the client's survivor menu shows for a
    /// survivor with several entities: it gets the status byte of each and
    /// returns the byte to show, or `None` for the game's. Client only; one
    /// mod at a time. It runs every frame while the menu shows, once per such
    /// survivor: it must not allocate or lock.
    ///
    /// # Errors
    ///
    /// [`Off`] on the server, and as for [`Load::on_survivor_lookup`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Show only the bits every entity has.
    ///     load.on_survivor_status(|_, statuses| {
    ///         (statuses.len() >= 2).then(|| statuses.iter().fold(u8::MAX, |all, s| all & s))
    ///     })
    /// }
    /// ```
    pub fn on_survivor_status<F>(&mut self, merge: F) -> Result<(), Off>
    where
        F: Fn(NpcId, &[u8]) -> Option<u8> + Send + Sync + 'static;
}
