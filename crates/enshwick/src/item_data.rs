//! Data a mod attaches to single items, such as a staff's owner or a
//! ring's rolled bonus: typed values in kinds named `<mod>:<name>`, kept
//! per world and sent to players.
//!
//! Unstable: needs the `unstable` feature and may change in any release.
//! After a restart, a value on an item a load cannot find stays aside and no
//! longer reads.
//!
//! An item with an entity of its own, its PIDE (see
//! [`Slot::pide`](crate::inventory::Slot::pide)), is an [`ItemInstance`]:
//! the entity and the item id it holds. Within a session a value stays with
//! its item across moves and trades; it reads as missing once the entity
//! holds another item, and the server drops values of items that are gone
//! when the world saves. A load numbers item entities anew, so the server
//! keeps a value under the item's place when the world saves: the placed
//! container and the slot it lies in. After the next load the value finds
//! the item in that slot again. An item in a player's inventory is kept
//! under the character and the slot instead: when the player leaves, or the
//! server stops while they play, the value waits for that character's next
//! join and then finds the item in that slot again, if it holds the same
//! item; one whose slot holds another item when the player leaves again
//! goes aside. On a game build whose table does not say where the server
//! keeps a player's character, the value goes aside at the leave, kept but
//! attached to nothing.
//!
//! The server keeps the values, in `mod-data/enshwick/worlds/<key>/`, never
//! in the game's save, and sends every change to the clients that run
//! Enshwick; a client reads what it got. Any mod can open any kind by its
//! full id: kinds are how mods share item data without depending on each
//! other. A value is encoded with `postcard`, at most [`MAX_VALUE`] bytes.
//!
//! A count the player should see on the item belongs in its
//! [`FuelSetup`](crate::content::items::FuelSetup), which the game itself saves,
//! sends and draws.
//!
//! # Examples
//!
//! ```rust,standalone_crate
//! use enshwick::inventory::ItemInstance;
//!
//! #[derive(serde::Serialize, serde::Deserialize)]
//! struct Engraving {
//!     by: String,
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let engravings = load.item_data::<Engraving>("engraving")?;
//!     let ring = ItemInstance { pide: enshwick::Entity(900), item: enshwick::ItemId::new(0x5e01) };
//!     let _set = engravings.set(ring, &Engraving { by: "Balthazar".into() });
//!     let _by = engravings.get(ring).map(|e| e.by);
//!     Ok(())
//! }
//! ```

use core::marker::PhantomData;

use crate::abi::{Status, Str};
use crate::inventory::ItemInstance;
use crate::{Host, Load, Off};

/// The largest encoded value.
pub const MAX_VALUE: usize = 512;

/// Why a value was not set.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::item_data::SetError;
///
/// assert_eq!(SetError::NoEntity.to_string(), "no such entity");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SetError {
    /// The value did not encode.
    #[error("the value does not encode: {0}")]
    Encode(String),
    #[cfg_attr(
        feature = "unstable",
        doc = "The encoded value is longer than [`MAX_VALUE`]."
    )]
    #[cfg_attr(
        not(feature = "unstable"),
        doc = "The encoded value is longer than the largest value an item keeps."
    )]
    #[error("the value takes {0} bytes, more than {MAX_VALUE}")]
    TooLarge(usize),
    /// Called on a client that hosts no game, or for entity data off the
    /// server's game thread.
    #[error("only the server sets mod data, entity data on its game thread")]
    NotServer,
    /// The world has not loaded yet.
    #[error("the world has not loaded")]
    NoWorld,
    /// The entity does not exist.
    #[error("no such entity")]
    NoEntity,
    /// Nothing finds the entity again after the world's next load: a
    /// player's character, an entity with no place in the world, or any
    /// entity on a game build whose layouts the loader does not read.
    #[error("the entity is not found again after a load")]
    NotKept,
    /// The item has no entity, or the loader refused for another reason.
    #[error("mod data refused (status {0})")]
    Refused(u32),
}

/// A kind of item data with values of type `T`; get it from
/// [`Load::item_data`]. Copy it anywhere.
///
/// # Examples
///
/// See the [module](self).
pub struct ItemData<T> {
    host: Host,
    kind: u32,
    value: PhantomData<fn() -> T>,
}

impl<T> Clone for ItemData<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ItemData<T> {}

impl<T> core::fmt::Debug for ItemData<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ItemData")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl<T> ItemData<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    /// The item's value; `None` when it has none, when its entity holds
    /// another item now, or when the value does not decode as `T`. Any
    /// thread: on the server its own values, on a client what the server
    /// sent.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn get(&self, item: ItemInstance) -> Option<T> {
        let mut buffer = [0u8; MAX_VALUE];
        let mut len = 0u32;
        let status = (self.host.api().item_data_get)(
            self.host.handle(),
            self.kind,
            item.pide,
            item.item,
            buffer.as_mut_ptr(),
            u32::try_from(buffer.len()).unwrap_or(u32::MAX),
            &raw mut len,
        );
        if !status.is_ok() {
            return None;
        }
        postcard::from_bytes(buffer.get(..usize::try_from(len).ok()?)?).ok()
    }

    /// Sets the item's value; the clients that run Enshwick get it. On the
    /// server, or a client while it hosts, once the world has loaded.
    ///
    /// # Errors
    ///
    /// [`SetError`] when the value does not encode or is too long, off the
    /// server, before the world has loaded, or for an item without an
    /// entity.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn set(&self, item: ItemInstance, value: &T) -> Result<(), SetError> {
        let bytes = postcard::to_allocvec(value).map_err(|e| SetError::Encode(e.to_string()))?;
        if bytes.is_empty() || bytes.len() > MAX_VALUE {
            return Err(SetError::TooLarge(bytes.len()));
        }
        self.write(item, &bytes)
    }

    /// Removes the item's value.
    ///
    /// # Errors
    ///
    /// As for [`ItemData::set`].
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// fn wipe(kind: &enshwick::item_data::ItemData<u16>, item: enshwick::inventory::ItemInstance) {
    ///     let _removed = kind.remove(item);
    /// }
    /// ```
    pub fn remove(&self, item: ItemInstance) -> Result<(), SetError> {
        self.write(item, &[])
    }

    fn write(&self, item: ItemInstance, bytes: &[u8]) -> Result<(), SetError> {
        let status = (self.host.api().item_data_set)(
            self.host.handle(),
            self.kind,
            item.pide,
            item.item,
            bytes.as_ptr(),
            u32::try_from(bytes.len()).unwrap_or(u32::MAX),
        );
        match status {
            Status::OK => Ok(()),
            Status::REFUSED => Err(SetError::NotServer),
            Status::MISSING => Err(SetError::NoWorld),
            Status::NOT_KEPT => Err(SetError::NotKept),
            other => Err(SetError::Refused(other.0)),
        }
    }
}

impl Load<'_> {
    /// Opens the item data kind `id`: `name` for the mod's own,
    /// `<mod>:<name>`, or `mod:name` for another mod's, each side 1 to 64 of
    /// `[a-z0-9_-]`. Every mod that opens one id shares its values, so the
    /// value type must match. Call it during `init`, on both sides.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed id or after init.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[cfg(feature = "unstable")]
    pub fn item_data<T>(&mut self, id: &str) -> Result<ItemData<T>, Off>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        self.open_item_data(id)
    }

    /// [`Load::item_data`] for the stable parts built on item data, such as
    /// the stats of single items.
    pub(crate) fn open_item_data<T>(&mut self, id: &str) -> Result<ItemData<T>, Off>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let host = self.host();
        let mut kind = 0;
        let status = (host.api().item_data_open)(host.handle(), Str::new(id), &raw mut kind);
        crate::load::ok(
            status,
            ItemData {
                host,
                kind,
                value: PhantomData,
            },
            || format!("item data {id:?}"),
        )
    }
}
