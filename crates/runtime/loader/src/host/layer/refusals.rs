//! The status and policy of every domain error a mod's call can meet: the
//! one place a domain error turns into a [`Status`], so domain code never
//! names one.

use core::fmt;

use enshwick_core::abi::Status;

use super::{Policy, Refusal};

mod actions;
mod content;

/// A loader rule the call breaks: refused, with `0` as the calling mod's
/// warning, for a check that has no error type of its own, such as a name
/// the owning mod does not export.
#[derive(Debug)]
pub(crate) struct Rule(pub(crate) String);

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Refusal for Rule {
    fn status(&self) -> Status {
        Status::REFUSED
    }

    fn policy(&self) -> Policy {
        Policy::Warn
    }
}

/// A rule the call breaks that leaves the calling mod unable to run as
/// written: refused, and the mod turns off with `0` as the reason.
#[derive(Debug)]
pub(crate) struct RuleOff(pub(crate) String);

impl fmt::Display for RuleOff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Refusal for RuleOff {
    fn status(&self) -> Status {
        Status::REFUSED
    }

    fn policy(&self) -> Policy {
        Policy::TurnOff
    }
}

/// A content id another mod holds, or one malformed or outside the mod's
/// namespace: the mod's content would collide, so it turns off.
impl Refusal for crate::base::ids::DeclareIdError {
    fn status(&self) -> Status {
        Status::REFUSED
    }

    fn policy(&self) -> Policy {
        Policy::TurnOff
    }
}

/// A full mask leaves the mod's prop without its bit, so the mod turns off.
impl Refusal for crate::base::ids::CraftingPropError {
    fn status(&self) -> Status {
        Status::REFUSED
    }

    fn policy(&self) -> Policy {
        Policy::TurnOff
    }
}

/// Refusals for declarations that break the loader's rules: a name twice or
/// outside the mod's namespace, or more than an id holds. Each is logged as
/// the calling mod's warning.
macro_rules! warned {
    ($($error:ty),* $(,)?) => {$(
        impl Refusal for $error {
            fn status(&self) -> Status {
                Status::REFUSED
            }

            fn policy(&self) -> Policy {
                Policy::Warn
            }
        }
    )*};
}

warned!(
    crate::base::ids::UndeclaredContentError,
    crate::base::calls::DeclareCallError,
    crate::base::points::DeclarePointError,
    crate::base::services::DeclareServiceError,
);

/// A registry sealed once init ends: the mod asked too late.
impl Refusal for crate::engine::registry::Sealed {
    fn status(&self) -> Status {
        Status::TOO_LATE
    }
}

/// Too late after init; unavailable where the build or the side lacks the
/// feature; refused where another registration holds the place.
impl Refusal for crate::engine::registry::JoinFeatureError {
    fn status(&self) -> Status {
        match self {
            Self::TooLate => Status::TOO_LATE,
            Self::Unavailable => Status::UNAVAILABLE,
            Self::Taken => Status::REFUSED,
        }
    }
}

/// A malformed prompt is a bad argument; the rest as for registering.
impl Refusal for crate::actions::interaction_prompts::PromptError {
    fn status(&self) -> Status {
        match self {
            Self::Malformed => Status::BAD_ARGS,
            Self::Register(error) => error.status(),
        }
    }
}

/// A malformed alias is a bad argument; a second ask or a taken name is
/// refused, the taken name already reported as a part off.
impl Refusal for crate::actions::commands::NameCommandsError {
    fn status(&self) -> Status {
        match self {
            Self::Register(error) => error.status(),
            Self::Malformed => Status::BAD_ARGS,
            Self::Twice | Self::Taken => Status::REFUSED,
        }
    }
}

/// A bad name or mod index is a bad argument; a one-sided mod or a channel open already
/// is refused.
impl Refusal for crate::base::net::OpenChannelError {
    fn status(&self) -> Status {
        match self {
            Self::Register(error) => error.status(),
            Self::BadName | Self::NoMod => Status::BAD_ARGS,
            Self::OneSided | Self::Twice => Status::REFUSED,
        }
    }
}

/// What a data kind's open, read or write met; its log line, for a kind
/// open for the other scope, is written where it is found.
impl Refusal for crate::inventory::item_data::UseItemDataError {
    fn status(&self) -> Status {
        match self {
            Self::Register(error) => error.status(),
            Self::BadId | Self::UnknownKind | Self::TooLong | Self::NoHolder => Status::BAD_ARGS,
            Self::OtherScope | Self::Full => Status::REFUSED,
            Self::NotServer | Self::NoWorld | Self::Unopened => Status::UNAVAILABLE,
            Self::NotKept => Status::NOT_KEPT,
            Self::WrongThread => Status::WRONG_THREAD,
            Self::NoValue => Status::MISSING,
        }
    }
}

/// A malformed control is a bad argument; one declared twice is refused.
impl Refusal for crate::actions::controls::DeclareControlError {
    fn status(&self) -> Status {
        match self {
            Self::Register(error) => error.status(),
            Self::Malformed => Status::BAD_ARGS,
            Self::Twice => Status::REFUSED,
        }
    }
}

/// Unavailable off a client, missing for a screen the build lacks; a bad edge or a
/// screen of play is a bad argument.
impl Refusal for crate::ui::interface::ExtendScreenError {
    fn status(&self) -> Status {
        match self {
            Self::Register(error) => error.status(),
            Self::NotClient => Status::UNAVAILABLE,
            Self::UnknownScreen => Status::MISSING,
            Self::BadPlace | Self::ScreenOfPlay => Status::BAD_ARGS,
        }
    }
}

/// A mod's write of its own setting, answered as `HostApi::set_config`
/// states it and logged as the mod's warning.
impl Refusal for crate::base::configure::SetOwnSettingError {
    fn status(&self) -> Status {
        use crate::base::configure::{ApplySettingError as Apply, ChangeSettingError as Change};
        match &*self.error {
            Apply::NoWorld => Status::UNAVAILABLE,
            Apply::NoSchema { .. } | Apply::Change(Change::Find(_)) => Status::MISSING,
            Apply::NoMod
            | Apply::Change(
                Change::BadValue { .. } | Change::Refused { .. } | Change::NoElement { .. },
            ) => Status::BAD_ARGS,
            Apply::Change(
                Change::Secret { .. } | Change::Toml { .. } | Change::Unwritable { .. },
            )
            | Apply::Write { .. }
            | Apply::Check(_)
            | Apply::Nested => Status::REFUSED,
        }
    }

    fn policy(&self) -> Policy {
        Policy::Warn
    }
}

/// A level or experience write: unavailable without the level step,
/// refused for a mod that did not ask or an entity no player has, busy for
/// a full queue.
impl Refusal for crate::knowledge::progress::WriteProgressError {
    fn status(&self) -> Status {
        match self {
            Self::BadKind => Status::BAD_ARGS,
            Self::Unavailable => Status::UNAVAILABLE,
            Self::Register(error) => error.status(),
            Self::NotAsked | Self::NoPlayer => Status::REFUSED,
            Self::Full => Status::BUSY,
            Self::World(error) => error.status(),
        }
    }
}

/// A stat or affix call: too late after init; unavailable off the server
/// or while stats do not run; missing for what is not defined; bad
/// arguments for what does not read; refused outside the mod's namespace,
/// defined twice or past an index; a key another mod holds turns the mod off.
impl Refusal for crate::inventory::stats::StatsError {
    fn status(&self) -> Status {
        match self {
            Self::TooLate => Status::TOO_LATE,
            Self::NotRunning | Self::NotServer => Status::UNAVAILABLE,
            Self::Undefined | Self::NoAffix => Status::MISSING,
            Self::NoMod
            | Self::BadId
            | Self::BadDef
            | Self::BadStat
            | Self::BadModifier
            | Self::NothingToWatch => Status::BAD_ARGS,
            Self::NotOwn | Self::Taken(_) | Self::Defined | Self::Full | Self::NotPlayer => {
                Status::REFUSED
            }
            Self::World(error) => error.status(),
        }
    }

    fn policy(&self) -> Policy {
        match self {
            Self::Taken(_) => Policy::TurnOff,
            _ => Policy::Quiet,
        }
    }
}

/// A knowledge read or change: unavailable without the build's reads, the
/// store, its queue or a world; missing where the id read or an answer is
/// not there; a bad argument for a malformed change; refused for a
/// character without its record or save; busy for a full queue.
impl Refusal for crate::knowledge::store::KnowledgeError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoStore | Self::NoQueue | Self::NoWorld => Status::UNAVAILABLE,
            Self::Unknown | Self::Unanswered => Status::MISSING,
            Self::BadChange => Status::BAD_ARGS,
            Self::PlayerOnly | Self::NoRecord | Self::NotLoaded => Status::REFUSED,
            Self::Full => Status::BUSY,
            Self::Fuse(error) => error.status(),
            Self::World(error) => error.status(),
        }
    }
}

/// A mod's sound call refused: unavailable off a client and while the
/// sounds are off, a bad argument out of bounds, missing for an unknown
/// container, busy past the frame's starts or with the mod's queue full.
impl Refusal for crate::ui::sound::SoundError {
    fn status(&self) -> Status {
        use crate::ui::sound::SoundError;
        match self {
            SoundError::NotClient | SoundError::Off => Status::UNAVAILABLE,
            SoundError::BadArgs => Status::BAD_ARGS,
            SoundError::Missing => Status::MISSING,
            SoundError::Full => Status::BUSY,
        }
    }
}

/// A mod's sound for clients refused: unavailable where no server runs,
/// refused for a mod not on both sides, a bad argument out of bounds,
/// missing for an absent entity, the wrong thread for a read of the world
/// off the game thread, busy past the step's sounds.
impl Refusal for crate::ui::sound::RelayError {
    fn status(&self) -> Status {
        use crate::ui::sound::RelayError;
        match self {
            RelayError::NotServing => Status::UNAVAILABLE,
            RelayError::OneSided => Status::REFUSED,
            RelayError::BadArgs => Status::BAD_ARGS,
            RelayError::NoEntity => Status::MISSING,
            RelayError::WrongThread => Status::WRONG_THREAD,
            RelayError::Full => Status::BUSY,
        }
    }
}

/// An item rolled anew: unavailable while affixes are not open, a bad argument
/// without an item, refused when no roll fits or the stats did not encode.
impl Refusal for crate::inventory::stats::RollAffixesError {
    fn status(&self) -> Status {
        match self {
            Self::Unopened => Status::UNAVAILABLE,
            Self::NoItem => Status::BAD_ARGS,
            Self::NoRoll | Self::Encode => Status::REFUSED,
            Self::Kept(error) => error.status(),
            Self::World(error) => error.status(),
            Self::Item(error) => error.status(),
        }
    }
}

/// The world unreadable now: off the game thread or inside a step is the
/// wrong thread; the entity gone missing; the rest, a build without the
/// reads, no world or no server, unavailable.
impl Refusal for crate::engine::world::WorldError {
    fn status(&self) -> Status {
        match self {
            Self::Absent => Status::MISSING,
            Self::Unread | Self::NoWorld | Self::NoServer | Self::UnreadForCall => {
                Status::UNAVAILABLE
            }
            Self::WrongThread | Self::InStep => Status::WRONG_THREAD,
        }
    }
}

/// A bad account or bit is a bad argument, an account not in game refused;
/// the rest is the world's own answer.
#[cfg(windows)]
impl Refusal for crate::world::permissions::PermissionError {
    fn status(&self) -> Status {
        match self {
            Self::BadBits => Status::BAD_ARGS,
            Self::NotInGame => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// A character that is no player, or a point gone, is missing; an entity
/// that is no spawn point a bad argument; an event this build lays out
/// wrong unavailable, so a mod never reads a layout gap as its own mistake;
/// the rest the world's, the write's or the event's own answer.
#[cfg(windows)]
impl Refusal for crate::world::respawn::RespawnError {
    fn status(&self) -> Status {
        use crate::base::emits::EmitError;
        match self {
            Self::Unread | Self::Emit(EmitError::BadSize) => Status::UNAVAILABLE,
            Self::NotPlayer | Self::NoPoint => Status::MISSING,
            Self::NotASpawnPoint => Status::BAD_ARGS,
            Self::World(error) => error.status(),
            Self::Write(error) => error.status(),
            Self::Emit(error) => error.status(),
        }
    }
}

/// A link or patch after arming is too late, a malformed one a bad
/// argument, one another holds refused.
impl Refusal for crate::engine::chains::LinkError {
    fn status(&self) -> Status {
        match self {
            Self::Armed => Status::TOO_LATE,
            Self::Malformed => Status::BAD_ARGS,
            Self::SlotTaken | Self::Clash => Status::REFUSED,
        }
    }
}

/// No handler is missing; another mod's is refused.
impl Refusal for crate::engine::ticks::SetEveryError {
    fn status(&self) -> Status {
        match self {
            Self::NoHandler => Status::MISSING,
            Self::OtherMod => Status::REFUSED,
        }
    }
}

/// A write refused once its feature is off, or before its live object
/// exists, is unavailable.
impl Refusal for crate::engine::fuse::FuseError {
    fn status(&self) -> Status {
        match self {
            Self::Off | Self::Unmade => Status::UNAVAILABLE,
        }
    }
}

/// A bad box or voxel is a bad argument; a box out of the world or not
/// loaded says so; the rest is unavailable, or the world's own answer.
impl Refusal for crate::world::voxels::VoxelError {
    fn status(&self) -> Status {
        match self {
            Self::BadBox | Self::BadVoxel => Status::BAD_ARGS,
            Self::Unread | Self::NoEditor => Status::UNAVAILABLE,
            Self::World(error) => error.status(),
            Self::OutsideWorld => Status::OUTSIDE_WORLD,
            Self::NotLoaded => Status::NOT_LOADED,
        }
    }
}

/// A malformed edit or material is a bad argument; one out of the world
/// says so; the rest is unavailable, or the world's or the voxels' own
/// answer.
impl Refusal for crate::world::terrain::TerrainError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoEditor => Status::UNAVAILABLE,
            Self::BadEdit | Self::BadMaterial => Status::BAD_ARGS,
            Self::OutsideWorld => Status::OUTSIDE_WORLD,
            Self::World(error) => error.status(),
            Self::Voxels(error) => error.status(),
        }
    }
}

/// A bad box is a bad argument; the rest is unavailable, or the world's
/// own answer.
impl Refusal for crate::world::rights::RightsError {
    fn status(&self) -> Status {
        match self {
            Self::BadBox => Status::BAD_ARGS,
            Self::Unread | Self::NoZones => Status::UNAVAILABLE,
            Self::World(error) => error.status(),
        }
    }
}

/// Where inventories do not run, unavailable; malformed operations, a bad
/// argument.
impl Refusal for crate::inventory::transactions::SubmitInventoryError {
    fn status(&self) -> Status {
        match self {
            Self::Unavailable => Status::UNAVAILABLE,
            Self::BadOps => Status::BAD_ARGS,
        }
    }
}

/// An item the game does not have is missing; a build, registry or world
/// without the item lookup unavailable.
impl Refusal for crate::inventory::transactions::ItemInfoError {
    fn status(&self) -> Status {
        match self {
            Self::NoItem => Status::MISSING,
            Self::Unread | Self::NoRegistry | Self::NoWorld => Status::UNAVAILABLE,
        }
    }
}

/// A spawn the build or the world cannot take is unavailable, a malformed one a
/// bad argument, one the engine refuses refused.
impl Refusal for crate::inventory::spawns::SpawnError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoWorld => Status::UNAVAILABLE,
            Self::BadRequest => Status::BAD_ARGS,
            Self::Refused => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// A pickup that does not read is unavailable, a malformed drop a bad
/// argument; the item lookup, the world and the spawn give their own
/// answer.
impl Refusal for crate::inventory::drops::DropError {
    fn status(&self) -> Status {
        match self {
            Self::Unread => Status::UNAVAILABLE,
            Self::BadRequest => Status::BAD_ARGS,
            Self::Item(error) => error.status(),
            Self::World(error) => error.status(),
            Self::Spawn(error) => error.status(),
        }
    }
}

/// No entity list or entity call is unavailable; entity 0 a bad
/// argument; a player's character, a vanilla entity or the engine's
/// refusal refused.
impl Refusal for crate::world::entities::EntityError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoWorld | Self::Unavailable => Status::UNAVAILABLE,
            Self::NoEntity => Status::BAD_ARGS,
            Self::Player | Self::NotSpawned | Self::Refused => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// A hold the hooks cannot make is unavailable.
impl Refusal for crate::world::prop_holds::HoldError {
    fn status(&self) -> Status {
        Status::UNAVAILABLE
    }
}

/// A transaction's reason, the status a call about it and its record's
/// reason both answer: malformed changes are bad arguments; a prop the
/// game would not hand back, an undo it cannot make, rights or a world
/// that changed are refused; no room in the queue is busy; a change that cannot run here
/// unavailable, an entity or blueprint gone missing; the rest says what it
/// is, or is the
/// answer of what the change read.
impl Refusal for crate::world::transactions::TransactionError {
    fn status(&self) -> Status {
        match self {
            Self::BadBox
            | Self::BadEdit
            | Self::BadCount
            | Self::BadPlace
            | Self::BadItem
            | Self::BadShape => Status::BAD_ARGS,
            Self::NotAProp => Status::NOT_A_PROP,
            Self::Unavailable => Status::UNAVAILABLE,
            Self::Gone => Status::MISSING,
            Self::OutsideWorld => Status::OUTSIDE_WORLD,
            Self::NotLoaded => Status::NOT_LOADED,
            Self::NotUndoable
            | Self::NoRights
            | Self::HoldsState
            | Self::Changed
            | Self::BlueprintChanged => Status::REFUSED,
            Self::Full => Status::BUSY,
            Self::World(error) => error.status(),
            Self::Voxels(error) => error.status(),
            Self::Terrain(error) => error.status(),
            Self::Rights(error) => error.status(),
            Self::Item(error) => error.status(),
            Self::Spawn(error) => error.status(),
            Self::Entity(error) => error.status(),
        }
    }
}

/// No weather is unavailable, a bad one a bad argument; the rest is the fuse's
/// or the world's own answer.
impl Refusal for crate::world::weather::WeatherError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoSystem => Status::UNAVAILABLE,
            Self::BadWeather => Status::BAD_ARGS,
            Self::Fuse(error) => error.status(),
            Self::World(error) => error.status(),
        }
    }
}

/// No clock or speed link is unavailable, a bad factor a bad argument, a mod
/// that did not ask refused.
impl Refusal for crate::world::daytime::DaytimeError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoClock | Self::SpeedOff => Status::UNAVAILABLE,
            Self::BadFactor => Status::BAD_ARGS,
            Self::NotAsked => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// Off a server or without a ready link, unavailable; a bad circle a bad
/// argument; a message not sent refused.
impl Refusal for crate::world::fog::UncoverError {
    fn status(&self) -> Status {
        match self {
            Self::NotServer | Self::NotReady => Status::UNAVAILABLE,
            Self::BadCircle => Status::BAD_ARGS,
            Self::NotSent => Status::REFUSED,
        }
    }
}

/// A mod that did not open the fog is refused; no copy is unavailable.
impl Refusal for crate::world::fog::FogReadError {
    fn status(&self) -> Status {
        match self {
            Self::Unopened => Status::REFUSED,
            Self::NoCopy => Status::UNAVAILABLE,
        }
    }
}

/// A box out of the fog is outside the world; a short buffer a bad
/// argument.
impl Refusal for crate::world::fog_image::ReadError {
    fn status(&self) -> Status {
        match self {
            Self::Outside => Status::OUTSIDE_WORLD,
            Self::Short => Status::BAD_ARGS,
        }
    }
}

/// No write is unavailable, entity 0 a bad argument, a type mods do not
/// write refused, a static half said so, a component the entity lacks
/// missing.
impl Refusal for crate::world::writes::WriteError {
    fn status(&self) -> Status {
        match self {
            Self::Unavailable => Status::UNAVAILABLE,
            Self::NoEntity => Status::BAD_ARGS,
            Self::Refused => Status::REFUSED,
            Self::StaticHalf => Status::STATIC_HALF,
            Self::Absent => Status::MISSING,
            Self::World(error) => error.status(),
        }
    }
}

/// No labels or string table is unavailable, a dead entity missing; the
/// game's no refused.
impl Refusal for crate::world::label::LabelError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoStrings => Status::UNAVAILABLE,
            Self::Dead => Status::MISSING,
            Self::Refused => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// A mod without a sizer is refused.
impl Refusal for crate::world::altars::NoSizerError {
    fn status(&self) -> Status {
        Status::REFUSED
    }
}

/// No buffs are unavailable, a bad request a bad argument, a cleansed buff
/// refused.
impl Refusal for crate::world::buffs::BuffError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoRegistry => Status::UNAVAILABLE,
            Self::BadRequest => Status::BAD_ARGS,
            Self::Cleansed => Status::REFUSED,
            Self::World(error) => error.status(),
            Self::Write(error) => error.status(),
        }
    }
}

/// No settings or world is unavailable; bad bytes a bad argument; the rest the
/// fuse's or the world's own answer.
impl Refusal for crate::world::game_settings::GameSettingsError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoSettings | Self::NoWorldKey => Status::UNAVAILABLE,
            Self::BadSize | Self::BadLength => Status::BAD_ARGS,
            Self::Fuse(error) => error.status(),
            Self::World(error) => error.status(),
        }
    }
}

/// A scene still reading is not loaded; every other reason is
/// unavailable, or the world's own answer.
impl Refusal for crate::world::scene::SceneError {
    fn status(&self) -> Status {
        match self {
            Self::Reading => Status::NOT_LOADED,
            Self::Unread
            | Self::NoScene
            | Self::Off
            | Self::Unreadable
            | Self::NotClient
            | Self::NoTypes => Status::UNAVAILABLE,
            Self::World(error) => error.status(),
        }
    }
}

/// A late request is too late, bad limits or caps bad arguments, a build
/// without the raise unavailable.
impl Refusal for crate::world::bases::BasesError {
    fn status(&self) -> Status {
        match self {
            Self::TooLate => Status::TOO_LATE,
            Self::BadLimits | Self::BelowGame => Status::BAD_ARGS,
            Self::Unavailable => Status::UNAVAILABLE,
        }
    }
}

/// Off a dedicated server or without the handler unavailable; a game
/// not running or a stop that did not start refused.
impl Refusal for crate::base::stop::StopError {
    fn status(&self) -> Status {
        match self {
            Self::NotServer | Self::Unread => Status::UNAVAILABLE,
            Self::NotRunning | Self::NoThread => Status::REFUSED,
        }
    }
}

/// An unknown event is a bad argument; another mod's refused.
impl Refusal for crate::base::services::SendEventError {
    fn status(&self) -> Status {
        match self {
            Self::Unknown => Status::BAD_ARGS,
            Self::NotOwner => Status::REFUSED,
        }
    }
}

/// An unknown point is a bad argument; another mod's refused.
impl Refusal for crate::base::points::RunPointError {
    fn status(&self) -> Status {
        match self {
            Self::Unknown => Status::BAD_ARGS,
            Self::NotOwner => Status::REFUSED,
        }
    }
}

/// A client without notices is unavailable.
impl Refusal for crate::base::notices::NoticeError {
    fn status(&self) -> Status {
        match self {
            Self::NoClient => Status::UNAVAILABLE,
            Self::Net(error) => error.status(),
            Self::World(error) => error.status(),
        }
    }
}

/// No link is missing, a full one busy, a bad payload a bad argument.
impl Refusal for crate::base::carrier::CarrierError {
    fn status(&self) -> Status {
        match self {
            Self::NoLink => Status::MISSING,
            Self::Full => Status::BUSY,
            Self::BadPayload => Status::BAD_ARGS,
        }
    }
}

/// No peer is missing; a channel not the mod's or a long message a bad
/// argument; a client sending to clients the wrong thread, as the ABI
/// states; a spent rate busy; a message not carried refused.
impl Refusal for crate::base::net::NetError {
    fn status(&self) -> Status {
        match self {
            Self::NoPeer => Status::MISSING,
            Self::BadChannel | Self::TooLong => Status::BAD_ARGS,
            Self::NotServing => Status::WRONG_THREAD,
            Self::Throttled => Status::BUSY,
            Self::NotCarried => Status::REFUSED,
            Self::Carrier(error) => error.status(),
        }
    }
}

/// Calls not sealed yet are unavailable, a call no mod serves missing; a bad id, caller or null
/// argument a bad argument; an off server refused.
impl Refusal for crate::base::calls::RunCallError {
    fn status(&self) -> Status {
        match self {
            Self::Unsealed => Status::UNAVAILABLE,
            Self::NoServer => Status::MISSING,
            Self::Unknown | Self::NotCaller | Self::NullArgs => Status::BAD_ARGS,
            Self::ServerOff => Status::REFUSED,
        }
    }
}

/// No saves or world is unavailable; a server that will not save, or a save
/// that does not read, refused.
impl Refusal for crate::base::saves::SaveError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoWorld => Status::UNAVAILABLE,
            Self::Blocked | Self::NotAllowed | Self::Unreadable => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// No saves, or holds that are off, are unavailable.
impl Refusal for crate::base::saves::HoldSavesError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::Off => Status::UNAVAILABLE,
        }
    }
}

/// No events, world or teleport are unavailable; a type mods may not
/// raise, the engine's no, a notifier past the step's bound or an entity it
/// does not move refused; bad bytes or entity 0 bad arguments.
impl Refusal for crate::base::emits::EmitError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoWorld | Self::Unavailable => Status::UNAVAILABLE,
            Self::NotAllowed | Self::Refused | Self::TooMany | Self::NotMovable => Status::REFUSED,
            Self::BadSize | Self::NoEntity => Status::BAD_ARGS,
            Self::World(error) => error.status(),
        }
    }
}

/// Where no windows are carried, unavailable; a bad change or window a bad
/// argument; a mod without actions refused; a full queue busy.
impl Refusal for crate::ui::windows::ShowWindowError {
    fn status(&self) -> Status {
        match self {
            Self::NotServing => Status::UNAVAILABLE,
            Self::BadChange | Self::BadWindow => Status::BAD_ARGS,
            Self::NoActions => Status::REFUSED,
            Self::Full => Status::BUSY,
        }
    }
}

/// Markers that are off are unavailable; too many a bad argument.
impl Refusal for crate::ui::interface::MarkersError {
    fn status(&self) -> Status {
        match self {
            Self::Off => Status::UNAVAILABLE,
            Self::TooMany => Status::BAD_ARGS,
        }
    }
}

/// Off a client unavailable; a waiting dialog from the interface thread
/// the wrong thread; a malformed one a bad argument; a full queue busy.
impl Refusal for crate::ui::interface::DialogError {
    fn status(&self) -> Status {
        match self {
            Self::NotClient => Status::UNAVAILABLE,
            Self::WrongThread => Status::WRONG_THREAD,
            Self::Malformed => Status::BAD_ARGS,
            Self::Full(_) => Status::BUSY,
        }
    }
}

/// A widget out of order or the HUD's or a tooltip's no refused; an unknown
/// one or an unknown item missing; no engine or nothing to show unavailable;
/// a full tooltip busy; outside its callback the wrong thread; a malformed
/// one a bad argument.
impl Refusal for crate::ui::widget_checks::WidgetError {
    fn status(&self) -> Status {
        match self {
            Self::OutOfOrder | Self::HudRefused | Self::TooltipRefused => Status::REFUSED,
            Self::NoEngine | Self::Unavailable => Status::UNAVAILABLE,
            Self::Unknown | Self::NoItem => Status::MISSING,
            Self::TooltipFull => Status::BUSY,
            Self::WrongThread => Status::WRONG_THREAD,
            Self::Malformed => Status::BAD_ARGS,
        }
    }
}

/// A reflection lookup by name: ambiguous for more than one match, else
/// missing on this build.
impl Refusal for enshwick_gamedata::reflect::LookupError {
    fn status(&self) -> Status {
        match self {
            Self::Ambiguous(_) => Status::AMBIGUOUS,
            _ => Status::MISSING,
        }
    }
}

/// Missing while nothing usable runs, refused when what runs does not fit
/// or is not exported; the mod reads the reason beside the status.
impl Refusal for crate::base::services::FindServiceError {
    fn status(&self) -> Status {
        match self {
            Self::Unpublished | Self::PublisherOff { .. } => Status::MISSING,
            Self::OtherMajors { .. } | Self::Layout { .. } | Self::Unexported { .. } => {
                Status::REFUSED
            }
        }
    }
}

/// A protected player is refused, no world unavailable; a file not written is the
/// calling mod's warning.
impl Refusal for crate::actions::chat_mute::SetMuteError {
    fn status(&self) -> Status {
        match self {
            Self::NoWorld => Status::UNAVAILABLE,
            Self::Protected | Self::Write(_) => Status::REFUSED,
        }
    }

    fn policy(&self) -> Policy {
        match self {
            Self::Write(_) => Policy::Warn,
            Self::Protected | Self::NoWorld => Policy::Quiet,
        }
    }
}

#[cfg(test)]
mod tests;
