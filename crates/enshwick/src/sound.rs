//! The game's sounds: a client plays any sound container, the game's own or
//! a mod's, by its [`SoundContainerId`].
//!
//! A sound plays inside a frame of the interface with
//! [`Frame::play`](crate::ui::Frame::play), heard only on the client that
//! plays it and from no place in the world.
//!
//! # Examples
//!
//! ```
//! use enshwick::content::Guid;
//! use enshwick::sound::SoundContainerId;
//!
//! const BELL: SoundContainerId = SoundContainerId::from_guid(Guid::named("you.bells:bell"));
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on_frame(|frame| {
//!         frame.window("bells", "Bells", |frame| {
//!             if frame.button("Ring") && frame.play(BELL).is_err() {
//!                 enshwick::warn!("this game holds no bell");
//!             }
//!         });
//!     })
//! }
//! ```

use crate::content::Guid;

/// A sound container by its GUID: the sounds one event of the game plays,
/// and how it plays them.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::sound::SoundContainerId;
///
/// let guid = Guid::named("you.bells:bell");
/// assert_eq!(SoundContainerId::from_guid(guid).guid(), guid);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundContainerId(Guid);

impl SoundContainerId {
    /// The container with GUID `guid`: one of the game's, as its archive
    /// names it, or a mod's, under the GUID of its content id
    /// ([`Guid::named`]).
    ///
    /// # Examples
    ///
    /// See [`SoundContainerId`].
    #[must_use]
    pub const fn from_guid(guid: Guid) -> Self {
        Self(guid)
    }

    /// Its GUID.
    ///
    /// # Examples
    ///
    /// See [`SoundContainerId`].
    #[must_use]
    pub const fn guid(self) -> Guid {
        self.0
    }
}
