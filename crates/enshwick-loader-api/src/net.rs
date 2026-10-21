//! Messages between a mod's server part and its client part.
//!
//! A join mod opens a channel by name and version during `init`, on both
//! sides; the loader carries its messages in chat lines no player sees, and
//! only to clients that run Enshwick, so a vanilla client never gets one.
//! A message is a value of the channel's type, encoded with `postcard`, of
//! at most [`MAX_MESSAGE`] bytes; each side sends at most about twenty
//! messages a second to each peer and drops what is over.
//!
//! On the server the handler runs on the game thread, with the player's
//! account key; on a client on the thread that takes the server's chat. A
//! client that hosts a game is both: its server part talks to the clients,
//! itself among them, and its client part to its own server.
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::net::Peer;
//!
//! #[derive(serde::Serialize, serde::Deserialize)]
//! enum Sync {
//!     Hello { version: u32 },
//!     Marker { at: [f64; 3] },
//! }
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     let markers = load.channel::<Sync, _>("markers", 1, |from, message| match (from, message) {
//!         (Peer::Player(key), Sync::Hello { version }) => {
//!             enshwick_loader_api::info!("player {key:016x} runs version {version}");
//!         }
//!         (Peer::Server, Sync::Marker { at }) => enshwick_loader_api::debug!("marker at {at:?}"),
//!         _ => {}
//!     })?;
//!     // On a client: greet the server once it takes messages.
//!     if markers.is_ready(Peer::Server) {
//!         let _sent = markers.send(Peer::Server, &Sync::Hello { version: 1 });
//!     }
//!     Ok(())
//! }
//! ```

use core::ffi::c_void;
use core::marker::PhantomData;

use crate::abi::{NetMessage, Status, Str};
use crate::{Host, Load, Off};

/// The largest message a channel carries, encoded, in bytes.
pub const MAX_MESSAGE: usize = 4096;

/// Who a message comes from or goes to.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Peer {
    /// The server, as a client sees it.
    Server,
    /// A player's client, by the account key, as the server sees it; the
    /// key [`Player::key`](crate::world::Player::key) gives.
    Player(u64),
}

impl Peer {
    fn to_raw(self) -> u64 {
        match self {
            Self::Server => NetMessage::SERVER,
            Self::Player(key) => key,
        }
    }

    fn from_raw(raw: u64) -> Self {
        if raw == NetMessage::SERVER {
            Self::Server
        } else {
            Self::Player(raw)
        }
    }
}

/// Why a message did not leave.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::net::SendError;
///
/// assert_eq!(SendError::NotReady.to_string(), "the peer takes no messages now");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SendError {
    /// The message is over [`MAX_MESSAGE`] bytes encoded.
    #[error("the message is over {MAX_MESSAGE} bytes")]
    TooLarge,
    /// The peer takes no messages now: a client without Enshwick, a player
    /// who left, or a server that has not answered yet.
    #[error("the peer takes no messages now")]
    NotReady,
    /// The peer's rate is spent for now.
    #[error("over the rate of messages to the peer")]
    OverRate,
    /// The message does not encode.
    #[error("the message does not encode: {0}")]
    Encode(String),
    /// The loader refused for another reason.
    #[error("the loader refused the message")]
    Refused,
}

/// A channel of messages of type `M`, from [`Load::channel`]; `Copy`, for
/// handlers and tasks.
///
/// # Examples
///
/// See the [module](self).
#[derive(Debug)]
pub struct Channel<M> {
    host: Host,
    id: u32,
    _message: PhantomData<fn(M) -> M>,
}

impl<M> Clone for Channel<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M> Copy for Channel<M> {}

impl<M: serde::Serialize> Channel<M> {
    /// Sends `message` to `to`: a client sends to [`Peer::Server`], the
    /// server to [`Peer::Player`]. Any thread.
    ///
    /// # Errors
    ///
    /// A [`SendError`] saying why it did not leave.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn send(&self, to: Peer, message: &M) -> Result<(), SendError> {
        let bytes = postcard::to_allocvec(message).map_err(|e| SendError::Encode(e.to_string()))?;
        send_raw(self.host, self.id, to.to_raw(), &bytes)
    }

    /// Sends `message` from the server to every client that runs Enshwick.
    /// Any thread.
    ///
    /// # Errors
    ///
    /// As for [`Channel::send`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn announce(markers: &enshwick_loader_api::net::Channel<String>) {
    ///     let _sent = markers.broadcast(&"restart in five minutes".to_owned());
    /// }
    /// ```
    pub fn broadcast(&self, message: &M) -> Result<(), SendError> {
        let bytes = postcard::to_allocvec(message).map_err(|e| SendError::Encode(e.to_string()))?;
        send_raw(self.host, self.id, 0, &bytes)
    }
}

impl<M> Channel<M> {
    /// Whether `peer` takes messages now: a player's client that runs
    /// Enshwick, on the server; the server once it answered, on a client.
    /// Any thread.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn is_ready(&self, peer: Peer) -> bool {
        let api = self.host.api();
        (api.net_ready)(self.host.handle(), peer.to_raw()).is_ok()
    }
}

fn send_raw(host: Host, id: u32, peer: u64, bytes: &[u8]) -> Result<(), SendError> {
    if bytes.len() > MAX_MESSAGE {
        return Err(SendError::TooLarge);
    }
    let api = host.api();
    let size = u32::try_from(bytes.len()).map_err(|_| SendError::TooLarge)?;
    match (api.net_send)(host.handle(), id, peer, bytes.as_ptr(), size) {
        Status::OK => Ok(()),
        Status::MISSING => Err(SendError::NotReady),
        Status::REFUSED => Err(SendError::OverRate),
        Status::BAD_ARGS => Err(SendError::TooLarge),
        _ => Err(SendError::Refused),
    }
}

/// A channel's handler as the loader keeps it.
struct Handler<F> {
    handler: F,
}

unsafe extern "system" fn deliver<M, F>(message: *const NetMessage, user: *mut c_void)
where
    M: serde::de::DeserializeOwned,
    F: Fn(Peer, M),
{
    // SAFETY: the loader passes a valid message for the call.
    let Some(message) = (unsafe { message.as_ref() }) else {
        return;
    };
    let bytes = if message.size == 0 || message.data.is_null() {
        &[][..]
    } else {
        // SAFETY: the message holds `size` bytes, valid for the call.
        unsafe { core::slice::from_raw_parts(message.data, message.size as usize) }
    };
    // SAFETY: `user` is the leaked `Handler<F>` this function was registered with.
    let handler = unsafe { &*user.cast::<Handler<F>>() };
    let from = Peer::from_raw(message.peer);
    crate::host::guard(
        "panic in a channel handler",
        || (),
        || match postcard::from_bytes::<M>(bytes) {
            Ok(value) => (handler.handler)(from, value),
            Err(error) => crate::debug!("a message on a channel does not decode: {error}"),
        },
    );
}

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
    /// See the [module](self).
    pub fn channel<M, F>(&mut self, name: &str, version: u16, handler: F) -> Result<Channel<M>, Off>
    where
        M: serde::Serialize + serde::de::DeserializeOwned + 'static,
        F: Fn(Peer, M) + Send + Sync + 'static,
    {
        let host = self.host();
        let full = format!("{name}@{version}");
        let mut id = 0u32;
        crate::load::hand_over(
            Handler { handler },
            |user| {
                (host.api().net_channel)(
                    host.handle(),
                    Str::new(&full),
                    deliver::<M, F>,
                    user,
                    &raw mut id,
                )
            },
            &format!("channel {full}"),
        )?;
        Ok(Channel {
            host,
            id,
            _message: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peers_round_trip_through_their_raw_form() {
        for peer in [Peer::Server, Peer::Player(7), Peer::Player(0)] {
            assert_eq!(Peer::from_raw(peer.to_raw()), peer);
        }
    }
}
