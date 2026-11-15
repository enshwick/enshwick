//! The status and policy of every domain error a mod's call can meet: the
//! one place a domain error turns into a [`Status`], so domain code never
//! names one.

use core::fmt;

use enshwick_core::abi::Status;

use super::{Policy, Refusal};

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

/// Bounds unlike the first call's are a bad argument, warned of; a full
/// mask leaves the mod's prop without its bit, so the mod turns off.
impl Refusal for crate::base::ids::CraftingPropError {
    fn status(&self) -> Status {
        match self {
            Self::Bounds { .. } => Status::BAD_ARGS,
            Self::Full { .. } => Status::REFUSED,
        }
    }

    fn policy(&self) -> Policy {
        match self {
            Self::Bounds { .. } => Policy::Warn,
            Self::Full { .. } => Policy::TurnOff,
        }
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

/// Too late after init; missing where the build or the side lacks the
/// feature; refused where another registration holds the place.
impl Refusal for crate::engine::registry::JoinFeatureError {
    fn status(&self) -> Status {
        match self {
            Self::TooLate => Status::TOO_LATE,
            Self::Unavailable => Status::MISSING,
            Self::Taken => Status::REFUSED,
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

/// A resource another mod serves from a different base is refused.
impl Refusal for crate::content::served::ServeResourceError {
    fn status(&self) -> Status {
        match self {
            Self::Register(error) => error.status(),
            Self::OtherBase => Status::REFUSED,
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
            Self::OtherScope | Self::Full | Self::NotServer => Status::REFUSED,
            Self::NotKept => Status::NOT_KEPT,
            Self::WrongThread => Status::WRONG_THREAD,
            Self::NoValue | Self::Unopened => Status::MISSING,
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

/// Missing off a client or for a screen the build lacks; a bad edge or a
/// screen of play is a bad argument.
impl Refusal for crate::ui::interface::ExtendScreenError {
    fn status(&self) -> Status {
        match self {
            Self::Register(error) => error.status(),
            Self::NotClient | Self::UnknownScreen => Status::MISSING,
            Self::BadEdge | Self::ScreenOfPlay => Status::BAD_ARGS,
        }
    }
}

/// A mod's write of its own setting, answered as `HostApi::config_set`
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

/// A level or experience write: missing without the level step, refused
/// for a mod that did not ask, an entity no player has or a full queue.
impl Refusal for crate::knowledge::progress::WriteProgressError {
    fn status(&self) -> Status {
        match self {
            Self::BadKind => Status::BAD_ARGS,
            Self::Unavailable => Status::MISSING,
            Self::Register(error) => error.status(),
            Self::NotAsked | Self::NoPlayer | Self::Full => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// A stat or affix call: too late after init; missing while stats do not
/// run or for what is not defined; bad arguments for what does not read;
/// refused off the server, outside the mod's namespace, defined twice or
/// past an index; a key another mod holds turns the mod off.
impl Refusal for crate::inventory::stats::StatsError {
    fn status(&self) -> Status {
        match self {
            Self::TooLate => Status::TOO_LATE,
            Self::NotRunning | Self::Undefined | Self::NoAffix => Status::MISSING,
            Self::NoMod
            | Self::BadId
            | Self::BadDef
            | Self::BadStat
            | Self::BadModifier
            | Self::NothingToWatch => Status::BAD_ARGS,
            Self::NotOwn
            | Self::Taken(_)
            | Self::Defined
            | Self::Full
            | Self::NotServer
            | Self::NotPlayer => Status::REFUSED,
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

/// A knowledge read or change: missing where the store, its queue, the id
/// read or an answer is not there; a bad argument for a malformed change;
/// refused for a character without its record or save, or a full queue.
impl Refusal for crate::knowledge::store::KnowledgeError {
    fn status(&self) -> Status {
        match self {
            Self::Unread
            | Self::NoStore
            | Self::NoQueue
            | Self::NoWorld
            | Self::Unknown
            | Self::Unanswered => Status::MISSING,
            Self::BadChange => Status::BAD_ARGS,
            Self::PlayerOnly | Self::NoRecord | Self::NotLoaded | Self::Full => Status::REFUSED,
            Self::Fuse(error) => error.status(),
            Self::World(error) => error.status(),
        }
    }
}

/// A line the console did not run: an empty one a bad argument, off the
/// game thread the wrong thread, an unknown word missing.
impl Refusal for crate::actions::console::ConsoleError {
    fn status(&self) -> Status {
        match self {
            Self::Empty => Status::BAD_ARGS,
            Self::WrongThread => Status::WRONG_THREAD,
            Self::Unknown => Status::MISSING,
        }
    }
}

/// An account action: a build without the handler missing, account 0 a
/// bad argument, what the game would refuse refused.
impl Refusal for crate::actions::console::AccountActionError {
    fn status(&self) -> Status {
        match self {
            Self::Unread => Status::MISSING,
            Self::NoAccount => Status::BAD_ARGS,
            Self::Refused => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// A command line run for a remote caller: a bad line a bad argument, off
/// the game thread the wrong thread, an unknown word missing.
impl Refusal for crate::actions::commands::RunCommandError {
    fn status(&self) -> Status {
        match self {
            Self::BadLine => Status::BAD_ARGS,
            Self::WrongThread => Status::WRONG_THREAD,
            Self::Unknown => Status::MISSING,
        }
    }
}

/// Chat lines while chat is off on this build are missing.
impl Refusal for crate::actions::chat::ChatOffError {
    fn status(&self) -> Status {
        Status::MISSING
    }
}

/// The local player's held mask, unread lately, is missing.
impl Refusal for crate::actions::local_held::HeldError {
    fn status(&self) -> Status {
        Status::MISSING
    }
}

/// An item rolled anew: missing while affixes are not open, a bad argument
/// without an item, refused when no roll fits or the stats did not encode.
impl Refusal for crate::inventory::stats::RollAffixesError {
    fn status(&self) -> Status {
        match self {
            Self::Unopened => Status::MISSING,
            Self::NoItem => Status::BAD_ARGS,
            Self::NoRoll | Self::Encode => Status::REFUSED,
            Self::Kept(error) => error.status(),
            Self::World(error) => error.status(),
            Self::Item(error) => error.status(),
        }
    }
}

/// The world unreadable now: off the game thread or inside a step is the
/// wrong thread; an entity call without a server is unavailable; the rest,
/// no world or the entity gone, is missing.
impl Refusal for crate::engine::world::WorldError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoWorld | Self::Absent => Status::MISSING,
            Self::NoServer => Status::UNAVAILABLE,
            Self::WrongThread | Self::InStep => Status::WRONG_THREAD,
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

/// A write refused once its feature is off is unavailable; before its live
/// object exists, missing.
impl Refusal for crate::engine::fuse::FuseError {
    fn status(&self) -> Status {
        match self {
            Self::Off => Status::UNAVAILABLE,
            Self::Unmade => Status::MISSING,
        }
    }
}

/// A bad box or voxel is a bad argument; a box out of the world or not
/// loaded says so; the rest is missing, or the world's own answer.
impl Refusal for crate::world::voxels::VoxelError {
    fn status(&self) -> Status {
        match self {
            Self::BadBox | Self::BadVoxel => Status::BAD_ARGS,
            Self::Unread | Self::NoEditor => Status::MISSING,
            Self::World(error) => error.status(),
            Self::OutsideWorld => Status::OUTSIDE_WORLD,
            Self::NotLoaded => Status::NOT_LOADED,
        }
    }
}

/// A malformed edit or material is a bad argument; one out of the world
/// says so; the rest is missing, or the world's or the voxels' own answer.
impl Refusal for crate::world::terrain::TerrainError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoEditor => Status::MISSING,
            Self::BadEdit | Self::BadMaterial => Status::BAD_ARGS,
            Self::OutsideWorld => Status::OUTSIDE_WORLD,
            Self::World(error) => error.status(),
            Self::Voxels(error) => error.status(),
        }
    }
}

/// A bad box is a bad argument; the rest is missing, or the world's own
/// answer.
impl Refusal for crate::world::rights::RightsError {
    fn status(&self) -> Status {
        match self {
            Self::BadBox => Status::BAD_ARGS,
            Self::Unread | Self::NoZones => Status::MISSING,
            Self::World(error) => error.status(),
        }
    }
}

/// Where inventories do not run, missing; malformed operations, a bad
/// argument.
impl Refusal for crate::inventory::transactions::SubmitInventoryError {
    fn status(&self) -> Status {
        match self {
            Self::Unavailable => Status::MISSING,
            Self::BadOps => Status::BAD_ARGS,
        }
    }
}

/// An item's data unreadable for any reason is missing.
impl Refusal for crate::inventory::transactions::ItemInfoError {
    fn status(&self) -> Status {
        Status::MISSING
    }
}

/// A spawn the build or the world cannot take is missing, a malformed one a
/// bad argument, one the engine refuses refused.
impl Refusal for crate::inventory::spawns::SpawnError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoWorld => Status::MISSING,
            Self::BadRequest => Status::BAD_ARGS,
            Self::Refused => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// A pickup that does not read is missing, a malformed drop a bad argument,
/// an item the game does not have refused, an item lookup the build, the
/// registry or the world lacks missing.
impl Refusal for crate::inventory::drops::DropError {
    fn status(&self) -> Status {
        match self {
            Self::Unread => Status::MISSING,
            Self::BadRequest => Status::BAD_ARGS,
            Self::Item(crate::inventory::transactions::ItemInfoError::NoItem) => Status::REFUSED,
            Self::Item(error) => error.status(),
            Self::World(error) => error.status(),
            Self::Spawn(error) => error.status(),
        }
    }
}

/// No entity list is missing; no entity call unavailable; entity 0 a bad
/// argument; a player's character, a vanilla entity or the engine's
/// refusal refused.
impl Refusal for crate::world::entities::EntityError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoWorld => Status::MISSING,
            Self::Unavailable => Status::UNAVAILABLE,
            Self::NoEntity => Status::BAD_ARGS,
            Self::Player | Self::NotSpawned | Self::Refused => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// A hold the hooks cannot make is missing.
impl Refusal for crate::world::prop_holds::HoldError {
    fn status(&self) -> Status {
        Status::MISSING
    }
}

/// A transaction's reason, the status a call about it and its record's
/// reason both answer: malformed changes are bad arguments; a prop the
/// game would not hand back, no room, an undo it cannot make, rights or a
/// world that changed are refused; the rest says what it is, or is the
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
            Self::Unavailable | Self::Gone | Self::BlueprintChanged => Status::MISSING,
            Self::OutsideWorld => Status::OUTSIDE_WORLD,
            Self::NotLoaded => Status::NOT_LOADED,
            Self::Full | Self::NotUndoable | Self::NoRights | Self::HoldsState | Self::Changed => {
                Status::REFUSED
            }
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

/// No weather is missing, a bad one a bad argument; the rest is the fuse's
/// or the world's own answer.
impl Refusal for crate::world::weather::WeatherError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoSystem => Status::MISSING,
            Self::BadWeather => Status::BAD_ARGS,
            Self::Fuse(error) => error.status(),
            Self::World(error) => error.status(),
        }
    }
}

/// No clock or speed link is missing, a bad factor a bad argument, a mod
/// that did not ask refused.
impl Refusal for crate::world::daytime::DaytimeError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoClock | Self::SpeedOff => Status::MISSING,
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

/// A mod that did not open the fog is refused; no copy is missing.
impl Refusal for crate::world::fog::FogReadError {
    fn status(&self) -> Status {
        match self {
            Self::Unopened => Status::REFUSED,
            Self::NoCopy => Status::MISSING,
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

/// No labels, entity or string table is missing; the game's no refused.
impl Refusal for crate::world::label::LabelError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::Dead | Self::NoStrings => Status::MISSING,
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

/// No buffs are missing, a bad request a bad argument, a cleansed buff
/// refused.
impl Refusal for crate::world::buffs::BuffError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoRegistry => Status::MISSING,
            Self::BadRequest => Status::BAD_ARGS,
            Self::Cleansed => Status::REFUSED,
            Self::World(error) => error.status(),
            Self::Write(error) => error.status(),
        }
    }
}

/// No settings or world is missing; bad bytes a bad argument; the rest the
/// fuse's or the world's own answer.
impl Refusal for crate::world::game_settings::GameSettingsError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoSettings | Self::NoWorldKey => Status::MISSING,
            Self::BadSize | Self::BadLength => Status::BAD_ARGS,
            Self::Fuse(error) => error.status(),
            Self::World(error) => error.status(),
        }
    }
}

/// A scene still reading is not loaded; every other reason is missing, or
/// the world's own answer.
impl Refusal for crate::world::scene::SceneError {
    fn status(&self) -> Status {
        match self {
            Self::Reading => Status::NOT_LOADED,
            Self::Unread
            | Self::NoScene
            | Self::Off
            | Self::Unreadable
            | Self::NotClient
            | Self::NoTypes => Status::MISSING,
            Self::World(error) => error.status(),
        }
    }
}

/// A late request is too late, bad limits or caps bad arguments, a build
/// without the raise missing.
impl Refusal for crate::world::bases::BasesError {
    fn status(&self) -> Status {
        match self {
            Self::TooLate => Status::TOO_LATE,
            Self::BadLimits | Self::BelowGame => Status::BAD_ARGS,
            Self::Unavailable => Status::MISSING,
        }
    }
}

/// Off a dedicated server unavailable; without the handler missing; a game
/// not running or a stop that did not start refused.
impl Refusal for crate::base::stop::StopError {
    fn status(&self) -> Status {
        match self {
            Self::NotServer => Status::UNAVAILABLE,
            Self::Unread => Status::MISSING,
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

/// A client without notices is missing.
impl Refusal for crate::base::notices::NoticeError {
    fn status(&self) -> Status {
        match self {
            Self::NoClient => Status::MISSING,
            Self::Net(error) => error.status(),
            Self::World(error) => error.status(),
        }
    }
}

/// No link is missing, a full one refused, a bad payload a bad argument.
impl Refusal for crate::base::carrier::CarrierError {
    fn status(&self) -> Status {
        match self {
            Self::NoLink => Status::MISSING,
            Self::Full => Status::REFUSED,
            Self::BadPayload => Status::BAD_ARGS,
        }
    }
}

/// No peer is missing; a channel not the mod's or a long message a bad
/// argument; a client sending to clients the wrong thread, as the ABI
/// states; a spent rate or a message not carried refused.
impl Refusal for crate::base::net::NetError {
    fn status(&self) -> Status {
        match self {
            Self::NoPeer => Status::MISSING,
            Self::BadChannel | Self::TooLong => Status::BAD_ARGS,
            Self::NotServing => Status::WRONG_THREAD,
            Self::Throttled | Self::NotCarried => Status::REFUSED,
            Self::Carrier(error) => error.status(),
        }
    }
}

/// Calls not sealed or not served are missing; a bad id, caller or null
/// argument a bad argument; an off server refused.
impl Refusal for crate::base::calls::RunCallError {
    fn status(&self) -> Status {
        match self {
            Self::Unsealed | Self::NoServer => Status::MISSING,
            Self::Unknown | Self::NotCaller | Self::NullArgs => Status::BAD_ARGS,
            Self::ServerOff => Status::REFUSED,
        }
    }
}

/// No saves or world is missing; a server that will not save, or a save
/// that does not read, refused.
impl Refusal for crate::base::saves::SaveError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoWorld => Status::MISSING,
            Self::Blocked | Self::NotAllowed | Self::Unreadable => Status::REFUSED,
            Self::World(error) => error.status(),
        }
    }
}

/// No saves are missing; holds that are off unavailable.
impl Refusal for crate::base::saves::HoldSavesError {
    fn status(&self) -> Status {
        match self {
            Self::Unread => Status::MISSING,
            Self::Off => Status::UNAVAILABLE,
        }
    }
}

/// No events or world are missing; a type mods may not raise, the engine's
/// no or an entity it does not move refused; bad bytes or entity 0 bad
/// arguments; no teleport unavailable.
impl Refusal for crate::base::emits::EmitError {
    fn status(&self) -> Status {
        match self {
            Self::Unread | Self::NoWorld => Status::MISSING,
            Self::NotAllowed | Self::Refused | Self::NotMovable => Status::REFUSED,
            Self::BadSize | Self::NoEntity => Status::BAD_ARGS,
            Self::Unavailable => Status::UNAVAILABLE,
            Self::World(error) => error.status(),
        }
    }
}

/// Every refused rename is refused.
impl Refusal for crate::content::content_renames::RenameError {
    fn status(&self) -> Status {
        Status::REFUSED
    }
}

/// A bad kind or name is a bad argument; no rules on this build missing.
impl Refusal for crate::content::recipes::RecipeRuleError {
    fn status(&self) -> Status {
        match self {
            Self::BadKind | Self::BadName => Status::BAD_ARGS,
            Self::Unavailable => Status::MISSING,
        }
    }
}

/// Bad bytes are a bad argument; content the archive lacks missing.
impl Refusal for crate::content::textures::TextureError {
    fn status(&self) -> Status {
        match self {
            Self::BadData => Status::BAD_ARGS,
            Self::NotInArchive => Status::MISSING,
        }
    }
}

/// A bad rate, factor or one undeclared is a bad argument; a rate clients
/// count from a mod that does not join refused; no rate missing; a late one
/// too late.
impl Refusal for crate::content::rates::RateError {
    fn status(&self) -> Status {
        match self {
            Self::BadRate | Self::BadFactor | Self::Undeclared => Status::BAD_ARGS,
            Self::NotJoined => Status::REFUSED,
            Self::Unavailable => Status::MISSING,
            Self::TooLate => Status::TOO_LATE,
        }
    }
}

/// Late or needing a restart is too late; too many changes, an unknown one
/// or a bad value bad arguments.
impl Refusal for crate::content::data_changes::DataChangeError {
    fn status(&self) -> Status {
        match self {
            Self::TooLate | Self::NeedsRestart => Status::TOO_LATE,
            Self::TooMany | Self::Unknown | Self::BadValue => Status::BAD_ARGS,
        }
    }
}

/// A key the mod does not serve is a bad argument; one building on this
/// thread says so; no base, build or archive entry missing; a size changed
/// or a read before the seal refused.
impl Refusal for crate::content::served::ServedError {
    fn status(&self) -> Status {
        match self {
            Self::NotServed => Status::BAD_ARGS,
            Self::Building => Status::BUILDING,
            Self::NoBase | Self::NotBuilt | Self::NotInArchive => Status::MISSING,
            Self::SizeChanged | Self::Unsealed => Status::REFUSED,
        }
    }
}

/// Where no windows are carried, missing; a bad change or window a bad
/// argument; a mod without actions or a full queue refused.
impl Refusal for crate::ui::windows::ShowWindowError {
    fn status(&self) -> Status {
        match self {
            Self::NotServing => Status::MISSING,
            Self::BadChange | Self::BadWindow => Status::BAD_ARGS,
            Self::NoActions | Self::Full => Status::REFUSED,
        }
    }
}

/// Markers that are off are missing; too many a bad argument.
impl Refusal for crate::ui::interface::MarkersError {
    fn status(&self) -> Status {
        match self {
            Self::Off => Status::MISSING,
            Self::TooMany => Status::BAD_ARGS,
        }
    }
}

/// Off a client missing; a waiting dialog from the interface thread the
/// wrong thread; a malformed one a bad argument; a full queue refused.
impl Refusal for crate::ui::interface::DialogError {
    fn status(&self) -> Status {
        match self {
            Self::NotClient => Status::MISSING,
            Self::WrongThread => Status::WRONG_THREAD,
            Self::Malformed => Status::BAD_ARGS,
            Self::Full(_) => Status::REFUSED,
        }
    }
}

/// A widget out of order or the HUD's no refused; an unknown one, no engine
/// or nothing to show missing; outside its callback the wrong thread; a
/// malformed one a bad argument.
impl Refusal for crate::ui::widget_checks::WidgetError {
    fn status(&self) -> Status {
        match self {
            Self::OutOfOrder | Self::HudRefused => Status::REFUSED,
            Self::Unknown | Self::NoEngine | Self::Unavailable => Status::MISSING,
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

/// A protected player or no world says all; a file not written is the
/// calling mod's warning.
impl Refusal for crate::actions::chat_mute::SetMuteError {
    fn status(&self) -> Status {
        match self {
            Self::NoWorld => Status::MISSING,
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
