//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::spawns::{SpawnEntry, SpawnerChange, SpawnerPick};
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_spawns)]
impl Load<'_> {
    /// Adds `entry` to the random spawners it picks, after their own
    /// entries, on the server; mods that change one spawner change it in
    /// load order. The spawner picks it by its weight against the others'.
    ///
    /// Unstable: needs the `typed` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] when a picked template is no vanilla random spawner, no
    /// vanilla spawner lists the template a [`SpawnerPick::Listing`] names,
    /// or as [`Load::change_template`].
    ///
    /// # Examples
    ///
    /// See [`spawns`](crate::content::spawns).
    #[script]
    pub fn add_spawn_entry(&mut self, entry: SpawnEntry) -> Result<(), Off>;

    /// Changes the counts and timeouts of the random spawners `spawner`
    /// picks, on the server; mods that change one spawner change it in load
    /// order.
    ///
    /// Unstable: needs the `typed` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// As [`Load::add_spawn_entry`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::content::spawns::{SpawnerChange, SpawnerPick};
    ///
    /// fn init(load: &mut enshwick::Load<'_>, nest: Guid) -> Result<(), enshwick::Off> {
    ///     let mut spawner_change = SpawnerChange::default();
    ///     spawner_change.max_entities_alive = Some(2);
    ///     load.change_spawner(SpawnerPick::Spawner(nest), spawner_change)
    /// }
    /// ```
    #[script]
    pub fn change_spawner(
        &mut self,
        spawner: SpawnerPick,
        change: SpawnerChange,
    ) -> Result<(), Off>;
}
