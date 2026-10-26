//! The exports and the manifest a mod DLL gives the loader.

/// Builds a mod's static [`Manifest`](crate::abi::Manifest).
///
/// `id` is `[a-z0-9-]+` and names the DLL and the settings file; `side` is
/// `SERVER`, `CLIENT` or `BOTH`. Optional, in this order:
///
/// - `version: "x.y.z"`, the crate's version when omitted;
/// - `name: "..."`, what players read (the id when omitted),
///   `description: "..."`, `authors: ["...", ...]`, `license: "..."` and
///   `homepage: "https://..."`, each taken from the crate's `Cargo.toml`
///   (`description`, `authors`, `license`, `homepage` or else `repository`)
///   when omitted;
/// - `join: true`: the mod counts in the join check;
/// - `reload: true`: the mod may be reloaded while a development server runs
///   (see [`ManifestFlags::RELOAD`](crate::abi::ManifestFlags::RELOAD));
/// - `requires: ["id" => "range", ...]`: mods that must load first, in a
///   Cargo-style version range (`""` for any);
/// - `after: ["id", ...]`: mods that load first when present;
/// - `recommends: ["id" => "range", ...]`: mods that load first when present
///   and that the loader notes as missing otherwise;
/// - `breaks: ["id" => "range", ...]`: mods this one does not work beside in
///   that range; the loader skips this mod when one of them loads;
/// - `replaces: ["id", ...]`: old ids of this mod, from before a rename;
///   while this mod is present the loader never starts theirs;
/// - `builds: [{ side, guid, age, symbols: ["id" => rva], constants: ["id" => value] }]`,
///   symbols and constants the mod pins itself for those game builds.
///
/// The loader names the mod and its version in every message about it: a
/// missing or mismatched dependency, or a server with a different mod set.
///
/// # Examples
///
/// ```
/// let manifest = enshwick_loader_api::manifest!(
///     id: "survivors-per-altar",
///     side: BOTH,
///     name: "Survivors per Altar",
///     authors: ["themaiby"],
///     join: true,
///     requires: ["flame-altar-zones" => ">=0.1, <1.0"],
///     breaks: ["old-survivors" => "<0.3"],
///     replaces: ["survivors"],
///     builds: [{
///         side: SERVER,
///         guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
///         age: 387,
///         symbols: ["survivors.spawn_return" => 0x4_05cb],
///         constants: ["survivors.bed_id" => 0x48],
///     }],
/// );
/// assert_eq!(manifest.dependency_count, 3);
/// assert_eq!(manifest.build_count, 1);
/// assert_eq!(manifest.author_count, 1);
/// // SAFETY: static strings.
/// assert_eq!(unsafe { manifest.name.as_str() }, Some("Survivors per Altar"));
/// // SAFETY: as above; the license comes from this crate's Cargo.toml.
/// assert_eq!(unsafe { manifest.license.as_str() }, Some("MIT"));
///
/// let pinned = enshwick_loader_api::manifest!(id: "safes", side: SERVER, version: "0.3.0");
/// // SAFETY: a static string.
/// assert_eq!(unsafe { pinned.version.as_str() }, Some("0.3.0"));
/// ```
#[macro_export]
macro_rules! manifest {
    (
        id: $id:literal,
        side: $side:ident
        $(, version: $version:literal)?
        $(, name: $name:literal)?
        $(, description: $description:literal)?
        $(, authors: [$($author:literal),* $(,)?])?
        $(, license: $license:literal)?
        $(, homepage: $homepage:literal)?
        $(, join: $join:expr)?
        $(, reload: $reload:expr)?
        $(, requires: [$($req:literal => $range:literal),* $(,)?])?
        $(, after: [$($after:literal),* $(,)?])?
        $(, recommends: [$($rec:literal => $rec_range:literal),* $(,)?])?
        $(, breaks: [$($breaks:literal => $breaks_range:literal),* $(,)?])?
        $(, replaces: [$($replaces:literal),* $(,)?])?
        $(, builds: [$({
            side: $bside:ident,
            guid: $guid:literal,
            age: $age:literal,
            symbols: [$($sym:literal => $rva:expr),* $(,)?]
            $(, constants: [$($constant:literal => $value:expr),* $(,)?])? $(,)?
        }),* $(,)?])?
        $(,)?
    ) => {{
        const _: () = ::core::assert!($crate::abi::is_valid_id($id), "a mod id is [a-z0-9-]+");
        $($(const _: () = ::core::assert!($crate::abi::is_valid_id($replaces), "a replaced mod id is [a-z0-9-]+");)*)?
        static DEPENDENCIES: &[$crate::abi::Dependency] = &[
            $($($crate::abi::Dependency {
                id: $crate::abi::Str::new($req),
                range: $crate::abi::Str::new($range),
                relation: $crate::abi::Relation::REQUIRES,
            },)*)?
            $($($crate::abi::Dependency {
                id: $crate::abi::Str::new($after),
                range: $crate::abi::Str::EMPTY,
                relation: $crate::abi::Relation::AFTER,
            },)*)?
            $($($crate::abi::Dependency {
                id: $crate::abi::Str::new($rec),
                range: $crate::abi::Str::new($rec_range),
                relation: $crate::abi::Relation::RECOMMENDS,
            },)*)?
            $($($crate::abi::Dependency {
                id: $crate::abi::Str::new($breaks),
                range: $crate::abi::Str::new($breaks_range),
                relation: $crate::abi::Relation::BREAKS,
            },)*)?
            $($($crate::abi::Dependency {
                id: $crate::abi::Str::new($replaces),
                range: $crate::abi::Str::EMPTY,
                relation: $crate::abi::Relation::REPLACES,
            },)*)?
        ];
        const AUTHOR_COUNT: usize = $crate::__private::author_count(
            &[$($($author),*)?],
            ::core::env!("CARGO_PKG_AUTHORS"),
        );
        static AUTHORS: [$crate::abi::Str; AUTHOR_COUNT] = $crate::__private::authors(
            &[$($($author),*)?],
            ::core::env!("CARGO_PKG_AUTHORS"),
        );
        static BUILDS: &[$crate::abi::BuildRow] = &[
            $($($crate::abi::BuildRow {
                side: $crate::abi::Side::$bside,
                fingerprint: $crate::abi::Fingerprint { guid: $crate::abi::guid($guid), age: $age },
                symbols: (&[$($crate::abi::Symbol { name: $crate::abi::Str::new($sym), rva: $rva }),*]
                    as &[$crate::abi::Symbol]).as_ptr(),
                symbol_count: (&[$($crate::abi::Symbol { name: $crate::abi::Str::new($sym), rva: $rva }),*]
                    as &[$crate::abi::Symbol]).len(),
                constants: (&[$($($crate::abi::Symbol { name: $crate::abi::Str::new($constant), rva: $value }),*)?]
                    as &[$crate::abi::Symbol]).as_ptr(),
                constant_count: (&[$($($crate::abi::Symbol { name: $crate::abi::Str::new($constant), rva: $value }),*)?]
                    as &[$crate::abi::Symbol]).len(),
            },)*)?
        ];
        static MANIFEST: $crate::abi::Manifest = $crate::abi::Manifest {
            header: $crate::abi::Header::of::<$crate::abi::Manifest>(),
            id: $crate::abi::Str::new($id),
            version: $crate::abi::Str::new($crate::__private::or_default(
                &[$($version)?],
                ::core::env!("CARGO_PKG_VERSION"),
            )),
            side: $crate::abi::Side::$side,
            flags: $crate::__private::flags(&[$($join)?], &[$($reload)?]),
            dependencies: DEPENDENCIES.as_ptr(),
            dependency_count: DEPENDENCIES.len(),
            builds: BUILDS.as_ptr(),
            build_count: BUILDS.len(),
            name: $crate::abi::Str::new($crate::__private::first_text(&[$($name)?])),
            description: $crate::abi::Str::new($crate::__private::first_text(
                &[$($description,)? ::core::env!("CARGO_PKG_DESCRIPTION")],
            )),
            authors: AUTHORS.as_ptr(),
            author_count: AUTHOR_COUNT,
            license: $crate::abi::Str::new($crate::__private::first_text(
                &[$($license,)? ::core::env!("CARGO_PKG_LICENSE")],
            )),
            homepage: $crate::abi::Str::new($crate::__private::first_text(
                &[$($homepage,)? ::core::env!("CARGO_PKG_HOMEPAGE"), ::core::env!("CARGO_PKG_REPOSITORY")],
            )),
        };
        &MANIFEST
    }};
}

/// Exports a [`Mod`](crate::Mod) from the DLL: the five functions of
/// [`abi::exports`](crate::abi::exports), each catching panics.
///
/// Also adds `instance()` to the type, the mod's state once initialised.
///
/// # Examples
///
/// ```rust,standalone_crate
/// struct Quiet;
///
/// impl enshwick_loader_api::Mod for Quiet {
///     fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
///         enshwick_loader_api::manifest!(id: "quiet", side: SERVER)
///     }
///
///     fn init(_load: &mut enshwick_loader_api::Load<'_>) -> Result<Self, enshwick_loader_api::Off> {
///         Ok(Self)
///     }
/// }
///
/// enshwick_loader_api::declare_mod!(Quiet);
///
/// assert!(Quiet::instance().is_none());
/// ```
#[macro_export]
macro_rules! declare_mod {
    ($ty:ty) => {
        static __ENSHWICK_INSTANCE: ::std::sync::OnceLock<$ty> = ::std::sync::OnceLock::new();

        impl $ty {
            /// The mod's state, once the loader has initialised it.
            pub fn instance() -> ::core::option::Option<&'static Self> {
                __ENSHWICK_INSTANCE.get()
            }
        }

        /// The interface major this mod was built for.
        #[unsafe(no_mangle)]
        pub extern "system" fn enshwick_mod_abi() -> u32 {
            ::core::primitive::u32::from($crate::abi::ABI_MAJOR)
        }

        /// The mod's static manifest.
        #[unsafe(no_mangle)]
        pub extern "system" fn enshwick_mod_query() -> *const $crate::abi::Manifest {
            $crate::__private::query::<$ty>()
        }

        /// Initialises the mod.
        ///
        /// # Safety
        ///
        /// Called by the loader with its table and this mod's handle.
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn enshwick_mod_init(
            api: *const $crate::abi::HostApi,
            me: $crate::abi::ModHandle,
        ) -> $crate::abi::Status {
            // SAFETY: the loader passes its process-wide table.
            unsafe { $crate::__private::init::<$ty>(api, me, &__ENSHWICK_INSTANCE) }
        }

        /// Starts the mod after the hooks are armed.
        ///
        /// # Safety
        ///
        /// Called by the loader after `enshwick_mod_init` succeeded.
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn enshwick_mod_ready(
            _api: *const $crate::abi::HostApi,
            _me: $crate::abi::ModHandle,
        ) {
            $crate::__private::ready::<$ty>(&__ENSHWICK_INSTANCE);
        }

        /// Stops the mod before a development reload unloads it.
        #[unsafe(no_mangle)]
        pub extern "system" fn enshwick_mod_shutdown() {
            $crate::__private::shutdown::<$ty>(&__ENSHWICK_INSTANCE);
        }
    };
}

/// Logs a line at `error` level under the mod's id; formats like `format!`.
///
/// # Examples
///
/// ```
/// enshwick_loader_api::error!("solver failed: {}", "no zones");
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
/// enshwick_loader_api::warn!("unexpected bed {}", 1210);
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
/// enshwick_loader_api::info!("{} altars merged", 3);
/// ```
#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => { $crate::__log!($crate::abi::Level::INFO, $($arg)+) };
}

/// Logs a line at `debug` level under the mod's id; formats like `format!`.
///
/// Kept only when `mod-configs/enshwick.toml` asks for `debug`; otherwise the
/// arguments are neither evaluated nor formatted.
///
/// # Examples
///
/// ```
/// enshwick_loader_api::debug!("bed {} checked", 1210);
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
///     enshwick_loader_api::info_once!("first altar seen: {altar}");
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
///     enshwick_loader_api::warn_once!("an altar without a zone");
/// }
/// ```
#[macro_export]
macro_rules! warn_once {
    ($($arg:tt)+) => { $crate::__once!($crate::abi::Level::WARN, $($arg)+) };
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
///     enshwick_loader_api::warn_every!(Duration::from_secs(60), "bed {bed} has no owner");
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
/// enshwick_loader_api::error_every!(Duration::from_secs(60), "the zone table is full");
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
