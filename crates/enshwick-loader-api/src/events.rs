//! Typed loader events for [`Load::on`](crate::Load::on).
//!
//! Events from the server are raised on its game thread: keep handlers short,
//! and leave slow work to a thread of the mod's own. On a client they come
//! from the built-in server while it hosts a game, solo play included; each
//! hosted game starts with [`WorldLoaded`] and ends with [`ShutdownStarted`], so
//! state of one game is reset there.

use core::ffi::c_void;

use crate::abi::{BuildingData, Event, PlayerData, TickData};

mod sealed {
    pub trait Sealed {}
}

/// An event with its data type; implemented by the types in this module only.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Event;
/// use enshwick_loader_api::events::{Kind, PlayerJoined};
///
/// assert!(PlayerJoined::EVENT == Event::PLAYER_JOINED);
/// ```
pub trait Kind: sealed::Sealed + 'static {
    /// The raw event.
    const EVENT: Event;
    /// What a handler gets: an ABI struct, or `()` for events without data.
    type Data: Copy + 'static;

    /// The event's data from the raw pointer the loader passes; `None` when
    /// it is missing. Fields an older loader does not know are zero.
    ///
    /// # Safety
    ///
    /// `raw` is the pointer the loader passed with [`Kind::EVENT`], valid for
    /// the handler's call.
    unsafe fn data(raw: *const c_void) -> Option<Self::Data>;
}

macro_rules! without_data {
    ($($(#[$doc:meta])* $name:ident = $event:ident;)*) => {$(
        $(#[$doc])*
        ///
        /// # Examples
        ///
        /// See [`Load::on`](crate::Load::on).
        #[derive(Debug, Clone, Copy)]
        pub struct $name;

        impl sealed::Sealed for $name {}

        impl Kind for $name {
            const EVENT: Event = Event::$event;
            type Data = ();

            unsafe fn data(_raw: *const c_void) -> Option<()> {
                Some(())
            }
        }
    )*};
}

macro_rules! with_data {
    ($($(#[$doc:meta])* $name:ident = $event:ident: $data:ty, $born:expr;)*) => {$(
        $(#[$doc])*
        ///
        /// # Examples
        ///
        /// See [`Load::on`](crate::Load::on).
        #[derive(Debug, Clone, Copy)]
        pub struct $name;

        impl sealed::Sealed for $name {}

        impl Kind for $name {
            const EVENT: Event = Event::$event;
            type Data = $data;

            unsafe fn data(raw: *const c_void) -> Option<$data> {
                // SAFETY: the caller passes the loader's pointer for this event,
                // which starts with a header; an all-zero struct is valid.
                unsafe { crate::abi::read_headed(raw.cast::<$data>(), $born) }
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
    ShutdownStarted = SHUTDOWN;
    /// [`Event::JOIN_MISSING`]; [`crate::mods::join_missing`] lists the mods.
    JoinRefused = JOIN_MISSING;
    /// [`Event::ALTARS_CHANGED`]; [`crate::altars::Altars::list`] lists them.
    AltarsChanged = ALTARS_CHANGED;
    /// [`Event::DAY_STARTED`]; [`crate::daytime::Clock::now`] reads the clock.
    DayStarted = DAY_STARTED;
    /// [`Event::NIGHT_STARTED`].
    NightStarted = NIGHT_STARTED;
}

// Each event names its data and the size that data had in the minor that
// added the event: a field appended later is zero under an older loader.
with_data! {
    /// [`Event::TICK`].
    Ticked = TICK: TickData, size_of::<TickData>();
    /// [`Event::PLAYER_JOINED`].
    PlayerJoined = PLAYER_JOINED: PlayerData, size_of::<PlayerData>();
    /// [`Event::PLAYER_LEFT`].
    PlayerLeft = PLAYER_LEFT: PlayerData, size_of::<PlayerData>();
    /// [`Event::BUILDING_PLACED`].
    BuildingPlaced = BUILDING_PLACED: BuildingData, size_of::<BuildingData>();
    /// [`Event::BUILDING_REMOVED`].
    BuildingRemoved = BUILDING_REMOVED: BuildingData, size_of::<BuildingData>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::Header;

    #[test]
    fn data_is_read_only_when_it_has_every_field_of_its_minor() {
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

    #[test]
    fn data_from_a_newer_loader_is_read() {
        #[repr(C)]
        struct Newer {
            player: PlayerData,
            appended: u64,
        }
        let mut newer = Newer {
            player: PlayerData {
                header: Header::of::<PlayerData>(),
                slot: 2,
                entity: 9,
                key: 1,
            },
            appended: 5,
        };
        newer.player.header.size = u32::try_from(size_of::<Newer>()).unwrap();
        // SAFETY: a valid player, as long as its header says.
        let joined = unsafe { PlayerJoined::data((&raw const newer).cast()) }.unwrap();
        assert_eq!((joined.entity, joined.key), (9, 1));
        assert_eq!(newer.appended, 5);
    }
}
