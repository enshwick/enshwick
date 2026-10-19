//! The loader as one mod sees it.

use std::sync::OnceLock;

use crate::abi::{
    BuildInfo, CallerFrame, Fingerprint, Header, HostApi, Level, ModHandle, Side, Str,
};

static HOST: OnceLock<Host> = OnceLock::new();

/// The loader's services for this mod, valid for the life of the process.
///
/// Each mod DLL links its own copy of this crate, so [`host`] always answers
/// for the mod that calls it.
///
/// # Examples
///
/// ```
/// if let Some(host) = enshwick_api::host() {
///     host.log(enshwick_api::abi::Level::INFO, "ready");
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Host {
    api: &'static HostApi,
    me: ModHandle,
}

/// The loader's services, once the loader has initialised this mod.
///
/// # Examples
///
/// See [`Host`].
#[must_use]
pub fn host() -> Option<Host> {
    HOST.get().copied()
}

impl Host {
    /// Wraps the loader's table for the mod `me`.
    ///
    /// # Safety
    ///
    /// `api` must point at a valid [`HostApi`] that outlives the process, as
    /// the loader's does.
    pub(crate) unsafe fn new(api: *const HostApi, me: ModHandle) -> Option<Self> {
        // SAFETY: the caller guarantees `api` is valid for the process.
        let api = unsafe { api.as_ref() }?;
        (api.header.abi_major == crate::abi::ABI_MAJOR).then_some(Self { api, me })
    }

    /// Makes this the answer of [`host`]; the first call wins.
    pub(crate) fn install(self) -> Self {
        *HOST.get_or_init(|| self)
    }

    /// The raw table, for calls the safe layer does not wrap.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_api::host() {
    ///     assert_eq!(host.api().header.abi_major, enshwick_api::abi::ABI_MAJOR);
    /// }
    /// ```
    #[must_use]
    pub fn api(&self) -> &'static HostApi {
        self.api
    }

    /// This mod's handle.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_api::host() {
    ///     assert_ne!(host.handle(), enshwick_api::abi::ModHandle::NONE);
    /// }
    /// ```
    #[must_use]
    pub fn handle(&self) -> ModHandle {
        self.me
    }

    /// The running game: side, image base and build fingerprint.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_api::host() {
    ///     let build = host.build();
    ///     assert_ne!(build.base, 0);
    /// }
    /// ```
    #[must_use]
    pub fn build(&self) -> BuildInfo {
        let mut out = BuildInfo {
            header: Header::of::<BuildInfo>(),
            side: Side(0),
            base: 0,
            fingerprint: Fingerprint {
                guid: [0; 16],
                age: 0,
            },
        };
        (self.api.build)(&raw mut out);
        out
    }

    /// Writes `message` to the log under this mod's id.
    ///
    /// Not for hot paths: a hook logs once per event worth knowing.
    ///
    /// # Examples
    ///
    /// See [`Host`].
    pub fn log(&self, level: Level, message: &str) {
        (self.api.log)(self.me, level, Str::new(message));
    }

    /// Reports that this mod failed at run time. The loader unlinks every hook
    /// of the mod, so the game runs vanilla where the mod was.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_api::host() {
    ///     host.fault("the altar table is corrupt");
    /// }
    /// ```
    pub fn fault(&self, reason: &str) {
        (self.api.fault)(self.me, Str::new(reason));
    }

    /// The game caller's context at entry of the running hook chain, for hooks
    /// declared with [`HookFlags::CALLER_FRAME`](crate::abi::HookFlags).
    ///
    /// Valid until the hook calls anything hooked.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_api::host() {
    ///     let frame = host.caller_frame();
    ///     let _ = frame.stack();
    /// }
    /// ```
    #[must_use]
    pub fn caller_frame(&self) -> CallerFrame {
        let mut frame = CallerFrame::default();
        (self.api.caller_frame)(&raw mut frame);
        frame
    }
}
