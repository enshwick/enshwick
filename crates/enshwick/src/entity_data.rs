//! Data a mod attaches to any entity, such as a placed chest's owner or a
//! survivor's nickname: typed values in kinds named `<mod>:<name>`, kept
//! per world and, for kinds that ask for it, sent to players.
//!
//! Unstable: needs the `unstable` feature and may change in any release.
//!
//! A load numbers the world's entities anew, so the server keeps each value
//! under its entity's place: the template and the position as the save
//! keeps it. After the first step of a loaded world every value finds its
//! entity again; a value whose place no entity, or more than one, holds
//! goes aside in the world's file and is never given to another entity.
//! Only an entity the save keeps in place carries a value: a player's
//! character does not, so [`EntityData::set`] refuses it with
//! [`SetError::NotKept`]. Once the entity is gone or holds another
//! template, the value reads as missing on the server's game thread, and the
//! server drops it when the world saves. Values live in
//! `mod-data/enshwick/worlds/<key>/`, never in the game's save, beside
//! [item data](crate::item_data), which is the same store for items.
//!
//! A kind opened with [`Replication::Clients`] goes to the clients that run
//! Enshwick, keyed by the server's entity ids, which are not the ids the
//! client's own world gives the same things: a client reads a value by an
//! id the server named, such as one in a mod message. A value is encoded
//! with `postcard`, at most [`MAX_VALUE`] bytes.
//!
//! # Examples
//!
//! ```
//! use enshwick::Entity;
//! use enshwick::entity_data::Replication;
//!
//! #[derive(serde::Serialize, serde::Deserialize)]
//! struct Owner {
//!     player: u64,
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let owners = load.entity_data::<Owner>("owner", Replication::Server)?;
//!     let chest = Entity(4_200);
//!     if owners.get(chest).is_none() {
//!         let _set = owners.set(chest, &Owner { player: 7 });
//!     }
//!     Ok(())
//! }
//! ```

use core::marker::PhantomData;

use crate::abi::{EntityDataFlags, Status, Str};
pub use crate::item_data::{MAX_VALUE, SetError};
use crate::{Entity, ItemId};
use crate::{Host, Load, Off};

/// Who gets a kind's values.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Replication {
    /// The server alone.
    #[default]
    Server,
    /// The server, and the clients that run Enshwick, which get every value
    /// on joining and every change after.
    Clients,
}

/// A kind of entity data with values of type `T`; get it from
/// [`Load::entity_data`]. Copy it anywhere.
///
/// # Examples
///
/// See the [module](self).
pub struct EntityData<T> {
    host: Host,
    kind: u32,
    value: PhantomData<fn() -> T>,
}

impl<T> Clone for EntityData<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for EntityData<T> {}

impl<T> core::fmt::Debug for EntityData<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EntityData")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl<T> EntityData<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    /// The entity's value; `None` when it has none, when the value does not
    /// decode as `T`, or, read on the server's game thread, when the entity
    /// is gone or another thing now. Any thread: on the server its own
    /// values, on a client what the server sent.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn get(&self, entity: Entity) -> Option<T> {
        let mut buffer = [0u8; MAX_VALUE];
        let mut len = 0u32;
        let status = (self.host.api().item_data_get)(
            self.host.handle(),
            self.kind,
            entity,
            ItemId::new(0),
            buffer.as_mut_ptr(),
            u32::try_from(buffer.len()).unwrap_or(u32::MAX),
            &raw mut len,
        );
        if !status.is_ok() {
            return None;
        }
        postcard::from_bytes(buffer.get(..usize::try_from(len).ok()?)?).ok()
    }

    /// Sets the entity's value, kept with the entity's template; the clients
    /// get it when the kind is sent. On the server's game thread, the
    /// dedicated server's or a hosting client's, once the world has loaded.
    ///
    /// # Errors
    ///
    /// [`SetError`] when the value does not encode or is too long, off the
    /// server or its game thread, before the world has loaded, for an
    /// entity that does not exist, or for one no load finds again
    /// ([`SetError::NotKept`]).
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn set(&self, entity: Entity, value: &T) -> Result<(), SetError> {
        let bytes = postcard::to_allocvec(value).map_err(|e| SetError::Encode(e.to_string()))?;
        if bytes.is_empty() || bytes.len() > MAX_VALUE {
            return Err(SetError::TooLarge(bytes.len()));
        }
        self.write(entity, &bytes)
    }

    /// Removes the entity's value.
    ///
    /// # Errors
    ///
    /// As for [`EntityData::set`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    ///
    /// fn forget(owners: &enshwick::entity_data::EntityData<u64>, chest: Entity) {
    ///     let _removed = owners.remove(chest);
    /// }
    /// ```
    pub fn remove(&self, entity: Entity) -> Result<(), SetError> {
        self.write(entity, &[])
    }

    fn write(&self, entity: Entity, bytes: &[u8]) -> Result<(), SetError> {
        let status = (self.host.api().item_data_set)(
            self.host.handle(),
            self.kind,
            entity,
            ItemId::new(0),
            bytes.as_ptr(),
            u32::try_from(bytes.len()).unwrap_or(u32::MAX),
        );
        match status {
            Status::OK => Ok(()),
            Status::REFUSED | Status::WRONG_THREAD => Err(SetError::NotServer),
            Status::MISSING => Err(SetError::NoWorld),
            Status::BAD_ARGS => Err(SetError::NoEntity),
            Status::NOT_KEPT => Err(SetError::NotKept),
            other => Err(SetError::Refused(other.0)),
        }
    }
}

impl Load<'_> {
    /// Opens the entity data kind `id`: `name` for the mod's own,
    /// `<mod>:<name>`, or `mod:name` for another mod's, each side 1 to 64 of
    /// `[a-z0-9_-]`. Every mod that opens one id shares its values, so the
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
    /// See the [module](self).
    pub fn entity_data<T>(
        &mut self,
        id: &str,
        replication: Replication,
    ) -> Result<EntityData<T>, Off>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let host = self.host();
        let flags = match replication {
            Replication::Server => EntityDataFlags::NONE,
            Replication::Clients => EntityDataFlags::SENT,
        };
        let mut kind = 0;
        let status =
            (host.api().entity_data_open)(host.handle(), Str::new(id), flags, &raw mut kind);
        crate::load::ok(
            status,
            EntityData {
                host,
                kind,
                value: PhantomData,
            },
            || format!("entity data {id:?}"),
        )
    }
}
