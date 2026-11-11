//! The exports of a mod DLL, which run its `Mod`'s `init` and `ready`.

/// The exports of `declare_mod!`: `$manifest` builds the mod's
/// `__private::Manifested`, `$texts` the mod's embedded texts, if any.
#[doc(hidden)]
#[macro_export]
macro_rules! __declare_exports {
    ($ty:ty, $manifest:expr, $texts:expr) => {
        $crate::__require_unwind!();

        static __ENSHWICK_INSTANCE: $crate::__private::Slot<$ty> = $crate::__private::Slot::new();

        static __ENSHWICK_MANIFEST: $crate::__private::Manifested = $manifest;

        static __ENSHWICK_TEXTS: ::core::option::Option<&str> = $texts;

        impl $ty {
            /// The mod's state, once the loader has initialised it.
            pub fn instance() -> ::core::option::Option<&'static Self> {
                __ENSHWICK_INSTANCE.get()
            }
        }

        /// The ABI major this mod was built for.
        #[cfg_attr(not(test), unsafe(no_mangle))]
        pub extern "system" fn enshwick_mod_abi() -> u32 {
            ::core::primitive::u32::from($crate::abi::ABI_MAJOR)
        }

        /// The mod's manifest, or null when it does not build.
        ///
        /// Any thread may call it, again and again. The first call reads
        /// the binary's schema section and allocates the schema text; a
        /// panic there is caught and returns null. The manifest stays
        /// valid for the process.
        #[cfg_attr(not(test), unsafe(no_mangle))]
        pub extern "system" fn enshwick_mod_query() -> *const $crate::abi::Manifest {
            __ENSHWICK_MANIFEST.query()
        }

        /// Initialises the mod.
        ///
        /// # Safety
        ///
        /// - `api` points at the loader's table, alive and unchanged for the
        ///   process.
        /// - `me` is the handle the loader gave this mod.
        #[cfg_attr(not(test), unsafe(no_mangle))]
        pub unsafe extern "system" fn enshwick_mod_init(
            api: *const $crate::abi::HostApi,
            me: $crate::abi::ModHandle,
        ) -> $crate::abi::Status {
            // SAFETY: the caller guarantees `api` is the loader's table, alive
            // for the process, which is the contract of `init`.
            unsafe {
                $crate::__private::init::<$ty>(
                    api,
                    me,
                    &__ENSHWICK_MANIFEST,
                    __ENSHWICK_TEXTS,
                    &__ENSHWICK_INSTANCE,
                )
            }
        }

        /// Starts the mod after the hooks are armed.
        ///
        /// # Safety
        ///
        /// - `enshwick_mod_init` of this mod returned OK before the call.
        #[cfg_attr(not(test), unsafe(no_mangle))]
        pub unsafe extern "system" fn enshwick_mod_ready(
            _api: *const $crate::abi::HostApi,
            _me: $crate::abi::ModHandle,
        ) {
            $crate::__private::ready::<$ty>(&__ENSHWICK_INSTANCE);
        }
    };
}
