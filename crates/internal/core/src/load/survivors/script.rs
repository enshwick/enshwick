//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::Guid;
use crate::script::{Bound, Class, Function, Object};
use crate::survivors::{Answer, NpcId};
use crate::{Entity, ItemId, Load, Off};
use core::ffi::c_void;
use enshwick_inventory::__internal::ScriptLookup;
use enshwick_inventory::__load::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[enshwick_macros::forward(base)]
#[crate::script(survivor_hooks)]
impl Load<'_> {
    /// Makes `handler` the answer to the game's survivor lookups, as
    /// [`Load::on_survivor_lookup`] does: it gets the lookup as an object.
    ///
    /// # Errors
    ///
    /// As [`Load::on_survivor_lookup`].
    #[script(name = "on_survivor_lookup")]
    pub(crate) fn on_survivor_lookup_script(
        &mut self,
        handler: impl Fn(ScriptLookup) -> Answer + Send + Sync + 'static,
    ) -> Result<(), Off>;

    /// Makes `merge` decide what the survivor menu shows, as
    /// [`Load::on_survivor_status`] does: it gets the status bytes as a
    /// list.
    ///
    /// # Errors
    ///
    /// As [`Load::on_survivor_status`].
    #[script(name = "on_survivor_status")]
    pub(crate) fn on_survivor_status_script(
        &mut self,
        merge: impl Fn(NpcId, Vec<u8>) -> Option<u8> + Send + Sync + 'static,
    ) -> Result<(), Off>;
}
