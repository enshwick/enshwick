//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::WorldError;
use crate::abi;
use crate::script::{
    Error as ScriptError, Field as ScriptField, FromScript, IntoScript, Record, Ty, Typed, Value,
};
use crate::{Host, Load};
use enshwick_world::__load::*;

mod edits;
use crate::Off;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The [`Voxels`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](crate::voxels).
    #[must_use]
    pub fn voxels(&mut self) -> Voxels;
}
