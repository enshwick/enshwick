//! The exports and the manifest a mod DLL gives the loader.

mod unwind;

/// Writes `$schema`, the schema text of a point, event or call or the head
/// of `declare_mod!`, into the binary's `.enshpt` section, where the
/// manifest and `cargo enshwick` find it.
#[doc(hidden)]
#[macro_export]
macro_rules! __schema_entry {
    ($schema:expr) => {
        const _: () = {
            #[used]
            #[cfg_attr(windows, unsafe(link_section = ".enshpt$m"))]
            #[cfg_attr(
                target_vendor = "apple",
                unsafe(link_section = "__DATA,__enshpt,regular,no_dead_strip")
            )]
            #[cfg_attr(
                not(any(windows, target_vendor = "apple")),
                unsafe(link_section = "enshpt")
            )]
            static ENTRY: [u8; $schema.len() + 1] = $crate::__private::entry($schema);
        };
    };
}

/// Logs a line at `error` level under the mod's id; formats like `format!`.
///
/// # Examples
///
/// ```
/// enshwick::error!("solver failed: {}", "no zones");
/// ```
#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => { $crate::__log!($crate::abi::Level::ERROR, $($arg)+) };
}

/// Logs a line at `warn` level under the mod's id; formats like `format!`.
///
/// # Examples
///
/// ```
/// enshwick::warn!("unexpected bed {}", 1210);
/// ```
#[macro_export]
macro_rules! warn {
    ($($arg:tt)+) => { $crate::__log!($crate::abi::Level::WARN, $($arg)+) };
}

/// Logs a line at `info` level under the mod's id; formats like `format!`.
///
/// For milestones, never per frame.
///
/// # Examples
///
/// ```
/// enshwick::info!("{} altars merged", 3);
/// ```
#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => { $crate::__log!($crate::abi::Level::INFO, $($arg)+) };
}

/// Logs a line at `debug` level under the mod's id; formats like `format!`.
///
/// Kept only when `enshwick.toml` in the game's folder asks for `debug`; otherwise the
/// arguments are neither evaluated nor formatted.
///
/// # Examples
///
/// ```
/// enshwick::debug!("bed {} checked", 1210);
/// ```
#[macro_export]
macro_rules! debug {
    ($($arg:tt)+) => { $crate::__log!($crate::abi::Level::DEBUG, $($arg)+) };
}

/// Logs a line at `info` level the first time this call site runs only.
///
/// # Examples
///
/// ```
/// for altar in 0..3 {
///     enshwick::info_once!("first altar seen: {altar}");
/// }
/// ```
#[macro_export]
macro_rules! info_once {
    ($($arg:tt)+) => { $crate::__once!($crate::abi::Level::INFO, $($arg)+) };
}

/// Logs a line at `warn` level the first time this call site runs only.
///
/// # Examples
///
/// ```
/// for _ in 0..3 {
///     enshwick::warn_once!("an altar without a zone");
/// }
/// ```
#[macro_export]
macro_rules! warn_once {
    ($($arg:tt)+) => { $crate::__once!($crate::abi::Level::WARN, $($arg)+) };
}

/// Logs a line at `error` level the first time this call site runs only.
///
/// # Examples
///
/// ```
/// for _ in 0..3 {
///     enshwick::error_once!("the altar table could not be read");
/// }
/// ```
#[macro_export]
macro_rules! error_once {
    ($($arg:tt)+) => { $crate::__once!($crate::abi::Level::ERROR, $($arg)+) };
}

/// Logs a line at `info` level at most once per period from this call site,
/// noting how many it skipped since the last.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// for player in 0..100 {
///     enshwick::info_every!(Duration::from_secs(60), "player {player} rested");
/// }
/// ```
#[macro_export]
macro_rules! info_every {
    ($period:expr, $($arg:tt)+) => { $crate::__every!($crate::abi::Level::INFO, $period, $($arg)+) };
}

/// Logs a line at `warn` level at most once per period from this call site,
/// noting how many it skipped since the last.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// for bed in 0..100 {
///     enshwick::warn_every!(Duration::from_secs(60), "bed {bed} has no owner");
/// }
/// ```
#[macro_export]
macro_rules! warn_every {
    ($period:expr, $($arg:tt)+) => { $crate::__every!($crate::abi::Level::WARN, $period, $($arg)+) };
}

/// Logs a line at `error` level at most once per period from this call site,
/// noting how many it skipped since the last.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// enshwick::error_every!(Duration::from_secs(60), "the zone table is full");
/// ```
#[macro_export]
macro_rules! error_every {
    ($period:expr, $($arg:tt)+) => { $crate::__every!($crate::abi::Level::ERROR, $period, $($arg)+) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log {
    ($level:expr, $($arg:tt)+) => {
        if $crate::__private::enabled($level) {
            $crate::__private::log($level, ::std::format_args!($($arg)+));
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __once {
    ($level:expr, $($arg:tt)+) => {{
        static ONCE: $crate::__private::Once = $crate::__private::Once::new();
        if $crate::__private::enabled($level) && ONCE.first() {
            $crate::__private::log($level, ::std::format_args!($($arg)+));
        }
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __every {
    ($level:expr, $period:expr, $($arg:tt)+) => {{
        static EVERY: $crate::__private::Every = $crate::__private::Every::new();
        if $crate::__private::enabled($level)
            && let ::core::option::Option::Some(skipped) = EVERY.due($period)
        {
            match skipped {
                0 => $crate::__private::log($level, ::std::format_args!($($arg)+)),
                skipped => $crate::__private::log(
                    $level,
                    ::std::format_args!("{} ({skipped} more since the last)", ::std::format_args!($($arg)+)),
                ),
            }
        }
    }};
}
