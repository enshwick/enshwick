//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::Str;
use crate::game_events::{Field, Scalar};
use crate::resources::{Bytes, Value as Data};
use crate::script::{Class, Function, Object};
use crate::world::WritableType;
use crate::{Entity, Load, Off, WorldError};
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(writable_type)]
impl Load<'_> {
    /// The component type `name` for writes, as [`WritableType::resolve`]
    /// resolves it.
    ///
    /// # Errors
    ///
    /// As [`WritableType::resolve`].
    #[script(name = "writable_type")]
    pub(crate) fn writable_type_script(&mut self, name: &str) -> Result<WritableType, Off>;
}
