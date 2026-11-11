//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::Status;
use crate::content::changes::{self, Change, Field};
use crate::content::recipes::{self, RecipeRule};
use crate::engine_ids as ids;
use crate::resources::{Registry, Value};
use crate::typed::{ItemFlag, Reflected as _};
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_removal)]
impl Load<'_> {
    /// Takes the vanilla item `item` out of play while the mod runs: no
    /// recipe makes it, no crafting or build menu lists it, and no loot
    /// table or starting inventory gives it. The item stays registered, so
    /// the stacks players hold keep it and a world opens as vanilla without
    /// the mod; a mod that wants held copies gone takes them out with the
    /// inventory API.
    ///
    /// Mods' changes of loot chain in load order: a later mod that gives the
    /// item a count gives it back.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game, after init, and when the build does not
    /// reflect the loot tables.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn remove_item(&mut self, item: Guid) -> Result<(), Off>;

    /// Takes the vanilla workshop of the station item `station` out of
    /// play while the mod runs: no recipe is made at it, and no recipe
    /// builds the station. Placed stations stay in the world, since they
    /// are entities of the station's template, which the mod leaves alone.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.remove_workshop(vanilla::item("Craftingstation_Workbench")?)
    /// }
    /// ```
    #[script]
    #[expect(
        clippy::unused_self,
        reason = "an init call beside the other removals, where mods and scripts find it"
    )]
    pub fn remove_workshop(&mut self, station: Guid) -> Result<(), Off>;

    /// Serves the vanilla loot table `table`, random or a container's fill
    /// table, giving nothing while the mod runs. Loot players hold is
    /// plain stacks and stays.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game and for a table the game lacks.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    ///
    /// fn init(load: &mut enshwick::Load<'_>, table: Guid) -> Result<(), enshwick::Off> {
    ///     load.remove_loot_table(table)
    /// }
    /// ```
    #[script]
    pub fn remove_loot_table(&mut self, table: Guid) -> Result<(), Off>;

    /// Serves the vanilla item `old` with the data of the item `new` while
    /// the mod runs, under `old`'s own id and GUID. `new` is a vanilla item
    /// or one this mod added with [`Load::add_item`] before the call. Saves,
    /// recipes and loot that name `old` get `new`'s look and use. The stack
    /// size is at least `old`'s, since the game cuts a held stack above its
    /// item's and saves the cut; saves keep `old`'s id, so a world opens as
    /// vanilla without the mod. `old` takes `new` as the game reads it, with
    /// every mod's changes of `new`, and follows each rebuild of `new`, such
    /// as a world's changed values, while the data still fits the bytes the
    /// game holds for `old`; inside that read, a re-key of `new`
    /// back to `old` is skipped, so two items re-keyed to each other swap.
    /// A ring of three or more re-keys does not rotate: each item takes the
    /// data of the item two steps on. Mods that serve `old` chain in load
    /// order, and the last re-key's data wins; a build that fails keeps
    /// `old` as it was.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game, for an item the game lacks, and when one of
    /// the two items has an entity of its own (`ItemFlag::HasPIDE`) and the
    /// other not.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn rekey_item(&mut self, old: Guid, new: Guid) -> Result<(), Off>;
}
