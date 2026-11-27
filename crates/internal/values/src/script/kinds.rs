//! The kind a script reads from an error, named by the error's own type.

use std::io;

/// The kind a script reads from an error a `#[script]` method returns, as
/// [`Error::kind`](super::Error::kind); a method's error type must have one.
///
/// Derived beside `thiserror::Error`: an enum's kind is the name of the
/// variant, a struct's the name of its type. An [`io::Error`]'s kind is the
/// name of its [`io::ErrorKind`], `Other` for one the toolchain has not
/// stabilised.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::script::ErrorKind;
///
/// #[derive(Debug, ErrorKind)]
/// enum PlantError {
///     NoSeed,
///     TooCold(f32),
/// }
///
/// assert_eq!(PlantError::TooCold(-3.0).kind(), "TooCold");
/// assert_eq!(PlantError::NoSeed.kind(), "NoSeed");
/// let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
/// assert_eq!(ErrorKind::kind(&missing), "NotFound");
/// ```
pub trait ErrorKind {
    /// The name a script compares, such as `WrongThread`.
    fn kind(&self) -> &'static str;
}

/// The [`Error`](super::Error) a `#[script]` method's error becomes: a
/// script error as it is, with its own kind, any other through its
/// [`ErrorKind`].
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` names no kind for scripts",
    label = "a `#[script]` method's error",
    note = "derive `script::ErrorKind` beside `thiserror::Error`, or return a `script::Error`"
)]
pub trait Failure {
    /// The script error `self` is.
    fn into_error(self) -> super::Error;
}

#[diagnostic::do_not_recommend]
impl<E: core::fmt::Display + ErrorKind> Failure for E {
    fn into_error(self) -> super::Error {
        super::Error::failed(&self)
    }
}

#[doc(hidden)]
impl Failure for super::Error {
    fn into_error(self) -> super::Error {
        self
    }
}

impl ErrorKind for crate::Off {
    fn kind(&self) -> &'static str {
        "Off"
    }
}

/// The names of the stable [`io::ErrorKind`]s; the compiler checks each
/// exists, and a test that every kind the system gives is listed.
macro_rules! io_kinds {
    ($kind:expr, $($name:ident),* $(,)?) => {
        match $kind {
            $(io::ErrorKind::$name => stringify!($name),)*
            _ => "Other",
        }
    };
}

impl ErrorKind for io::Error {
    fn kind(&self) -> &'static str {
        io_kinds!(
            io::Error::kind(self),
            NotFound,
            PermissionDenied,
            ConnectionRefused,
            ConnectionReset,
            HostUnreachable,
            NetworkUnreachable,
            ConnectionAborted,
            NotConnected,
            AddrInUse,
            AddrNotAvailable,
            NetworkDown,
            BrokenPipe,
            AlreadyExists,
            WouldBlock,
            NotADirectory,
            IsADirectory,
            DirectoryNotEmpty,
            ReadOnlyFilesystem,
            StaleNetworkFileHandle,
            InvalidInput,
            InvalidData,
            TimedOut,
            WriteZero,
            StorageFull,
            NotSeekable,
            QuotaExceeded,
            FileTooLarge,
            ResourceBusy,
            ExecutableFileBusy,
            Deadlock,
            CrossesDevices,
            TooManyLinks,
            InvalidFilename,
            ArgumentListTooLong,
            Interrupted,
            Unsupported,
            UnexpectedEof,
            OutOfMemory,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_io_error_answers_the_name_of_its_io_kind() {
        let kind = |io: io::ErrorKind| ErrorKind::kind(&io::Error::from(io));
        assert_eq!(kind(io::ErrorKind::NotFound), "NotFound");
        assert_eq!(kind(io::ErrorKind::StorageFull), "StorageFull");
        assert_eq!(kind(io::ErrorKind::Other), "Other");
        assert_eq!(ErrorKind::kind(&io::Error::other("disk on fire")), "Other");
    }

    #[derive(Debug, crate::script::ErrorKind)]
    #[expect(dead_code, reason = "the fields only give the variants their shapes")]
    enum PlantError {
        NoSeed,
        TooCold(f32),
        Blocked { by: u32 },
    }

    #[derive(Debug, crate::script::ErrorKind)]
    struct Refused<T>(T);

    #[derive(Debug, crate::script::ErrorKind)]
    enum r#Raw {
        r#Type,
        r#Match,
    }

    #[derive(Debug, crate::script::ErrorKind)]
    enum NeverFails {}

    // An enum without variants derives a kind that compiles.
    const _: fn(&NeverFails) -> &'static str = NeverFails::kind;

    #[test]
    fn a_derived_kind_is_the_variant_name_whatever_its_fields() {
        assert_eq!(PlantError::NoSeed.kind(), "NoSeed");
        assert_eq!(PlantError::TooCold(-3.0).kind(), "TooCold");
        assert_eq!(PlantError::Blocked { by: 7 }.kind(), "Blocked");
        assert_eq!(Refused(1_u8).kind(), "Refused");
    }

    #[test]
    fn a_raw_name_answers_without_its_prefix() {
        assert_eq!(r#Raw::r#Type.kind(), "Type");
        assert_eq!(r#Raw::r#Match.kind(), "Match");
    }

    /// An error whose hand-written kind is empty.
    struct Blank;

    impl core::fmt::Display for Blank {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("blank")
        }
    }

    impl ErrorKind for Blank {
        fn kind(&self) -> &'static str {
            ""
        }
    }

    #[test]
    fn an_empty_kind_reads_as_bad_args() {
        assert_eq!(crate::script::Error::failed(&Blank).kind(), "BadArgs");
    }

    #[test]
    fn a_script_error_keeps_its_kind_and_another_error_gives_its_own() {
        let gone = crate::script::Error::with_kind(crate::script::Error::GONE, "the call ended");
        assert_eq!(gone.into_error().kind(), "Gone");
        assert_eq!(crate::Off::new("no glow").into_error().kind(), "Off");
    }

    /// The io kinds the pinned toolchain has not stabilised, which the
    /// standard library still gives: an `io::Error` of one reads `Other`.
    const UNSTABLE_IO_KINDS: [&str; 5] = [
        "Uncategorized",
        "InProgress",
        "FilesystemLoop",
        "TooManyOpenFiles",
        "InputOutputError",
    ];

    #[test]
    fn every_io_kind_the_system_gives_answers_its_name_or_other_while_unstable() {
        for code in 0..20_000 {
            let error = io::Error::from_raw_os_error(code);
            let name = format!("{:?}", error.kind());
            let expected = if UNSTABLE_IO_KINDS.contains(&name.as_str()) {
                "Other"
            } else {
                name.as_str()
            };
            assert_eq!(ErrorKind::kind(&error), expected, "os error {code}");
        }
    }

    #[test]
    fn an_off_answers_off() {
        assert_eq!(crate::Off::new("no glow").kind(), "Off");
    }
}
