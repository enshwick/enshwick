//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Entity;
use crate::abi::{RenameCall, Str, read_headed};
use crate::world::Player;
use crate::world::Rename;
use crate::{Load, Off};
use core::ffi::c_void;
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(world_rename)]
impl Load<'_> {
    /// Runs `filter` before the server applies a player's rename of any
    /// entity, on its game thread; returning `false` refuses the rename. The
    /// game itself checks no permission for renames. A client runs it while
    /// it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the filter, as after init.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Only administrators rename anything.
    ///     load.on_rename(|rename| rename.player.as_ref().is_some_and(|p| p.is_admin()))
    /// }
    /// ```
    #[script]
    pub fn on_rename<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Rename) -> bool + Send + Sync + 'static;
}
