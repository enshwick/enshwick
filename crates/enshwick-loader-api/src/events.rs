//! Typed loader events for [`Load::on`](crate::Load::on).
//!
//! Events from the server are raised on its game thread: keep handlers short,
//! and leave slow work to a thread of the mod's own. On a client they come
//! from the built-in server while it hosts a game, solo play included; each
//! hosted game starts with [`WorldLoaded`] and ends with [`ShutdownStarted`], so
//! state of one game is reset there.

use core::ffi::c_void;

use crate::abi::{
    BuildingData, ChunkData, Event, KnowledgeData, PlayerData, PlayerDeathData, QuestData, TickData,
};

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
    /// it is missing.
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
    /// [`Event::SETTINGS_CHANGED`], on no fixed thread;
    /// [`Load::on_config_change`](crate::Load::on_config_change) re-reads the
    /// settings for you.
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
    /// [`Event::WORLD_SETTINGS`]; [`Load::on_config_change`](crate::Load::on_config_change)
    /// re-reads the settings for you, the ones that wait for a start too.
    WorldSettings = WORLD_SETTINGS;
}

// Each event names its data and the size that data had in the minor that
// added the event, the part every loader that runs the mod writes.
with_data! {
    /// [`Event::TICK`].
    Ticked = TICK: TickData, size_of::<TickData>();
    /// [`Event::PLAYER_JOINED`].
    PlayerJoined = PLAYER_JOINED: PlayerData, size_of::<PlayerData>();
    /// [`Event::PLAYER_LEFT`].
    PlayerLeft = PLAYER_LEFT: PlayerData, size_of::<PlayerData>();
    /// [`Event::BUILDING_PLACED`].
    BuildingPlaced = BUILDING_PLACED: BuildingData, size_of::<BuildingData>();
    /// [`Event::KNOWLEDGE_CHANGED`]; [`crate::knowledge::Knowledge`] reads and grants.
    KnowledgeChanged = KNOWLEDGE_CHANGED: KnowledgeData, size_of::<KnowledgeData>();
    /// [`Event::QUEST_COMPLETED`].
    QuestCompleted = QUEST_COMPLETED: QuestData, size_of::<QuestData>();
    /// [`Event::CHUNK_LOADED`].
    ChunkLoaded = CHUNK_LOADED: ChunkData, size_of::<ChunkData>();
    /// [`Event::CHUNK_UNLOADED`].
    ChunkUnloaded = CHUNK_UNLOADED: ChunkData, size_of::<ChunkData>();
    /// [`Event::BUILDING_REMOVED`].
    BuildingRemoved = BUILDING_REMOVED: BuildingData, size_of::<BuildingData>();
    /// [`Event::PLAYER_DIED`].
    PlayerDied = PLAYER_DIED: PlayerDeathData, core::mem::offset_of!(PlayerDeathData, killer_entity);
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
    fn a_death_from_minor_60_reads_without_its_killer_entity() {
        let mut death = PlayerDeathData {
            header: Header::of::<PlayerDeathData>(),
            entity: 3,
            cause: PlayerDeathData::HIT,
            key: 7,
            position: [0.0; 3],
            killer: [1; 16],
            deadly_shroud: 0,
            drowned: 0,
            reserved: 0,
            killer_entity: 9,
            reserved_tail: 0,
        };
        death.header.size = 56;
        // SAFETY: a valid death, as long as its header says.
        let read = unsafe { PlayerDied::data((&raw const death).cast()) }.unwrap();
        assert_eq!((read.entity, read.killer_entity), (3, 0));
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
