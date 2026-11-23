//! `cargo enshwick run`: install a mod into a dedicated server, start the
//! server and follow its logs until a world has loaded, then until Ctrl-C;
//! with `--watch`, rebuild, reinstall and restart on a change.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::bail;
use enshwick_core::abi::Level;

use crate::install::Built;
use crate::logs::{Entry, LOADER, LOGS};
use crate::steam::{Found, SERVER_APP};

/// The dedicated server's executable, which `run` starts.
const SERVER_EXE: &str = "enshrouded_server.exe";
/// How the loader's line opens once every mod's `ready` ran:
/// `ready: <running> of <all> mods running`.
const READY: &str = "ready: ";
/// How the loader's line opens as a world loads, before its mods read their
/// settings.
const WORLD_LOADED: &str = "world settings: world ";
/// How the line opens that a mod's log gets when the loader turns it off.
const OFF: &str = "off: ";
/// How the loader's line ends for a mod whose `init` passed, after its id
/// and version.
const INITIALISED: &str = " initialised";

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
    /// debug.
    #[arg(long, default_value = "info", value_parser = crate::logs::level)]
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
            } else if entry.message.starts_with(WORLD_LOADED) {
                self.world = true;
            } else if let Some(mod_and_version) = entry.message.strip_suffix(INITIALISED)
                && let Some((id, version)) = mod_and_version.split_once(' ')
                && !id.is_empty()
                && !version.contains(' ')
            {
                self.mods
                    .insert(id.to_owned(), State::Running(Some(version.to_owned())));
            }
            return;
        }
        // A mod id is `<author>.<name>`; the loader's own sources have no dot.
        if !entry.source.contains('.') {
            return;
        }
        if let Some(reason) = entry.message.strip_prefix(OFF) {
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
}

/// The files under a mod's folder with their sizes and times, build output
/// and hidden folders left out: what `--watch` compares.
fn fingerprint(dir: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
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
    let mut built = crate::install::build(&args.dir, args.package.as_deref())?;
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
        match follow(&server, &built.id, args, out)? {
            End::Stopped(said) | End::Exited(said) => return Ok(said),
            End::Changed(next) => built = next,
        }
    }
}

/// How a run of the server ended.
#[derive(Debug)]
enum End {
    /// Ctrl-C stopped it.
    Stopped(String),
    /// It exited on its own.
    Exited(String),
    /// The mod changed and built again, to install before the next start.
    Changed(Built),
}

/// Starts the server in `server` and follows its logs until it ends.
fn follow(server: &Path, id: &str, args: &Args, out: &mut impl Write) -> anyhow::Result<End> {
    let mut logs = crate::logs::Follower::new_run(&server.join(LOGS));
    let mut child = process::Server::start(&server.join(SERVER_EXE))?;
    writeln!(
        out,
        "started {SERVER_EXE} (process {}); Ctrl-C stops it",
        child.id()
    )?;
    let mut start = Start::default();
    let mut sources = args.watch.then(|| fingerprint(&args.dir));
    loop {
        let exited = child.exited()?;
        for entry in logs.poll()? {
            start.see(&entry);
            if start.shown(&entry, id, args.level) {
                writeln!(out, "{}", entry.text)?;
            }
            for line in start.report().into_iter().flatten() {
                writeln!(out, "{line}")?;
            }
        }
        out.flush()?;
        if let Some(status) = exited {
            return Ok(End::Exited(format!("{SERVER_EXE} exited: {status}")));
        }
        if process::stopping() {
            writeln!(out, "stopping {SERVER_EXE} (process {})", child.id())?;
            let how = child.stop()?;
            return Ok(End::Stopped(format!("{SERVER_EXE} stopped: {how}")));
        }
        if let Some(seen) = &mut sources {
            let now = fingerprint(&args.dir);
            if now != *seen {
                *seen = now;
                writeln!(out, "{} changed: building it again", args.dir.display())?;
                match crate::install::build(&args.dir, args.package.as_deref()) {
                    Ok(next) => {
                        writeln!(out, "restarting {SERVER_EXE}")?;
                        let how = child.stop()?;
                        writeln!(out, "{SERVER_EXE} stopped: {how}")?;
                        return Ok(End::Changed(next));
                    }
                    Err(error) => writeln!(
                        out,
                        "error: {error:#}\nthe server runs on with the mod installed before"
                    )?,
                }
            }
        }
        std::thread::sleep(crate::logs::POLL);
    }
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
        CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
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

    /// Whether Ctrl-C or Ctrl-Break reached this tool.
    static STOPPING: AtomicBool = AtomicBool::new(false);

    /// Takes Ctrl-C and Ctrl-Break from now on, so the run stops the server
    /// it started before it exits.
    pub(super) fn listen() -> anyhow::Result<()> {
        extern "system" fn on_ctrl(_kind: u32) -> windows_sys::core::BOOL {
            STOPPING.store(true, Ordering::Release);
            TRUE
        }
        // SAFETY: `on_ctrl` is an `extern "system"` handler of the signature
        // `PHANDLER_ROUTINE` names, which only stores to an atomic and lives
        // for the process.
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
        _job: Option<OwnedHandle>,
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
            Ok(Self { child, _job: job })
        }

        pub(super) fn id(&self) -> u32 {
            self.child.id()
        }

        /// The exit status once the server has exited.
        pub(super) fn exited(&mut self) -> anyhow::Result<Option<ExitStatus>> {
            Ok(self.child.try_wait()?)
        }

        /// Asks the server to stop with Ctrl-Break, sent to its process group
        /// alone, and ends it by its handle when it has not exited within
        /// [`GRACE`]; how it went.
        pub(super) fn stop(&mut self) -> anyhow::Result<String> {
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
}

/// Off Windows, where the dedicated server does not run: the start fails,
/// so no server ever exists.
#[cfg(not(windows))]
mod process {
    use std::path::Path;
    use std::process::ExitStatus;

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

        pub(super) fn id(&self) -> u32 {
            match *self {}
        }

        pub(super) fn exited(&mut self) -> anyhow::Result<Option<ExitStatus>> {
            match *self {}
        }

        pub(super) fn stop(&mut self) -> anyhow::Result<String> {
            match *self {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::tests::line;

    fn entry(level: Level, source: &str, message: &str) -> Entry {
        Entry {
            at: (0, "10:00:00.000Z".to_owned()),
            level: Some(level),
            source: source.to_owned(),
            message: message.to_owned(),
            text: line("10:00:00.000", level, source, message),
        }
    }

    #[test]
    fn the_state_of_each_mod_is_reported_once_the_world_has_loaded() {
        let mut start = Start::default();
        start.see(&entry(
            Level::INFO,
            LOADER,
            "you.hello-rust 0.1.0 initialised",
        ));
        start.see(&entry(Level::INFO, "you.hello-rust", "Hello, world"));
        start.see(&entry(Level::INFO, "you.hello-pack", "data pack: 0 ids"));
        start.see(&entry(
            Level::ERROR,
            "you.hello-pack",
            "off: init returned 3 (E2003, see https://enshwick.com/errors/e2003/)",
        ));
        start.see(&entry(
            Level::INFO,
            LOADER,
            "item drops ready: the pickup read",
        ));
        start.see(&entry(Level::ERROR, "enshwick-crash", "off: not a mod"));
        assert_eq!(start.report(), None, "not ready");
        start.see(&entry(Level::INFO, LOADER, "ready: 1 of 2 mods running"));
        assert_eq!(start.report(), None, "no world yet");
        start.see(&entry(
            Level::INFO,
            LOADER,
            "world settings: world D:/s/world0",
        ));
        assert_eq!(
            start.report().unwrap(),
            [
                "world loaded; ready: 1 of 2 mods running",
                "  you.hello-pack: off: init returned 3 (E2003, see https://enshwick.com/errors/e2003/)",
                "  you.hello-rust 0.1.0: running",
            ]
        );
        assert_eq!(start.report(), None, "once");
    }

    #[test]
    fn the_loader_says_less_once_the_world_has_loaded_and_the_mod_says_what_its_level_allows() {
        let mut start = Start::default();
        let loader_info = entry(Level::INFO, LOADER, "18 hook targets armed");
        let mine_debug = entry(Level::DEBUG, "you.mod", "detail");
        let other_info = entry(Level::INFO, "you.other", "hello");
        assert!(start.shown(&loader_info, "you.mod", Level::INFO));
        assert!(!start.shown(&mine_debug, "you.mod", Level::INFO));
        assert!(start.shown(&mine_debug, "you.mod", Level::DEBUG));
        assert!(!start.shown(&other_info, "you.mod", Level::DEBUG));
        start.reported = true;
        assert!(!start.shown(&loader_info, "you.mod", Level::INFO));
    }

    /// The lines `run` waits for are the ones the loader writes.
    #[test]
    fn the_markers_match_the_loader_s_lines() {
        let loader = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime/loader/src");
        let read = |file: &str| std::fs::read_to_string(loader.join(file)).unwrap();
        let start = read("start.rs");
        assert!(start.contains(&format!("\"{READY}{{running}} of {{}} mods running\"")));
        assert!(start.contains(&format!("\"{{}} {{}}{INITIALISED}\"")));
        assert!(read("base/worlds/live.rs").contains(&format!("\"{WORLD_LOADED}{{key}}\"")));
        assert!(read("engine/loader.rs").contains(&format!("\"{OFF}{{shown}} ({{}})\"")));
        assert!(read("engine/log.rs").contains("\"{source}_{stamp}.log\""));
    }

    #[test]
    fn a_change_under_the_mod_s_folder_shows_and_build_output_does_not() {
        let dir = crate::scratch("watch");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "a").unwrap();
        let before = fingerprint(&dir);
        std::fs::write(dir.join("target/out.dll"), "b").unwrap();
        assert_eq!(fingerprint(&dir), before);
        std::fs::write(dir.join("src/lib.rs"), "ab").unwrap();
        assert_ne!(fingerprint(&dir), before);
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_server_folder_is_steam_s_dedicated_server_and_never_the_game() {
        let game = crate::scratch("client");
        std::fs::write(game.join("enshrouded.exe"), "").unwrap();
        let refused = server_folder(Some(&game), Vec::new)
            .unwrap_err()
            .to_string();
        assert!(refused.contains("never the game"), "{refused}");
        let server = crate::scratch("server");
        std::fs::write(server.join(SERVER_EXE), "").unwrap();
        let found = |folder: &Path, app| Found {
            folder: folder.to_owned(),
            app,
        };
        let only_game = server_folder(None, || vec![found(&game, crate::steam::GAME_APP)]);
        assert!(
            only_game
                .unwrap_err()
                .to_string()
                .contains("no Enshrouded dedicated server")
        );
        let (taken, _) = server_folder(None, || {
            vec![
                found(&game, crate::steam::GAME_APP),
                found(&server, SERVER_APP),
            ]
        })
        .unwrap();
        assert_eq!(taken, server);
        for dir in [game, server] {
            let _gone = std::fs::remove_dir_all(dir);
        }
    }
}
