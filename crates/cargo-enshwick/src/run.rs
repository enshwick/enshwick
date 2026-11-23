//! `cargo enshwick run`: install a mod into a dedicated server, start the
//! server and follow its logs until a world has loaded, then until Ctrl-C;
//! with `--watch`, rebuild, reinstall and restart on a change.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::bail;
use enshwick_base::log_files::{
    self, LOG_LEVEL_KEY, LOG_LEVELS_KEY, READY, SETTINGS_FILE, WORLD_SETTINGS,
};
use enshwick_core::abi::Level;

use crate::install::{Built, SERVER_EXE};
use crate::logs::{Entry, Follower, LOADER, LOGS};
use crate::steam::{Found, SERVER_APP};

/// What `cargo enshwick run` was asked for.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The mod's folder: a crate, or a data pack or Luau mod holding
    /// `pack.toml`.
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// The dedicated server's folder, holding `enshrouded_server.exe`;
    /// `ENSHROUDED_DIR` when omitted, else Steam's dedicated server.
    #[arg(long, env = "ENSHROUDED_DIR")]
    game: Option<PathBuf>,
    /// The package to run, when the workspace has several mods.
    #[arg(long, short)]
    package: Option<String>,
    /// Also puts in the Enshwick loader of the release this tool is from
    /// when the server has none.
    #[arg(long)]
    with_loader: bool,
    /// Builds and installs the mod again on a change in its folder and
    /// restarts the server.
    #[arg(long)]
    watch: bool,
    /// The most detailed of the mod's own lines shown: error, warn, info or
    /// debug. The server's `enshwick.toml` decides which lines reach the
    /// files; the run says when it keeps fewer.
    #[arg(long, default_value = "info")]
    level: Level,
}

/// The dedicated server's folder: `given`, else Steam's install of it, with a
/// line naming the one Steam had.
pub(crate) fn server_folder(
    given: Option<&Path>,
    find: impl FnOnce() -> Vec<Found>,
) -> anyhow::Result<(PathBuf, Option<String>)> {
    let (folder, said) = if given.is_some() {
        crate::install::game_folder(given, Vec::new)?
    } else {
        let servers: Vec<Found> = find().into_iter().filter(|f| f.app == SERVER_APP).collect();
        if servers.is_empty() {
            bail!(
                "Steam has no Enshrouded dedicated server installed here; install it (Steam app {SERVER_APP}) or name its folder with --game or ENSHROUDED_DIR"
            );
        }
        crate::install::game_folder(None, || servers)?
    };
    if !folder.join(SERVER_EXE).is_file() {
        bail!(
            "{} has no {SERVER_EXE}: run starts a dedicated server, never the game itself",
            folder.display()
        );
    }
    Ok((folder, said))
}

/// The state of one mod as the run's logs tell it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// Running, with the version the loader named when it is known.
    Running(Option<String>),
    /// Turned off, with the loader's reason.
    Off(String),
}

/// What a run's logs have said of the start so far.
#[derive(Debug, Default)]
struct Start {
    /// The loader's ready line, once written.
    ready: Option<String>,
    world: bool,
    reported: bool,
    mods: BTreeMap<String, State>,
}

impl Start {
    /// Takes in what `entry` says of the start.
    fn see(&mut self, entry: &Entry) {
        if entry.source == LOADER {
            if entry.message.starts_with(READY) {
                self.ready = Some(entry.message.clone());
            } else if entry.message.starts_with(WORLD_SETTINGS) {
                self.world = true;
            } else if let Some((id, version)) = log_files::initialised_mod(&entry.message) {
                self.mods
                    .insert(id.to_owned(), State::Running(Some(version.to_owned())));
            }
            return;
        }
        // A mod id is `<author>.<name>`; the loader's own sources have no dot.
        if !entry.source.contains('.') {
            return;
        }
        if let Some(reason) = log_files::off_reason(&entry.message) {
            self.mods
                .insert(entry.source.clone(), State::Off(reason.to_owned()));
        } else {
            self.mods
                .entry(entry.source.clone())
                .or_insert(State::Running(None));
        }
    }

    /// Once the loader is ready and a world has loaded, the first time: the
    /// ready line and a line per mod.
    fn report(&mut self) -> Option<Vec<String>> {
        if self.reported || !self.world {
            return None;
        }
        let ready = self.ready.as_ref()?;
        self.reported = true;
        let mut lines = vec![format!("world loaded; {ready}")];
        lines.extend(self.mods.iter().map(|(id, state)| match state {
            State::Running(Some(version)) => format!("  {id} {version}: running"),
            State::Running(None) => format!("  {id}: running"),
            State::Off(reason) => format!("  {id}: off: {reason}"),
        }));
        Some(lines)
    }

    /// Whether `entry` is shown while the run follows the logs: the mod's
    /// own lines up to `level`; the loader's up to `info` until the world
    /// has loaded and up to `warn` after; other mods' up to `warn`.
    fn shown(&self, entry: &Entry, id: &str, level: Level) -> bool {
        let most = if entry.source == id {
            level
        } else if entry.source == LOADER && !self.reported {
            Level::INFO
        } else {
            Level::WARN
        };
        entry.level.is_none_or(|l| l <= most)
    }

    /// Writes the entries `logs` gained that the run shows for the mod `id`
    /// up to `level`, and the start report once it is due.
    fn show_new(
        &mut self,
        logs: &mut Follower,
        id: &str,
        level: Level,
        out: &mut impl Write,
    ) -> anyhow::Result<()> {
        for entry in logs.poll()? {
            self.see(&entry);
            if self.shown(&entry, id, level) {
                writeln!(out, "{}", entry.text)?;
            }
            for line in self.report().into_iter().flatten() {
                writeln!(out, "{line}")?;
            }
        }
        out.flush()?;
        Ok(())
    }
}

/// Lines saying where the server's `enshwick.toml` keeps fewer lines than the
/// run needs: the loader's `info` lines, which tell when the world has
/// loaded, and the mod `id`'s lines up to `level`.
fn level_notes(server: &Path, id: &str, level: Level) -> Vec<String> {
    let kept: toml::Table = std::fs::read_to_string(server.join(SETTINGS_FILE))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    let level_in = |value: &toml::Value| value.as_str()?.parse::<Level>().ok();
    let of = |source: &str| {
        kept.get(LOG_LEVELS_KEY)
            .and_then(|levels| levels.get(source))
            .and_then(level_in)
            .or_else(|| kept.get(LOG_LEVEL_KEY).and_then(level_in))
            .unwrap_or_default()
    };
    let mut notes = Vec::new();
    if of(LOADER) < Level::INFO {
        notes.push(format!(
            "note: {SETTINGS_FILE} keeps the loader's lines up to {}, so the run cannot tell when the world has loaded",
            of(LOADER).name()
        ));
    }
    if of(id) < level {
        notes.push(format!(
            "note: {SETTINGS_FILE} keeps {id}'s lines up to {kept}; add log_levels = {{ \"{id}\" = \"{wanted}\" }} to it to see {wanted} lines",
            kept = of(id).name(),
            wanted = level.name()
        ));
    }
    notes
}

/// The files under a mod's folder with their sizes and times, as `--watch`
/// compares them.
type Fingerprint = Vec<(PathBuf, u64, Option<SystemTime>)>;

/// The fingerprint of the files under the mod's folder `dir`, build output
/// and hidden folders left out.
fn fingerprint(dir: &Path) -> Fingerprint {
    let mut found = Vec::new();
    let mut folders = vec![dir.to_owned()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "target" {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                folders.push(entry.path());
            } else {
                found.push((entry.path(), meta.len(), meta.modified().ok()));
            }
        }
    }
    found.sort();
    found
}

/// Installs the mod, starts the server and follows it until Ctrl-C or until
/// it exits, writing what it sees to `out`.
pub(crate) fn run(args: &Args, out: &mut impl Write) -> anyhow::Result<String> {
    let (server, chosen) = server_folder(args.game.as_deref(), || {
        crate::steam::find(&crate::steam::steam_folders())
    })?;
    if let Some(chosen) = chosen {
        writeln!(out, "{chosen}")?;
    }
    if let Some(line) = crate::install::loader_line(&server, args.with_loader)? {
        if !crate::loader::is_installed(&server) {
            bail!("{line}");
        }
        writeln!(out, "{line}")?;
    }
    // Taken before each build, so a change saved while the mod builds or the
    // server restarts builds next.
    let mut sources = args.watch.then(|| fingerprint(&args.dir));
    let mut built = crate::install::build(&args.dir, args.package.as_deref())?;
    for note in level_notes(&server, &built.id, args.level) {
        writeln!(out, "{note}")?;
    }
    process::listen()?;
    loop {
        let folder = built.place(&server)?;
        writeln!(
            out,
            "{}\ninstalled {} {} in {}",
            built.summary,
            built.id,
            built.version,
            folder.display()
        )?;
        let host = Host {
            start: process::Server::start,
            stopping: process::stopping,
        };
        built = match follow(&server, &built.id, args, &mut sources, out, host)? {
            End::Stopped(said) => return Ok(said),
            End::Changed(next) => next,
            End::Exited(exited) => {
                let Some(seen) = &mut sources else {
                    return ended(&exited);
                };
                writeln!(
                    out,
                    "{SERVER_EXE} exited: {}; it starts again after the next change that builds",
                    exited.status
                )?;
                match rebuilt(args, seen, out, process::stopping)? {
                    Some(next) => next,
                    None => return Ok(format!("{SERVER_EXE} not started again")),
                }
            }
        };
    }
}

/// What a run says when the server exited on its own: a failure for a
/// failing exit status, such as a crash's.
fn ended(exited: &Exited) -> anyhow::Result<String> {
    let said = format!("{SERVER_EXE} exited: {}", exited.status);
    if !exited.success {
        bail!("{said}");
    }
    Ok(said)
}

/// The mod built again once its folder differs from `seen`, which then
/// takes the change in; `None` when nothing changed or the build failed,
/// which it says.
fn changed(
    args: &Args,
    seen: &mut Fingerprint,
    out: &mut impl Write,
) -> anyhow::Result<Option<Built>> {
    let now = fingerprint(&args.dir);
    if now == *seen {
        return Ok(None);
    }
    *seen = now;
    writeln!(out, "{} changed: building it again", args.dir.display())?;
    match crate::install::build(&args.dir, args.package.as_deref()) {
        Ok(next) => Ok(Some(next)),
        Err(error) => {
            writeln!(out, "error: {error:#}\nit builds again on the next change")?;
            Ok(None)
        }
    }
}

/// Waits for a change under the mod's folder that builds; `None` once
/// `stopping` says Ctrl-C came first.
fn rebuilt(
    args: &Args,
    seen: &mut Fingerprint,
    out: &mut impl Write,
    stopping: impl Fn() -> bool,
) -> anyhow::Result<Option<Built>> {
    while !stopping() {
        if let Some(next) = changed(args, seen, out)? {
            return Ok(Some(next));
        }
        std::thread::sleep(crate::logs::POLL);
    }
    Ok(None)
}

/// How a run of the server ended.
#[derive(Debug)]
enum End {
    /// Ctrl-C stopped it.
    Stopped(String),
    /// It exited on its own.
    Exited(Exited),
    /// The mod changed and built again, to install before the next start.
    Changed(Built),
}

/// How the server exited.
#[derive(Debug)]
struct Exited {
    /// The exit status as Windows gives it.
    status: String,
    /// Whether the status is a success.
    success: bool,
}

/// A server process `follow` watches: the one this tool started, or a
/// test's stand-in.
trait Process {
    fn id(&self) -> u32;
    /// Whether it ends when this tool does.
    fn bound(&self) -> bool;
    /// How it exited, once it has.
    fn exited(&mut self) -> anyhow::Result<Option<Exited>>;
    /// Stops it; how that went.
    fn stop(&mut self) -> anyhow::Result<String>;
}

/// What `follow` takes from around it: how the server starts from its
/// executable, and whether Ctrl-C asked the run to stop.
struct Host<S, C> {
    start: S,
    stopping: C,
}

/// Starts the server in `server` and follows its logs until it ends, with
/// `sources` the fingerprint a `--watch` change is seen against. On an error
/// here the server still gets its Ctrl-Break, so it saves before it exits.
fn follow<P: Process>(
    server: &Path,
    id: &str,
    args: &Args,
    sources: &mut Option<Fingerprint>,
    out: &mut impl Write,
    host: Host<impl FnOnce(&Path) -> anyhow::Result<P>, impl Fn() -> bool>,
) -> anyhow::Result<End> {
    let mut logs = Follower::new_run(&server.join(LOGS));
    let mut child = (host.start)(&server.join(SERVER_EXE))?;
    let mut start = Start::default();
    let mut followed = || -> anyhow::Result<End> {
        writeln!(
            out,
            "started {SERVER_EXE} (process {}); Ctrl-C stops it",
            child.id()
        )?;
        if !child.bound() {
            writeln!(
                out,
                "warning: Windows gave {SERVER_EXE} no job object, so it outlives this tool if the tool is killed; stop it with Ctrl-C"
            )?;
        }
        loop {
            let exited = child.exited()?;
            start.show_new(&mut logs, id, args.level, out)?;
            if let Some(exited) = exited {
                return Ok(End::Exited(exited));
            }
            if (host.stopping)() {
                writeln!(out, "stopping {SERVER_EXE} (process {})", child.id())?;
                let how = child.stop()?;
                start.show_new(&mut logs, id, args.level, out)?;
                return Ok(End::Stopped(format!("{SERVER_EXE} stopped: {how}")));
            }
            if let Some(seen) = sources.as_mut()
                && let Some(next) = changed(args, seen, out)?
            {
                writeln!(out, "restarting {SERVER_EXE}")?;
                let how = child.stop()?;
                start.show_new(&mut logs, id, args.level, out)?;
                writeln!(out, "{SERVER_EXE} stopped: {how}")?;
                return Ok(End::Changed(next));
            }
            std::thread::sleep(crate::logs::POLL);
        }
    };
    let ended = followed();
    if ended.is_err() {
        let _stopped = child.stop();
    }
    ended
}

/// The server process: started in a process group of its own, so Ctrl-C
/// reaches only this tool, and in a job object that ends it with this tool.
#[cfg(windows)]
mod process {
    use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
    use std::os::windows::process::CommandExt as _;
    use std::path::Path;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use anyhow::Context as _;
    use windows_sys::Win32::Foundation::TRUE;
    use windows_sys::Win32::System::Console::{
        CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
        GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;

    /// How long the server has to save and exit after Ctrl-Break before it
    /// is ended.
    const GRACE: Duration = Duration::from_secs(30);

    /// How long the handler holds a console close, logoff or shutdown event,
    /// so the main loop stops the server meanwhile: Windows ends a process
    /// as soon as its handler returns from one, and after the hung-app
    /// time-out, 5 seconds by default, in any case.
    const CLOSE_HOLD: Duration = Duration::from_secs(5);

    /// Whether Ctrl-C or Ctrl-Break reached this tool.
    static STOPPING: AtomicBool = AtomicBool::new(false);

    /// How long [`on_ctrl`] holds the console event `kind` before it returns.
    fn hold(kind: u32) -> Duration {
        if matches!(
            kind,
            CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT
        ) {
            CLOSE_HOLD
        } else {
            Duration::ZERO
        }
    }

    /// Asks the run to stop; for an event that ends this tool once it
    /// returns, it first gives the run the time to stop the server, and
    /// the process ends as soon as the run has.
    extern "system" fn on_ctrl(kind: u32) -> windows_sys::core::BOOL {
        STOPPING.store(true, Ordering::Release);
        std::thread::sleep(hold(kind));
        TRUE
    }

    /// Takes Ctrl-C, Ctrl-Break and closing the console from now on, so the
    /// run stops the server it started before it exits.
    pub(super) fn listen() -> anyhow::Result<()> {
        // SAFETY: `on_ctrl` is an `extern "system"` handler of the signature
        // `PHANDLER_ROUTINE` names, which only stores to an atomic and
        // sleeps on the thread Windows makes for it, and lives for the
        // process.
        if unsafe { SetConsoleCtrlHandler(Some(on_ctrl), TRUE) } == 0 {
            return Err(std::io::Error::last_os_error()).context("taking Ctrl-C");
        }
        Ok(())
    }

    /// Whether Ctrl-C asked the run to stop.
    pub(super) fn stopping() -> bool {
        STOPPING.load(Ordering::Acquire)
    }

    /// A server this tool started, and only that one.
    #[derive(Debug)]
    pub(super) struct Server {
        child: Child,
        /// Kept open while the server runs: closing it ends the server.
        job: Option<OwnedHandle>,
    }

    impl Server {
        /// Starts `exe` in its folder, reading its console from this tool's,
        /// its own output left out: the logs say what it does.
        pub(super) fn start(exe: &Path) -> anyhow::Result<Self> {
            let folder = exe.parent().unwrap_or(Path::new("."));
            let child = Command::new(exe)
                .current_dir(folder)
                .stdin(Stdio::inherit())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NEW_PROCESS_GROUP)
                .spawn()
                .with_context(|| exe.display().to_string())?;
            let job = job_of(&child);
            Ok(Self { child, job })
        }
    }

    impl super::Process for Server {
        fn id(&self) -> u32 {
            self.child.id()
        }

        fn bound(&self) -> bool {
            self.job.is_some()
        }

        fn exited(&mut self) -> anyhow::Result<Option<super::Exited>> {
            Ok(self
                .child
                .try_wait()?
                .map(|status: ExitStatus| super::Exited {
                    status: status.to_string(),
                    success: status.success(),
                }))
        }

        /// Asks the server to stop with Ctrl-Break, sent to its process group
        /// alone, and ends it by its handle when it has not exited within
        /// [`GRACE`].
        fn stop(&mut self) -> anyhow::Result<String> {
            if let Some(status) = self.child.try_wait()? {
                return Ok(format!("it had exited: {status}"));
            }
            // SAFETY: plain call; the group id is the process id of the server,
            // which `CREATE_NEW_PROCESS_GROUP` made the root of its own group.
            let asked = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, self.child.id()) } != 0;
            let until = Instant::now() + GRACE;
            while asked && Instant::now() < until {
                if let Some(status) = self.child.try_wait()? {
                    return Ok(format!("exited on Ctrl-Break: {status}"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            self.child.kill()?;
            let status = self.child.wait()?;
            Ok(format!(
                "ended {}: {status}",
                if asked {
                    "after it outlasted Ctrl-Break"
                } else {
                    "by its handle, as Ctrl-Break could not be sent"
                }
            ))
        }
    }

    /// A job object holding `child` that ends it when the job's last handle
    /// closes, so the server never outlives this tool; `None` when Windows
    /// refuses one.
    fn job_of(child: &Child) -> Option<OwnedHandle> {
        // SAFETY: null attributes and name ask for an unnamed job with the
        // default security; the handle returned is checked before use.
        let raw = unsafe { CreateJobObjectW(core::ptr::null(), core::ptr::null()) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: `raw` is a job handle just created and owned by no one
        // else; `OwnedHandle` closes it once.
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).ok()?;
        // SAFETY: `limits` is the struct `JobObjectExtendedLimitInformation`
        // reads, `size` bytes long, alive across the call.
        let set = unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size,
            )
        };
        // SAFETY: both handles are open: the job just made and the child's,
        // which `Child` keeps open until it drops.
        let assigned = set != 0
            && unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) } != 0;
        assigned.then_some(job)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_console_close_is_held_for_the_server_to_stop_and_ctrl_c_is_not() {
            use windows_sys::Win32::System::Console::CTRL_C_EVENT;
            assert_eq!(hold(CTRL_C_EVENT), Duration::ZERO);
            assert_eq!(hold(CTRL_BREAK_EVENT), Duration::ZERO);
            for kind in [CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT] {
                assert_eq!(hold(kind), CLOSE_HOLD);
            }
        }
    }
}

/// Off Windows, where the dedicated server does not run: the start fails,
/// so no server ever exists.
#[cfg(not(windows))]
mod process {
    use std::path::Path;

    /// Nothing to take: no server is ever started.
    #[expect(clippy::unnecessary_wraps, reason = "the Windows one can fail")]
    pub(super) fn listen() -> anyhow::Result<()> {
        Ok(())
    }

    pub(super) fn stopping() -> bool {
        false
    }

    /// No server: it cannot be made off Windows.
    #[derive(Debug)]
    pub(super) enum Server {}

    impl Server {
        pub(super) fn start(exe: &Path) -> anyhow::Result<Self> {
            anyhow::bail!("{} runs on Windows only", exe.display())
        }
    }

    impl super::Process for Server {
        fn id(&self) -> u32 {
            match *self {}
        }

        fn bound(&self) -> bool {
            match *self {}
        }

        fn exited(&mut self) -> anyhow::Result<Option<super::Exited>> {
            match *self {}
        }

        fn stop(&mut self) -> anyhow::Result<String> {
            match *self {}
        }
    }
}

#[cfg(test)]
mod tests;
