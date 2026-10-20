//! Typed loader events for [`Load::on`](crate::Load::on).
//!
//! Game events are raised on the server's game thread: keep handlers short,
//! and leave slow work to a thread of the mod's own.

use core::ffi::c_void;

use crate::abi::{Event, PlayerData, TickData};

mod sealed {
    pub trait Sealed {}
}

/// An event with its data type; implemented by the types in this module only.
pub trait Kind: sealed::Sealed + 'static {
    /// The raw event.
    const EVENT: Event;
    /// What a handler gets: an ABI struct, or `()` for events without data.
    type Data: 'static;

    /// The event's data from the raw pointer the loader passes; `None` when
    /// it is missing or shorter than this crate's struct.
    ///
    /// # Safety
    ///
    /// `raw` is the pointer the loader passed with [`Kind::EVENT`], valid for
    /// the handler's call.
    unsafe fn data<'a>(raw: *const c_void) -> Option<&'a Self::Data>;
}

macro_rules! without_data {
    ($($(#[$doc:meta])* $name:ident = $event:ident;)*) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, Copy)]
        pub struct $name;

        impl sealed::Sealed for $name {}

        impl Kind for $name {
            const EVENT: Event = Event::$event;
            type Data = ();

            unsafe fn data<'a>(_raw: *const c_void) -> Option<&'a ()> {
                Some(&())
            }
        }
    )*};
}

macro_rules! with_data {
    ($($(#[$doc:meta])* $name:ident = $event:ident: $data:ty;)*) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, Copy)]
        pub struct $name;

        impl sealed::Sealed for $name {}

        impl Kind for $name {
            const EVENT: Event = Event::$event;
            type Data = $data;

            unsafe fn data<'a>(raw: *const c_void) -> Option<&'a $data> {
                // SAFETY: the caller passes the loader's pointer for this event,
                // which starts with a header.
                let data = unsafe { raw.cast::<$data>().as_ref() }?;
                data.header.covers(size_of::<$data>()).then_some(data)
            }
        }
    )*};
}

without_data! {
    /// [`Event::SETTINGS_CHANGED`]; [`Load::on_config_change`](crate::Load::on_config_change)
    /// re-reads the settings for you.
    SettingsChanged = SETTINGS_CHANGED;
    /// [`Event::LOADER_SETTINGS_CHANGED`].
    LoaderSettingsChanged = LOADER_SETTINGS_CHANGED;
    /// [`Event::WORLD_LOADED`].
    WorldLoaded = WORLD_LOADED;
    /// [`Event::WORLD_SAVED`].
    WorldSaved = WORLD_SAVED;
    /// [`Event::SHUTDOWN`].
    Shutdown = SHUTDOWN;
}

with_data! {
    /// [`Event::TICK`].
    Tick = TICK: TickData;
    /// [`Event::PLAYER_JOINED`].
    PlayerJoined = PLAYER_JOINED: PlayerData;
    /// [`Event::PLAYER_LEFT`].
    PlayerLeft = PLAYER_LEFT: PlayerData;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::Header;

    #[test]
    fn data_is_read_only_when_it_is_whole() {
        let player = PlayerData {
            header: Header::of::<PlayerData>(),
            slot: 2,
            entity: 9,
            key: 1,
        };
        let raw = (&raw const player).cast::<c_void>();
        // SAFETY: a valid player for the call.
        let joined = unsafe { PlayerJoined::data(raw) };
        assert_eq!(joined.map(|p| p.entity), Some(9));
        // SAFETY: null is refused.
        assert!(unsafe { PlayerLeft::data(core::ptr::null()) }.is_none());
        let mut short = player;
        short.header.size = 8;
        // SAFETY: as above.
        assert!(unsafe { PlayerJoined::data((&raw const short).cast()) }.is_none());
        // SAFETY: no data is read.
        assert!(unsafe { WorldSaved::data(core::ptr::null()) }.is_some());
    }
}
