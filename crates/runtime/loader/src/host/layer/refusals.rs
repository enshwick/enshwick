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
            Self::World(status) => *status,
        }
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
            Self::World(status) => *status,
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
