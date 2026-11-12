//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::item_data::ItemData;
use crate::resources::Value as Data;
use crate::{Load, Off};
use enshwick_inventory::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Opens the item data kind `id`: `name` for the mod's own, or
    /// `<mod>:<name>` for any mod's, the name 1 to 64 of `[a-z0-9_-]`. Every mod that opens one id shares its values, so the
    /// value type must match. Call it during `init`, on both sides. These
    /// kinds are the one exception to the export rule of
    /// [how ids are namespaced](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md).
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed id or after init.
    ///
    /// # Examples
    ///
    /// See the [module](crate::item_data).
    #[cfg(feature = "unstable")]
    pub fn item_data<T>(&mut self, id: &str) -> Result<ItemData<T>, Off>
    where
        T: serde::Serialize + serde::de::DeserializeOwned;
}

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
#[crate::script(item_data)]
impl Load<'_> {
    /// Opens the item data kind `id` of game data values, as
    /// [`Load::item_data`] opens one of a Rust type.
    ///
    /// # Errors
    ///
    /// As [`Load::item_data`].
    #[script(name = "item_data")]
    pub(crate) fn item_data_script(&mut self, id: &str) -> Result<ItemData<Data>, Off>;
}
