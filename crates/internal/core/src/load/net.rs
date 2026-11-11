//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::net::{Channel, Peer};
use crate::resources::Value as Data;
use crate::{Load, Off};

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Opens the mod's channel `name`, in `version`, for messages of type
    /// `M` between its server and client parts, and runs `handler` on each
    /// message that comes in, with who sent it. Both parts open the same
    /// name and version; a peer on another version never hears it. Join
    /// mods only, since both ends must run the mod.
    ///
    /// # Errors
    ///
    /// [`Off`] for a mod that is no join mod, a name of more than 64 bytes or
    /// opened twice, or after `init`.
    ///
    /// # Examples
    ///
    /// See the [module](crate::net).
    pub fn channel<M, F>(
        &mut self,
        name: &str,
        version: u16,
        handler: F,
    ) -> Result<Channel<M>, Off>
    where
        M: serde::Serialize + serde::de::DeserializeOwned + 'static,
        F: Fn(Peer, M) + Send + Sync + 'static;
}

#[enshwick_macros::forward(base)]
#[crate::script(net)]
impl Load<'_> {
    /// Opens the mod's channel `name`, in `version`, for game data messages,
    /// as [`Load::channel`] opens one of a Rust type.
    ///
    /// # Errors
    ///
    /// As [`Load::channel`].
    #[script(name = "channel")]
    pub(crate) fn channel_script(
        &mut self,
        name: &str,
        version: u16,
        handler: impl Fn(Peer, Data) + Send + Sync + 'static,
    ) -> Result<Channel<Data>, Off>;
}
