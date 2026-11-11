//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::state::DataStore;
use crate::{Load, Off};

#[enshwick_macros::forward(base)]
#[crate::script(state)]
impl Load<'_> {
    /// The mod's store `name` for the world running now, of game data a
    /// script keeps, as `Load::world_store` opens a Rust mod's.
    ///
    /// # Errors
    ///
    /// As [`crate::Load::world_store`].
    #[script(name = "world_store")]
    pub(crate) fn world_store_script(&mut self, name: &str) -> Result<DataStore, crate::Off>;
}
