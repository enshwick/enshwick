//! The exports and the manifest a mod DLL gives the loader.

/// Builds a mod's static [`Manifest`](crate::abi::Manifest).
///
/// `id` is `[a-z0-9-]+` and names the DLL and the settings file; `side` is
/// `SERVER`, `CLIENT` or `BOTH`. Optional, in this order: `join = true` (the
/// mod counts in the join check), `requires: ["id" => "range", ...]` and
/// `after: ["id", ...]`. The version is the crate's.
///
/// # Examples
///
/// ```
/// let manifest = enshwick::manifest!(
///     id: "survivors-per-altar",
///     side: BOTH,
///     join = true,
///     requires: ["flame-altar-zones" => "^0.1"],
/// );
/// assert_eq!(manifest.dependency_count, 1);
/// ```
#[macro_export]
macro_rules! manifest {
    (
        id: $id:literal,
        side: $side:ident
        $(, join = $join:expr)?
        $(, requires: [$($req:literal => $range:literal),* $(,)?])?
        $(, after: [$($after:literal),* $(,)?])?
        $(,)?
    ) => {{
        const _: () = assert!($crate::abi::is_valid_id($id), "a mod id is [a-z0-9-]+");
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
        ];
        static MANIFEST: $crate::abi::Manifest = $crate::abi::Manifest {
            header: $crate::abi::Header::of::<$crate::abi::Manifest>(),
            id: $crate::abi::Str::new($id),
            version: $crate::abi::Str::new(env!("CARGO_PKG_VERSION")),
            side: $crate::abi::Side::$side,
            flags: $crate::__private::join_flag(&[$($join)?]),
            dependencies: DEPENDENCIES.as_ptr(),
            dependency_count: DEPENDENCIES.len(),
            builds: ::core::ptr::null(),
            build_count: 0,
        };
        &MANIFEST
    }};
}

/// Exports a [`Mod`](crate::Mod) from the DLL: the four functions the loader
/// looks for, each catching panics.
///
/// Also adds `instance()` to the type, the mod's state once initialised.
///
/// # Examples
///
/// ```
/// struct Quiet;
///
/// impl enshwick::Mod for Quiet {
///     fn manifest() -> &'static enshwick::abi::Manifest {
///         enshwick::manifest!(id: "quiet", side: SERVER)
///     }
///
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
///
/// enshwick::declare_mod!(Quiet);
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
            u32::from($crate::abi::ABI_MAJOR)
        }

        /// The mod's static manifest.
        #[unsafe(no_mangle)]
        pub extern "system" fn enshwick_mod_query() -> *const $crate::abi::Manifest {
            <$ty as $crate::Mod>::manifest()
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
    };
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
    ($($arg:tt)+) => { $crate::__private::log($crate::abi::Level::INFO, ::std::format_args!($($arg)+)) };
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
    ($($arg:tt)+) => { $crate::__private::log($crate::abi::Level::WARN, ::std::format_args!($($arg)+)) };
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
    ($($arg:tt)+) => { $crate::__private::log($crate::abi::Level::ERROR, ::std::format_args!($($arg)+)) };
}
