//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::world::WritableType;
use crate::{Load, Off};
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
