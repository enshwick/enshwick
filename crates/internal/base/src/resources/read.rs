//! A resource's bytes as the game has them or would build them now, read
//! through the mod's [`Host`].

use crate::abi::{ResourceKey, Status};
use crate::{Host, Off};

impl Host {
    /// A vanilla resource's bytes from the game's archive; see
    /// `content::game_resource`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::{Guid, types};
    ///
    /// if let Some(host) = enshwick::host() {
    ///     let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    ///     if let Ok(key) = types::ITEM_INFO.key(chest) {
    ///         let _ = host.game_resource(key);
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn game_resource(&self, key: ResourceKey) -> Option<Vec<u8>> {
        self.read_resource(self.api().resource_read, key).ok()
    }

    /// A resource's bytes as the game's first read would build them now: a
    /// served one with every mod's patches, others' included, any other
    /// from the archive. Call it after init, such as in a patch of another
    /// key, and once per need: each read runs every patch of the key again
    /// and frees what they built. When every patch fails, a changed vanilla
    /// key reads as the archive has it. A patch that reads a key this way
    /// runs again whenever that key is rebuilt. To give an item another's data,
    /// use `Load::rekey_item`, which keeps its
    /// ids.
    ///
    /// # Errors
    ///
    /// [`Off`] during init, for a key neither served nor in the archive,
    /// and in a patch of a key whose patches already run on this thread
    /// ([`Status::BUILDING`]), as reading it
    /// would build it again without end.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::{Guid, types};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let resin = types::ITEM_INFO.key(Guid::parse("2f04a05f-697d-489b-a32f-76bb3439f5bc"))?;
    ///     let wood = types::ITEM_INFO.key(Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"))?;
    ///     let host = load.host();
    ///     load.serve(resin, resin, move |resin| {
    ///         // Wood as other mods serve it; inside a build of wood, resin
    ///         // stays as it is rather than failing.
    ///         if let Ok(wood) = host.served_resource(wood) {
    ///             enshwick::info!("wood is served as {} bytes", wood.len());
    ///         }
    ///         Some(resin.to_vec())
    ///     })
    /// }
    /// ```
    pub fn served_resource(&self, key: ResourceKey) -> Result<Vec<u8>, Off> {
        self.read_resource(self.api().resource_served, key)
            .map_err(|status| crate::load::off_status(&format!("read resource {key:?}"), status))
    }

    /// Reads `key` through `read`, one of the loader's copy calls, into a
    /// buffer grown to the length it asks for.
    ///
    /// # Errors
    ///
    /// The loader's status when the read fails.
    #[doc(hidden)]
    pub fn read_resource(
        &self,
        read: extern "system" fn(
            crate::abi::ModHandle,
            *const ResourceKey,
            *mut u8,
            u32,
            *mut u32,
        ) -> Status,
        key: ResourceKey,
    ) -> Result<Vec<u8>, Status> {
        let mut buffer = vec![0u8; 64 << 10];
        loop {
            let mut len = 0u32;
            let capacity = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
            let status = read(
                self.handle(),
                &raw const key,
                buffer.as_mut_ptr(),
                capacity,
                &raw mut len,
            );
            let len = usize::try_from(len).unwrap_or(usize::MAX);
            if status.is_ok() {
                buffer.truncate(len);
                return Ok(buffer);
            }
            if status != Status::BAD_ARGS || len <= buffer.len() {
                return Err(status);
            }
            buffer.resize(len, 0);
        }
    }
}
