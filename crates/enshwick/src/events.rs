//! Typed loader events, and the [`Kind`] that [`Load::on`](crate::Load::on)
//! takes for them and for the typed [game events](crate::game_events).
//!
//! Events from the server are raised on its game thread: keep handlers short,
//! and leave slow work to a thread of the mod's own. On a client they come
//! from the built-in server while it hosts a game, solo play included; each
//! hosted game starts with [`WorldLoaded`] and ends with [`ShutdownStarted`], so
//! state of one game is reset there.
//!
//! Each event is named for what happened, a subject and a verb in the past
//! tense, and its [`Event`] constant is the same name in capitals:
//! [`PlayerJoined`] is [`Event::PLAYER_JOINED`].

use core::ffi::c_void;
use core::time::Duration;

use crate::abi::{
    BuildingData, ChunkData, Event, KnowledgeData, PlayerData, PlayerDeathData, QuestData,
    TickData, TickHandle,
};

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// An event [`Load::on`](crate::Load::on) subscribes to: a loader event of
/// this module or a typed game event of [`game_events`](crate::game_events);
/// implemented by those types only.
///
/// # Examples
///
/// ```
/// use enshwick::events::{Kind, PlayerJoined};
/// use enshwick::game_events::EntityDiedEvent;
///
/// fn watch<E: Kind>(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on::<E>(|_| enshwick::debug!("{}", core::any::type_name::<E>()))
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     watch::<PlayerJoined>(load)?;
///     watch::<EntityDiedEvent>(load)
/// }
/// ```
pub trait Kind: sealed::Sealed + 'static {
    /// What a handler gets: an ABI struct or `()` for a loader event, the
    /// event itself for a game event.
    type Data: 'static;

    /// Resolves what the event needs on this build and subscribes `handler`.
    #[doc(hidden)]
    fn subscribe<F>(load: &mut crate::Load<'_>, handler: F) -> Result<(), crate::Off>
    where
        F: Fn(&Self::Data) + Send + Sync + 'static;
}

/// A loader event, raised under its [`Event`] constant by the loader and by
/// the fake host's `raise`; implemented by the types in this module only.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Event;
/// use enshwick::events::{LoaderEvent, PlayerJoined};
///
/// assert!(PlayerJoined::EVENT == Event::PLAYER_JOINED);
/// ```
pub trait LoaderEvent: Kind<Data: Copy> {
    /// The raw event.
    const EVENT: Event;

    /// The event's data from the raw pointer the loader passes; `None` when
    /// it is missing.
    ///
    /// # Safety
    ///
    /// - `raw` is null or the pointer the loader passed with
    ///   [`LoaderEvent::EVENT`]: a [`Kind::Data`] the loader wrote, readable for the size its header
    ///   states; no alignment is needed.
    /// - No other thread writes that struct during the call.
    unsafe fn data(raw: *const c_void) -> Option<Self::Data>;
}

macro_rules! without_data {
    ($($(#[doc = $doc:literal])* $name:ident = $event:ident;)*) => {
        /// The script entries of the events without data.
        const SCRIPT_PLAIN: &[crate::script::EventKind] = &[$(
            crate::script::EventKind::new(
                stringify!($name),
                concat!($($doc, "\n"),*),
                Event::$event,
                &crate::script::Ty::Nil,
                |_raw: *const c_void| -> Option<crate::script::Value> { Some(crate::script::Value::Nil) },
            ),
        )*];

        $(
        $(#[doc = $doc])*
        ///
        /// # Examples
        ///
        /// See [`Load::on`](crate::Load::on).
        #[derive(Debug, Clone, Copy)]
        pub struct $name;

        impl sealed::Sealed for $name {}

        impl Kind for $name {
            type Data = ();

            fn subscribe<F>(load: &mut crate::Load<'_>, handler: F) -> Result<(), crate::Off>
            where
                F: Fn(&()) + Send + Sync + 'static,
            {
                load.on_raw_event(Event::$event, move |_raw| handler(&()))
            }
        }

        impl LoaderEvent for $name {
            const EVENT: Event = Event::$event;

            unsafe fn data(_raw: *const c_void) -> Option<()> {
                Some(())
            }
        }
        )*
    };
}

/// The subscription of a loader event with data: every raise, or through
/// a function of its own.
macro_rules! subscribe_with_data {
    ($load:ident, $handler:ident, $name:ident, $event:ident) => {
        $load.on_raw_event(Event::$event, move |raw| {
            // SAFETY: this closure is subscribed under
            // `Event::$event`, this type's `LoaderEvent::EVENT`, and
            // `EventFn` has the loader pass that event's data, null or
            // valid with no other thread writing it for the call.
            if let Some(data) = unsafe { <$name as LoaderEvent>::data(raw) } {
                $handler(&data);
            }
        })
    };
    ($load:ident, $handler:ident, $name:ident, $event:ident, $own:path) => {
        $own($load, $handler)
    };
}

macro_rules! with_data {
    ($($(#[doc = $doc:literal])* $name:ident = $event:ident: $data:ty, $born:expr $(, $own:path)?;)*) => {
        /// The script entries of the events with data.
        const SCRIPT_WITH_DATA: &[crate::script::EventKind] = &[$(
            crate::script::EventKind::new(
                stringify!($name),
                concat!($($doc, "\n"),*),
                Event::$event,
                <$data as crate::script::Typed>::TY,
                |raw: *const c_void| -> Option<crate::script::Value> {
                    // SAFETY: `EventKind::read` passes on the loader's pointer
                    // for this event with no other thread writing its data, by
                    // its `# Safety`, which is what `LoaderEvent::data` asks for.
                    unsafe { <$name as LoaderEvent>::data(raw) }.map(crate::script::IntoScript::into_value)
                },
            ),
        )*];

        $(
        $(#[doc = $doc])*
        ///
        /// # Examples
        ///
        /// See [`Load::on`](crate::Load::on).
        #[derive(Debug, Clone, Copy)]
        pub struct $name;

        impl sealed::Sealed for $name {}

        impl Kind for $name {
            type Data = $data;

            fn subscribe<F>(load: &mut crate::Load<'_>, handler: F) -> Result<(), crate::Off>
            where
                F: Fn(&$data) + Send + Sync + 'static,
            {
                subscribe_with_data!(load, handler, $name, $event $(, $own)?)
            }
        }

        impl LoaderEvent for $name {
            const EVENT: Event = Event::$event;

            unsafe fn data(raw: *const c_void) -> Option<$data> {
                // SAFETY: the caller guarantees `raw` is null or this event's
                // data, readable for the size its header states with no
                // concurrent writer, which is the contract of `read_headed`.
                unsafe { crate::abi::read_headed(raw.cast::<$data>(), $born) }
            }
        }
        )*
    };
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
    /// [`Event::SHUTDOWN_STARTED`].
    ShutdownStarted = SHUTDOWN_STARTED;
    /// [`Event::JOIN_REFUSED`]; [`crate::mods::join_missing`] lists the mods.
    JoinRefused = JOIN_REFUSED;
    /// [`Event::ALTARS_CHANGED`]; [`crate::altars::Altars::list`] lists them.
    AltarsChanged = ALTARS_CHANGED;
    /// [`Event::DAY_STARTED`]; [`crate::daytime::Clock::now`] reads the clock.
    DayStarted = DAY_STARTED;
    /// [`Event::NIGHT_STARTED`].
    NightStarted = NIGHT_STARTED;
    /// [`Event::SETTINGS_REPLACED`]; [`Load::on_config_change`](crate::Load::on_config_change)
    /// re-reads the settings for you, the ones that wait for a start too.
    SettingsReplaced = SETTINGS_REPLACED;
}

// Each event names its data and the size that data had in the minor that
// added the event, the part every loader that runs the mod writes.
with_data! {
    /// [`Event::TICKED`], every [`Ticked::DEFAULT_EVERY`] through
    /// [`Load::on`](crate::Load::on); [`Load::on_tick`](crate::Load::on_tick)
    /// picks the period and [`Load::on_every_step`](crate::Load::on_every_step)
    /// runs on every server step.
    Ticked = TICKED: TickData, core::mem::offset_of!(TickData, elapsed_steps), subscribe_ticked;
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

impl Ticked {
    /// How often a tick handler runs unless it asks otherwise: about every
    /// second step of an empty dedicated server (20 steps a second, build
    /// 399), more steps apart when the server steps faster.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// assert_eq!(enshwick::events::Ticked::DEFAULT_EVERY, Duration::from_millis(100));
    /// ```
    pub const DEFAULT_EVERY: Duration = Duration::from_millis(100);
}

/// [`Ticked`] through [`Load::on`](crate::Load::on): on the default period.
fn subscribe_ticked<F>(load: &mut crate::Load<'_>, handler: F) -> Result<(), crate::Off>
where
    F: Fn(&TickData) + Send + Sync + 'static,
{
    load.on_tick(Ticked::DEFAULT_EVERY, handler).map(|_| ())
}

/// A tick handler's period, from [`Load::on_tick`](crate::Load::on_tick);
/// copy it anywhere, the handler itself included, to change the period
/// while the game runs.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let period = load.on_tick(Duration::from_secs(1), |_| enshwick::debug!("a second"))?;
///     period.set_every(Duration::from_millis(250))
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct TickPeriod {
    host: crate::Host,
    handle: TickHandle,
}

impl TickPeriod {
    pub(crate) const fn new(host: crate::Host, handle: TickHandle) -> Self {
        Self { host, handle }
    }

    /// Runs the handler about every `every` from the next step on: its next
    /// call comes `every` after its last, or on the next step when that time
    /// has passed. [`Duration::ZERO`] runs it on every step.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when the loader refuses: this mod is off.
    ///
    /// # Examples
    ///
    /// See [`TickPeriod`].
    pub fn set_every(self, every: Duration) -> Result<(), crate::Off> {
        let status =
            (self.host.api().set_tick_every)(self.host.handle(), self.handle, nanos(every));
        crate::load::ok(status, (), || "the tick period".to_owned())
    }
}

/// `duration` in nanoseconds, the longest the ABI holds past that.
pub(crate) fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// Every event's script entry, for [`crate::script::events`].
pub(crate) const SCRIPT_EVENTS: [&[crate::script::EventKind]; 2] = [SCRIPT_PLAIN, SCRIPT_WITH_DATA];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::Header;

    #[test]
    fn data_is_read_only_when_it_has_every_field_of_its_minor() {
        let player = PlayerData {
            header: Header::of::<PlayerData>(),
            slot: 2,
            entity: crate::Entity(9),
            key: crate::PlayerKey::new(1),
        };
        let raw = (&raw const player).cast::<c_void>();
        // SAFETY: `player` is a whole `PlayerData` on this stack frame,
        // readable for the size its header states.
        let joined = unsafe { PlayerJoined::data(raw) };
        assert_eq!(joined.map(|p| p.entity), Some(crate::Entity(9)));
        // SAFETY: `data` accepts a null pointer and reads nothing.
        assert!(unsafe { PlayerLeft::data(core::ptr::null()) }.is_none());
        let mut short = player;
        short.header.size = 8;
        // SAFETY: `short` is a whole `PlayerData` on this stack frame, its
        // header stating fewer bytes than it holds.
        assert!(unsafe { PlayerJoined::data((&raw const short).cast()) }.is_none());
        // SAFETY: `data` of an event without data reads nothing.
        assert!(unsafe { WorldSaved::data(core::ptr::null()) }.is_some());
    }

    #[test]
    fn a_death_from_minor_60_reads_without_its_killer_entity() {
        let mut death = PlayerDeathData {
            header: Header::of::<PlayerDeathData>(),
            entity: crate::Entity(3),
            cause: PlayerDeathData::IMPACT,
            key: crate::PlayerKey::new(7),
            position: [0.0; 3],
            cause_guid: [1; 16],
            was_deadly_fog: 0,
            drowned: 0,
            reserved: 0,
            killer_entity: crate::Entity(9),
            reserved_tail: 0,
        };
        death.header.size = 56;
        // SAFETY: `death` is a whole `PlayerDeathData` on this stack frame,
        // its header stating fewer bytes than it holds.
        let read = unsafe { PlayerDied::data((&raw const death).cast()) }.unwrap();
        assert_eq!(
            (read.entity, read.killer_entity),
            (crate::Entity(3), crate::Entity(0))
        );
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
                entity: crate::Entity(9),
                key: crate::PlayerKey::new(1),
            },
            appended: 5,
        };
        newer.player.header.size = u32::try_from(size_of::<Newer>()).unwrap();
        // SAFETY: `newer` is on this stack frame and starts with a whole
        // `PlayerData` whose header states the size of `Newer`, all of it
        // readable.
        let joined = unsafe { PlayerJoined::data((&raw const newer).cast()) }.unwrap();
        assert_eq!(
            (joined.entity, joined.key),
            (crate::Entity(9), crate::PlayerKey::new(1))
        );
        assert_eq!(newer.appended, 5);
    }
}
