//! The status of the actions domain's errors.

use enshwick_core::abi::Status;

use super::Refusal;

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

/// An account action: a build without the handler unavailable, account 0 a
/// bad argument, what the game would refuse refused.
impl Refusal for crate::actions::console::AccountActionError {
    fn status(&self) -> Status {
        match self {
            Self::Unread => Status::UNAVAILABLE,
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

/// Chat lines while chat is off on this build are unavailable.
impl Refusal for crate::actions::chat::ChatOffError {
    fn status(&self) -> Status {
        Status::UNAVAILABLE
    }
}

/// The local player's held mask, unread lately, is unavailable.
impl Refusal for crate::actions::local_held::HeldError {
    fn status(&self) -> Status {
        Status::UNAVAILABLE
    }
}

/// A client command not queued: unavailable off a client and on a build
/// without what it needs, a bad argument for a wrong spec, refused while
/// one of its kind waits.
impl Refusal for crate::actions::client_commands::CommandError {
    fn status(&self) -> Status {
        use crate::actions::client_commands::CommandError;
        match self {
            CommandError::NotClient | CommandError::Unavailable => Status::UNAVAILABLE,
            CommandError::BadArgs => Status::BAD_ARGS,
            CommandError::Waiting | CommandError::NotNow => Status::REFUSED,
        }
    }
}

/// A read or rebind of the game's key bindings refused: unavailable while
/// they do not read, missing for an action the build does not name, a bad
/// argument for a key a rebind does not take.
impl Refusal for crate::actions::game_bindings::BindingError {
    fn status(&self) -> Status {
        use crate::actions::game_bindings::BindingError;
        match self {
            BindingError::Unavailable => Status::UNAVAILABLE,
            BindingError::NoAction(_) => Status::MISSING,
            BindingError::BadKey(_) => Status::BAD_ARGS,
        }
    }
}
