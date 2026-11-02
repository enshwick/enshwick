//! The loader as one mod sees it.

use core::ffi::c_void;

use crate::Off;
use crate::abi::{
    BuildInfo, CallerFrame, Event, EventFn, Fingerprint, Header, HostApi, Level, ModHandle, Side,
    Status, Str,
};

static HOST: crate::fresh::Fresh<Host> = crate::fresh::Fresh::new();

/// The loader's services for this mod, valid for the life of the process.
///
/// Each mod DLL links its own copy of this crate, so [`host`] always answers
/// for the mod that calls it.
///
/// # Examples
///
/// ```
/// if let Some(host) = enshwick::host() {
///     host.log(enshwick::abi::Level::INFO, "ready");
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

/// Runs a mod's callback for the loader; a panic faults the mod with
/// `reason` and the panic's message, and `fallback` answers in its place.
#[inline]
pub(crate) fn guard<R>(reason: &str, fallback: impl FnOnce() -> R, call: impl FnOnce() -> R) -> R {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).unwrap_or_else(|payload| {
        fault_with(reason, payload.as_ref());
        fallback()
    })
}

/// Faults the mod for `reason`, followed by the message of the panic that
/// left `payload`.
#[cold]
#[inline(never)]
pub(crate) fn fault_with(reason: &str, payload: &(dyn core::any::Any + Send)) {
    if let Some(host) = host() {
        host.fault(&format!("{reason}: {}", panic_text(payload)));
    }
}

/// The first line of a panic's message, which a reason shows on one line.
pub(crate) fn panic_text(payload: &(dyn core::any::Any + Send)) -> &str {
    let text = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or(NO_MESSAGE);
    text.lines().next().unwrap_or(NO_MESSAGE)
}

/// What a panic without a text message reads as.
const NO_MESSAGE: &str = "a panic without a message";

/// Logs every panic of this mod's code through the loader, with its message,
/// place and thread, before the panic's own handling runs. Each mod DLL has
/// its own panic hook, so the line goes to the mod's log; the previous hook
/// still runs.
pub(crate) fn log_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Some(host) = host() {
                host.log(Level::ERROR, &panic_line(info));
            }
            previous(info);
        }));
    });
}

/// `panicked at src/lib.rs:4:9 on thread main: the message`, on one line.
fn panic_line(info: &std::panic::PanicHookInfo<'_>) -> String {
    let message = info
        .payload_as_str()
        .unwrap_or(NO_MESSAGE)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("without a name");
    match info.location() {
        Some(at) => format!("panicked at {at} on thread {thread}: {message}"),
        None => format!("panicked on thread {thread}: {message}"),
    }
}

impl Host {
    /// Wraps the loader's table for the mod `me`; `None` for a table of
    /// another ABI major, an older minor or one shorter than this build's.
    ///
    /// # Safety
    ///
    /// - `api` is null, or points at an aligned [`HostApi`] readable for the size its
    ///   header states.
    /// - That table stays valid and unchanged for the rest of the process,
    ///   as the loader's does.
    pub(crate) unsafe fn new(api: *const HostApi, me: ModHandle) -> Option<Self> {
        // SAFETY: by this function's `# Safety`, `api` is null or an aligned
        // table that starts with its header and lives for the process;
        // `as_ref` maps null to `None`.
        let header = unsafe { api.cast::<Header>().as_ref() }?;
        if header.abi_major != crate::abi::ABI_MAJOR
            || !(crate::abi::ABI_MINOR..).contains(&header.abi_minor)
            || !header.covers(size_of::<HostApi>())
        {
            return None;
        }
        // SAFETY: the table is readable for its header's size by this
        // function's `# Safety`, and the check above found that size at least
        // `size_of::<HostApi>()`.
        let api = unsafe { api.as_ref() }?;
        Some(Self { api, me })
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
    /// if let Some(host) = enshwick::host() {
    ///     assert_eq!(host.api().header.abi_major, enshwick::abi::ABI_MAJOR);
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
    /// if let Some(host) = enshwick::host() {
    ///     assert_ne!(host.handle(), enshwick::abi::ModHandle::NONE);
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
    /// if let Some(host) = enshwick::host() {
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
            table: crate::abi::BuildTable::EXACT,
        };
        (self.api.build)(&raw mut out);
        out
    }

    /// Whether this process runs a game server now: always on the dedicated
    /// server; on the client while it hosts a game, solo play included, from
    /// the built-in server's first step until the game stops.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick::host() {
    ///     let acts = host.hosting();
    /// }
    /// ```
    #[must_use]
    pub fn hosting(&self) -> bool {
        (self.api.hosting)() != 0
    }

    /// The entity world of the game server this process runs, 0 while none
    /// runs or before its first step.
    pub(crate) fn server_world(&self) -> u64 {
        (self.api.server_world)()
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

    /// The most detailed level the loader keeps for this mod's lines, from
    /// `mod-configs/enshwick.toml`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Level;
    ///
    /// if let Some(host) = enshwick::host() {
    ///     let verbose = host.log_level() >= Level::DEBUG;
    /// }
    /// ```
    #[must_use]
    pub fn log_level(&self) -> Level {
        (self.api.log_level)(self.me)
    }

    /// The text of this mod's settings file, written from `defaults` when
    /// missing.
    pub(crate) fn settings_text(&self, defaults: &str) -> Result<String, Off> {
        let mut text = Str::EMPTY;
        let status = (self.api.settings)(self.me, Str::new(defaults), &raw mut text);
        crate::load::ok(status, (), || "settings".to_owned())?;
        // SAFETY: the loader keeps the text for the life of the process.
        let text = unsafe { text.as_str() }.ok_or_else(|| Off::new("settings: not UTF-8"))?;
        Ok(text.to_owned())
    }

    /// Hands the loader this mod's settings schema.
    pub(crate) fn config_schema(&self, schema: &str) {
        let status = (self.api.config_schema)(self.me, Str::new(schema));
        if !status.is_ok() {
            match crate::config::Schema::parse(schema) {
                Err(why) => crate::warn!("the loader refused the {why}"),
                Ok(_) => crate::warn!("the loader refused the settings schema ({})", status.0),
            }
        }
    }

    /// Reports this mod's join values, `synced`, and returns the server's to
    /// run with over them; `None` when there are none.
    pub(crate) fn sync(&self, synced: &str) -> Option<String> {
        let mut text = Str::EMPTY;
        let status = (self.api.sync)(self.me, Str::new(synced), &raw mut text);
        if !status.is_ok() {
            return None;
        }
        // SAFETY: the loader keeps the text for the life of the process.
        let text = unsafe { text.as_str() }?;
        (!text.is_empty()).then(|| text.to_owned())
    }

    /// Subscribes `callback` to `event`.
    pub(crate) fn subscribe(&self, event: Event, callback: EventFn, user: *mut c_void) -> Status {
        (self.api.subscribe)(self.me, event, callback, user)
    }

    /// Reports that this mod failed at run time. The loader unlinks every hook
    /// of the mod, so the game runs vanilla where the mod was.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick::host() {
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
    /// if let Some(host) = enshwick::host() {
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
