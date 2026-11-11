//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::{Load, Off};

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Serves `value` as the new resource `key`, of the type its key names,
    /// built from the value alone: every field the value leaves out is zero.
    /// The game must hold a resource of that type, which the loader opens as
    /// the base whose bytes the value replaces. Declare the key's GUID with
    /// [`Load::declare`](crate::Load::declare).
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game, for a type the build lacks or has no
    /// resource of, a value that does not fit the type, and when the loader
    /// refuses the serve, such as after init.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::{Id, id};
    /// use enshwick::resources::Value;
    ///
    /// const TABLE: Id = id!("balance");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let guid = load.declare(TABLE)?;
    ///     let value = Value::Struct(vec![("damage".into(), Value::Float(2.0))]);
    ///     load.serve_new(guid.key_by_hash(enshwick::resources::fnv1a32(b"demo::Table")), &value)
    /// }
    /// ```
    pub fn serve_new(
        &mut self,
        key: crate::abi::ResourceKey,
        value: &crate::resources::Value,
    ) -> Result<(), Off>;
}
