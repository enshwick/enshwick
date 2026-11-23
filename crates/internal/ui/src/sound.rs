//! The game's sounds: a client plays any sound container, the game's own or
//! a mod's, by its [`SoundContainerId`], and a mod changes how a container
//! plays and which recordings ([`SoundId`]) it picks from.
//!
//! A sound plays inside a frame of the interface with
//! [`Frame::play`](crate::ui::Frame::play), heard only on the client that
//! plays it and from no place in the world. The server places sounds through
//! notifiers: a mod hears each one the game raises in
//! [`Load::on_notify`](../struct.Load.html#method.on_notify) and raises one
//! of its own, which every client in range plays, with `Effects::notify`.
//!
//! Which container a game part plays stays a value of that part, changed
//! through the resource values: a notifier's `defaultSound`, an item's
//! equip sound.
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

#[cfg(feature = "unstable")]
pub(crate) mod add;
pub(crate) mod change;
#[cfg(feature = "unstable")]
mod decode;
#[cfg(feature = "unstable")]
pub(crate) mod qoa;
#[cfg(feature = "unstable")]
pub(crate) mod recording;
pub(crate) mod script;

use crate::Off;
use crate::abi::{Side, Str};
use crate::content::{EngineFacts as _, Guid, types};
use crate::game_events::{NotifyEvent, NotifyEventType};
use crate::resources::{Registry, Value};

#[cfg(feature = "unstable")]
pub use add::{
    AddedSound, INLINE_FRAMES, MAX_FILE_BYTES, MAX_MOD_BYTES, NewSound, NewTake, SAMPLE_RATE,
};
pub use change::{
    MAX_COUNT, MAX_DISTANCE, MAX_SLEEP, MAX_TAKES, MAX_VOLUME, PITCH, RandomPosition, SoundChange,
    SoundContainerScene, SoundContainerStealingMode, Take, Takes,
};
#[cfg(feature = "unstable")]
pub use decode::{Channels, Trim};

/// The longest take a mod adds, after its trim.
pub const MAX_TAKE: core::time::Duration = core::time::Duration::from_secs(600);
/// The gains in decibels a mod's sound takes.
pub const GAIN_DB: core::ops::RangeInclusive<f32> = -60.0..=24.0;
/// The most takes a mod's new sound holds.
pub const MAX_NEW_TAKES: usize = 32;
/// The content id part of each take's recording of a mod's new sound, in
/// take order.
const TAKE_PARTS: [&str; MAX_NEW_TAKES] = [
    "sound", "sound2", "sound3", "sound4", "sound5", "sound6", "sound7", "sound8", "sound9",
    "sound10", "sound11", "sound12", "sound13", "sound14", "sound15", "sound16", "sound17",
    "sound18", "sound19", "sound20", "sound21", "sound22", "sound23", "sound24", "sound25",
    "sound26", "sound27", "sound28", "sound29", "sound30", "sound31", "sound32",
];

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
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

/// One recording, the game's `SoundResource`, by its GUID: what a
/// container's [`Take`] plays.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::sound::SoundId;
///
/// let guid = Guid::named("you.bells:bell.sound");
/// assert_eq!(SoundId::from_guid(guid).guid(), guid);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct SoundId(Guid);

impl SoundId {
    /// The recording with GUID `guid`: one of the game's, as its archive
    /// names it, or a mod's, under the GUID of its content id.
    ///
    /// # Examples
    ///
    /// See [`SoundId`].
    #[must_use]
    pub const fn from_guid(guid: Guid) -> Self {
        Self(guid)
    }

    /// Its GUID.
    ///
    /// # Examples
    ///
    /// See [`SoundId`].
    #[must_use]
    pub const fn guid(self) -> Guid {
        self.0
    }
}

/// The qualified name of the game's container entry type, which every take
/// is.
fn entry_type(load: &crate::BaseLoad) -> Result<String, Off> {
    let hash = load.hash(crate::engine_ids::SOUND_CONTAINER_SOUND_ENTRY)?;
    Registry::game()?
        .hashed(hash)
        .map(|entry| entry.qualified_name().to_owned())
        .ok_or_else(|| Off::new("this game has no sound container entry"))
}

/// Checks that `sound` names a recording this mod may play: one of the
/// game's, or one a mod loaded before it added and lets it name. A mod's
/// recordings are the only content named `<sound>.sound` to
/// `<sound>.sound32`, so another id, a container's or an item's, is none.
fn known_sound(host: crate::Host, sound: SoundId) -> Result<(), Off> {
    let guid = sound.guid();
    if host
        .game_resource(types::SOUND_RESOURCE.key(guid)?)
        .is_some()
    {
        return Ok(());
    }
    let name = host.content_name(guid).ok_or_else(|| {
        Off::new(format!(
            "sound {guid} is neither the game's nor a mod's loaded before"
        ))
    })?;
    let recording = name
        .rsplit_once('.')
        .is_some_and(|(_, part)| TAKE_PARTS.contains(&part));
    if !recording {
        return Err(Off::new(format!("sound {name} is no recording")));
    }
    let status = (host.api().use_content)(host.handle(), Str::new(&name));
    crate::load::ok(status, (), || format!("sound {name}"))
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::change_sound`.
    #[doc(hidden)]
    fn change_sound(&mut self, sound: SoundContainerId, change: SoundChange) -> Result<(), Off>;

    /// As `Load::on_notify`.
    #[doc(hidden)]
    fn on_notify<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&NotifyEvent) + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn on_notify<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&NotifyEvent) + Send + Sync + 'static,
    {
        self.on::<NotifyEvent>(move |notify| {
            if notify.r#type != NotifyEventType::Vfx {
                handler(notify);
            }
        })
    }

    fn change_sound(&mut self, sound: SoundContainerId, change: SoundChange) -> Result<(), Off> {
        let container = sound.guid();
        let fail = move |why: &dyn core::fmt::Display| {
            Off::new(format!("sound container {container}: {why}"))
        };
        let fields = change.fields().map_err(|why| fail(&why))?;
        if self.host().build().side != Side::CLIENT {
            return Ok(());
        }
        let host = self.host();
        for take in change::named_sounds(&change) {
            known_sound(host, SoundId::from_guid(take)).map_err(|why| fail(&why))?;
        }
        let distances = SoundChange {
            min_distance: change.min_distance,
            max_distance: change.max_distance,
            ..SoundChange::default()
        };
        let takes = match change.takes {
            Some(takes) => Some((takes, entry_type(self)?)),
            None => None,
        };
        let key = types::SOUND_CONTAINER_RESOURCE.key(container)?;
        crate::resources::serve_value(host, key, key, move |held| {
            let (min, max) = change::held_distances(&held);
            change::check_distances(&distances, min, max).map_err(|why| fail(&why))?;
            let mut fields = fields.clone();
            if let Some((takes, entry_type)) = &takes {
                let entries =
                    change::merged_entries(held.field(change::ENTRIES), takes, entry_type)
                        .map_err(|why| fail(&why))?;
                fields.push((change::ENTRIES.to_owned(), entries));
            }
            Ok(Value::Struct(fields))
        })
    }
}
