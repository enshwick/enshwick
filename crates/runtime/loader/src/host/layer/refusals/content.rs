//! The status and policy of the content domain's errors.

use enshwick_core::abi::Status;

use super::{Policy, Refusal};

/// A resource another mod serves from a different base is refused.
impl Refusal for crate::content::served::ServeResourceError {
    fn status(&self) -> Status {
        match self {
            Self::Register(error) => error.status(),
            Self::OtherBase => Status::REFUSED,
        }
    }
}

/// A material the game lacks or a change it does not know is a bad
/// argument, a GUID twice or no free id left refused, each logged as the
/// mod's warning with its reason.
impl Refusal for crate::content::terrain_materials::TerrainMaterialError {
    fn status(&self) -> Status {
        match self {
            Self::TooLate => Status::TOO_LATE,
            Self::Unread => Status::UNAVAILABLE,
            Self::NotVanilla(_) | Self::BadChanges => Status::BAD_ARGS,
            Self::NotBoth | Self::Taken | Self::Full(_) => Status::REFUSED,
        }
    }

    fn policy(&self) -> Policy {
        match self {
            Self::TooLate => Policy::Quiet,
            Self::NotBoth => Policy::TurnOff,
            _ => Policy::Warn,
        }
    }
}

/// A request the loader does not know or a vanilla type the game lacks is a
/// bad argument, a mod clients do not run or an id taken refused, each
/// logged as the mod's warning with its reason; the mod stays on.
impl Refusal for crate::content::buff_types::BuffTypeError {
    fn status(&self) -> Status {
        match self {
            Self::TooLate => Status::TOO_LATE,
            Self::BadRequest => Status::BAD_ARGS,
            Self::NotBoth => Status::REFUSED,
            Self::Refused(refusal) => refusal.status(),
        }
    }

    fn policy(&self) -> Policy {
        match self {
            Self::TooLate => Policy::Quiet,
            _ => Policy::Warn,
        }
    }
}

/// Every refused rename is refused.
impl Refusal for crate::content::content_renames::RenameError {
    fn status(&self) -> Status {
        Status::REFUSED
    }
}

/// A bad kind or name is a bad argument; no rules on this build
/// unavailable.
impl Refusal for crate::content::recipes::RecipeRuleError {
    fn status(&self) -> Status {
        match self {
            Self::BadKind | Self::BadName => Status::BAD_ARGS,
            Self::Unavailable => Status::UNAVAILABLE,
        }
    }
}

/// Bad bytes are a bad argument; content the archive lacks missing; content
/// past the loader's limit refused, for the rest of the process.
impl Refusal for crate::content::textures::TextureError {
    fn status(&self) -> Status {
        match self {
            Self::BadData => Status::BAD_ARGS,
            Self::NotInArchive => Status::MISSING,
            Self::Full => Status::REFUSED,
        }
    }
}

/// A bad rate, factor or one undeclared is a bad argument; a rate clients
/// count from a mod that does not join refused; no rate unavailable; a late one
/// too late.
impl Refusal for crate::content::rates::RateError {
    fn status(&self) -> Status {
        match self {
            Self::BadRate | Self::BadFactor | Self::Undeclared => Status::BAD_ARGS,
            Self::NotJoined => Status::REFUSED,
            Self::Unavailable => Status::UNAVAILABLE,
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

/// The status [`BuildResourceError::status`](crate::content::served::BuildResourceError::status)
/// gives, which the fake host answers as well.
impl Refusal for crate::content::served::BuildResourceError {
    fn status(&self) -> Status {
        Self::status(*self)
    }
}
