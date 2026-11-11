//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::resources::Value;
use crate::typed::ecs::{RandomSpawner, SpawnConditionFlags};
use crate::typed::{Flags, Named as _};
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_spawns)]
impl Load<'_> {
    /// Adds `entry` to the random spawners it picks, after their own
    /// entries, on the server; mods that change one spawner change it in
    /// load order. The spawner picks it by its weight against the others'.
    ///
    /// # Errors
    ///
    /// [`Off`] when a picked template is no vanilla random spawner, no
    /// vanilla spawner lists the template a [`SpawnerPick::Listing`] names,
    /// or as [`Load::change_template`].
    ///
    /// # Examples
    ///
    /// See [`spawns`](self).
    #[script]
    pub fn add_spawn_entry(&mut self, entry: SpawnEntry) -> Result<(), Off>;

    /// Changes the counts and timeouts of the random spawners `spawner`
    /// picks, on the server; mods that change one spawner change it in load
    /// order.
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
    ///     load.change_spawner(
    ///         SpawnerPick::Spawner(nest),
    ///         SpawnerChange { max_entities_alive: Some(2), ..SpawnerChange::default() },
    ///     )
    /// }
    /// ```
    #[script]
    pub fn change_spawner(
        &mut self,
        spawner: SpawnerPick,
        change: SpawnerChange,
    ) -> Result<(), Off>;
}
