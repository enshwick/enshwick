//! The loader as one mod sees it.

use core::ffi::c_void;

use crate::Off;
use crate::abi::{
    BuildInfo, CallerFrame, Event, EventFn, Fingerprint, Header, HostApi, Level, ModHandle, Side,
    Status, Str,
};

static HOST: crate::fresh::Fresh<Host> = crate::fresh::Fresh::new();

thread_local! {
    /// The host of the data pack whose code this thread runs: the loader's
    /// own copy of the API runs every pack, and its process host is the
    /// loader's.
    static RUNNING: core::cell::Cell<Option<Host>> = const { core::cell::Cell::new(None) };
}

/// The loader's services for this mod, valid for the life of the process.
///
/// Each mod DLL links its own copy of this crate, so [`host`] answers for the
/// mod that calls it. The loader's own copy runs every data pack and answers
/// the pack whose code runs on the calling thread, so its answer is not kept
/// across calls.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// if let Some(host) = enshwick::host() {
///     host.log(enshwick::abi::Level::INFO, "ready");
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Host {
    api: &'static HostApi,
    me: ModHandle,
}

/// The loader's services, once the loader has initialised this mod; for a
/// data pack, the pack running on this thread.
///
/// # Examples
///
/// See [`Host`].
#[must_use]
pub fn host() -> Option<Host> {
    RUNNING.get().or_else(|| HOST.get().copied())
}

/// The data pack whose code this thread runs in the loader's copy of the
/// API; `None` in a mod's own copy and in the loader's own code.
pub fn pack() -> Option<Host> {
    RUNNING
        .get()
        .filter(|running| HOST.get().is_none_or(|own| own.me != running.me))
}

/// Runs `call` with `running` as the answer of [`host`] on this thread, the
/// process's host for `None`, and restores the answer after, also on a panic.
pub(crate) fn running_as<R>(running: Option<Host>, call: impl FnOnce() -> R) -> R {
    struct Restore(Option<Host>);
    impl Drop for Restore {
        fn drop(&mut self) {
            RUNNING.set(self.0);
        }
    }
    let _restore = Restore(RUNNING.replace(running));
    call()
}

/// Spawns `body` on a new thread named `name` that answers [`host`] as the
/// calling thread does, so a data pack's thread logs and faults as the pack.
///
/// # Errors
///
/// When the system cannot start a thread.
pub(crate) fn spawn(
    name: &str,
    body: impl FnOnce() + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    let running = RUNNING.get();
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || running_as(running, body))
}

/// Runs a mod's callback for the loader; a panic faults the mod with
/// `reason` and the panic's message, and `fallback` answers in its place.
#[inline]
pub fn guard<R>(reason: &str, fallback: impl FnOnce() -> R, call: impl FnOnce() -> R) -> R {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).unwrap_or_else(|payload| {
        fault_with(reason, payload.as_ref());
        fallback()
    })
}

/// Faults the mod for `reason`, followed by the message of the panic that
/// left `payload`.
#[cold]
#[inline(never)]
pub fn fault_with(reason: &str, payload: &(dyn core::any::Any + Send)) {
    if let Some(host) = host() {
        host.fault(&format!("{reason}: {}", panic_text(payload)));
    }
}

/// The first line of a panic's message, which a reason shows on one line.
pub fn panic_text(payload: &(dyn core::any::Any + Send)) -> &str {
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
/// place and thread.
///
/// The line comes before the panic's own handling. Each mod DLL has its own
/// panic hook, so the line goes to the mod's log; the previous hook still
/// runs.
pub fn log_panics() {
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

/// Whether a loader table under `header` serves a mod built for minor
/// `minor` of this major: the same major, and that minor or a newer one.
fn serves(header: Header, minor: u16) -> bool {
    header.abi_major == crate::abi::ABI_MAJOR && header.abi_minor >= minor
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
    #[doc(hidden)]
    pub unsafe fn new(api: *const HostApi, me: ModHandle) -> Option<Self> {
        // SAFETY: by this function's `# Safety`, `api` is null or an aligned
        // table that starts with its header and lives for the process;
        // `as_ref` maps null to `None`.
        let header = unsafe { api.cast::<Header>().as_ref() }?;
        if !serves(*header, crate::abi::ABI_MINOR) || !header.covers(size_of::<HostApi>()) {
            return None;
        }
        // SAFETY: the table is readable for its header's size by this
        // function's `# Safety`, and the check above found that size at least
        // `size_of::<HostApi>()`.
        let api = unsafe { api.as_ref() }?;
        Some(Self { api, me })
    }

    /// Makes this the answer of [`host`]; the first call wins.
    #[doc(hidden)]
    pub fn install(self) -> Self {
        *HOST.get_or_init(|| self)
    }

    /// Runs `call` with this as the answer of [`host`] on this thread, so a
    /// data pack's code logs, reads and faults as the pack.
    #[doc(hidden)]
    pub fn scope<R>(self, call: impl FnOnce() -> R) -> R {
        running_as(Some(self), call)
    }

    /// The raw table, for calls the safe layer does not wrap.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
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
    /// # extern crate enshwick_core as enshwick;
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
    /// # extern crate enshwick_core as enshwick;
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
    /// # extern crate enshwick_core as enshwick;
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
    /// `enshwick.toml` in the game's folder.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
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

    /// The text of this mod's settings file `file` in force, empty while no
    /// world runs.
    pub(crate) fn settings_text(&self, file: &str) -> Result<String, Off> {
        let mut text = Str::EMPTY;
        let status = (self.api.settings)(self.me, Str::new(file), &raw mut text);
        crate::load::ok(status, (), || format!("settings {file}.toml"))?;
        // SAFETY: the loader keeps the text for the life of the process.
        let text = unsafe { text.as_str() }.ok_or_else(|| Off::new("settings: not UTF-8"))?;
        Ok(text.to_owned())
    }

    /// Hands the loader the schema and template of this mod's settings file
    /// `file`.
    pub(crate) fn config_schema(
        &self,
        file: &str,
        schema: &str,
        template: &str,
    ) -> Result<(), Off> {
        let status = (self.api.config_schema)(
            self.me,
            Str::new(file),
            Str::new(schema),
            Str::new(template),
        );
        if status == Status::REFUSED {
            return Err(Off::new(format!(
                "settings: two settings types write {file}.toml; name a file for each"
            )));
        }
        crate::load::ok(status, (), || format!("the settings schema of {file}.toml"))
    }

    /// Subscribes `callback` to `event`.
    #[doc(hidden)]
    pub fn subscribe(&self, event: Event, callback: EventFn, user: *mut c_void) -> Status {
        (self.api.subscribe)(self.me, event, callback, user)
    }

    /// Reports that this mod failed at run time. The loader unlinks every hook
    /// of the mod, so the game runs vanilla where the mod was.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
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
    /// # extern crate enshwick_core as enshwick;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::ABI_MAJOR;

    fn table(major: u16, minor: u16) -> Header {
        Header {
            size: 0,
            abi_major: major,
            abi_minor: minor,
        }
    }

    #[test]
    fn a_table_serves_mods_of_its_major_up_to_its_minor() {
        assert!(serves(table(ABI_MAJOR, 1), 0));
        assert!(serves(table(ABI_MAJOR, 1), 1));
        assert!(!serves(table(ABI_MAJOR, 0), 1), "an older loader");
        assert!(!serves(table(ABI_MAJOR + 1, 1), 1));
        assert!(!serves(table(ABI_MAJOR - 1, 1), 1));
    }
}
