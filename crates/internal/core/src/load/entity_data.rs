//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::{EntityDataFlags, Status, Str};
use crate::entity_data::{EntityData, Replication};
use crate::resources::Value as Data;
use crate::{Entity, ItemId};
use crate::{Host, Load, Off};
use core::marker::PhantomData;
use enshwick_inventory::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Opens the entity data kind `id`: `name` for the mod's own, or
    /// `<mod>:<name>` for any mod's, the name 1 to 64 of `[a-z0-9_-]`, see
    /// [how ids are namespaced](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md).
    /// Every mod that opens one id shares its values, so the
    /// value type must match; the kind is sent to clients once any mod
    /// opens it with [`Replication::Clients`]. Call it during `init`, on
    /// both sides.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed id, an id opened as an item kind, or after
    /// init.
    ///
    /// # Examples
    ///
    /// See the [module](crate::entity_data).
    pub fn entity_data<T>(
        &mut self,
        id: &str,
        replication: Replication,
    ) -> Result<EntityData<T>, Off>
    where
        T: serde::Serialize + serde::de::DeserializeOwned;
}

#[enshwick_macros::forward(base)]
#[crate::script(entity_data)]
impl Load<'_> {
    /// Opens the entity data kind `id` of game data values, as
    /// [`Load::entity_data`] opens one of a Rust type.
    ///
    /// # Errors
    ///
    /// As [`Load::entity_data`].
    #[script(name = "entity_data")]
    pub(crate) fn entity_data_script(
        &mut self,
        id: &str,
        replication: Replication,
    ) -> Result<EntityData<Data>, Off>;
}
