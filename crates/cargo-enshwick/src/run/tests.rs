use super::*;
use crate::logs::tests::line;

fn entry(level: Level, source: &str, message: &str) -> Entry {
    Entry {
        at: (0, 36_000_000),
        level: Some(level),
        source: source.to_owned(),
        message: message.to_owned(),
        text: line("10:00:00.000", level, source, message),
    }
}

/// A stand-in server: each look at it writes its next log line into the
/// server folder, as the loader would, and once they are out it exits
/// when `exits`, else runs until stopped, writing a last line as it
/// stops.
struct Stub {
    logs: PathBuf,
    lines: std::collections::VecDeque<(&'static str, String)>,
    exits: bool,
    bound: bool,
    stopped: std::rc::Rc<std::cell::Cell<bool>>,
}

impl Stub {
    /// Appends `text` to `source`'s file of the run.
    fn write(&self, source: &str, text: &str) -> std::io::Result<()> {
        use std::io::Write as _;
        let path = self.logs.join(format!("{source}_2026-11-23_10-00-00.log"));
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(text.as_bytes())
    }
}

impl Process for Stub {
    fn id(&self) -> u32 {
        7
    }

    fn bound(&self) -> bool {
        self.bound
    }

    fn exited(&mut self) -> anyhow::Result<Option<Exited>> {
        if let Some((source, text)) = self.lines.pop_front() {
            self.write(source, &text)?;
            return Ok(None);
        }
        Ok(self.exits.then(|| Exited {
            status: "exit code: 0".to_owned(),
            success: true,
        }))
    }

    fn stop(&mut self) -> anyhow::Result<String> {
        self.stopped.set(true);
        let last = line(
            "10:00:20.000",
            Level::WARN,
            "you.hello",
            "save hold refused",
        );
        self.write("you.hello", &last)?;
        Ok("exited on Ctrl-Break".to_owned())
    }
}

/// A writer whose every write fails, as a closed pipe's does.
struct Broken;

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A run's options for the mod in `dir`.
fn args(dir: &Path, watch: bool) -> Args {
    Args {
        dir: dir.to_owned(),
        game: None,
        package: None,
        with_loader: false,
        watch,
        level: Level::INFO,
    }
}

/// The items of a pack that checks clean.
const ITEMS: &str = "[[item]]\nid = \"lantern\"\nlike = \"6943053a-32c1-4718-ae22-e61427ddf54d\"\n";

/// A server folder with a loader that starts `you.hello` and loads a
/// world, the stub that writes its logs and the run's options.
fn fake_server(name: &str, exits: bool) -> (PathBuf, Stub, Args) {
    let server = crate::scratch(name);
    let logs = server.join(LOGS);
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(
        logs.join("enshwick_2026-11-22_10-00-00.log"),
        "an old run\n",
    )
    .unwrap();
    let lines = [
        (
            LOADER,
            line("10:00:00.000", Level::INFO, LOADER, "starting"),
        ),
        (
            "you.hello",
            line("10:00:00.100", Level::INFO, "you.hello", "Hello, world"),
        ),
        (
            "you.hello",
            line("10:00:00.110", Level::DEBUG, "you.hello", "detail"),
        ),
        (
            LOADER,
            line(
                "10:00:00.200",
                Level::INFO,
                LOADER,
                &log_files::initialised("you.hello", "0.1.0"),
            ),
        ),
        (
            LOADER,
            line("10:00:00.300", Level::INFO, LOADER, &log_files::ready(1, 1)),
        ),
        (
            LOADER,
            line(
                "10:00:10.000",
                Level::INFO,
                LOADER,
                &log_files::world_settings("w"),
            ),
        ),
        (
            LOADER,
            line("10:00:11.000", Level::INFO, LOADER, "after the world"),
        ),
    ];
    let stub = Stub {
        logs,
        lines: lines.into_iter().collect(),
        exits,
        bound: true,
        stopped: std::rc::Rc::default(),
    };
    let args = args(&server, false);
    (server, stub, args)
}

#[test]
fn a_run_prints_the_start_and_a_line_per_mod_until_the_server_exits() {
    let (server, stub, args) = fake_server("exits", true);
    let mut out = Vec::new();
    let host = Host {
        start: |exe: &Path| {
            assert_eq!(exe, server.join(SERVER_EXE));
            Ok(stub)
        },
        stopping: || false,
    };
    let end = follow(&server, "you.hello", &args, &mut None, &mut out, host).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(
        matches!(&end, End::Exited(exited) if exited.success && exited.status == "exit code: 0"),
        "{end:?}"
    );
    assert!(
        out.starts_with("started enshrouded_server.exe (process 7)"),
        "{out}"
    );
    assert!(!out.contains("no job object"), "{out}");
    assert!(out.contains("[you.hello] Hello, world"), "{out}");
    assert!(
        out.contains("world loaded; ready: 1 of 1 mods running\n  you.hello 0.1.0: running\n"),
        "{out}"
    );
    assert!(!out.contains("an old run"), "{out}");
    assert!(!out.contains("detail"), "{out}");
    assert!(
        !out.contains("after the world"),
        "the loader says only warnings then: {out}"
    );
    let _gone = std::fs::remove_dir_all(&server);
}

#[test]
fn ctrl_c_stops_the_server_the_run_started_and_prints_its_last_lines() {
    let (server, mut stub, args) = fake_server("stops", false);
    stub.bound = false;
    let stopped = std::rc::Rc::clone(&stub.stopped);
    let looks = std::cell::Cell::new(0);
    let host = Host {
        start: |_: &Path| Ok(stub),
        stopping: || {
            looks.set(looks.get() + 1);
            looks.get() > 8
        },
    };
    let mut out = Vec::new();
    let end = follow(&server, "you.hello", &args, &mut None, &mut out, host).unwrap();
    assert!(stopped.get());
    assert!(
        matches!(&end, End::Stopped(said) if said.ends_with("exited on Ctrl-Break")),
        "{end:?}"
    );
    let out = String::from_utf8(out).unwrap();
    let (_, after) = out
        .split_once("stopping enshrouded_server.exe (process 7)")
        .unwrap();
    assert!(after.contains("[you.hello] save hold refused"), "{out}");
    assert!(out.contains("no job object"), "{out}");
    let _gone = std::fs::remove_dir_all(&server);
}

#[test]
fn an_error_while_following_still_stops_the_server_with_ctrl_break() {
    let (server, stub, args) = fake_server("broken", false);
    let stopped = std::rc::Rc::clone(&stub.stopped);
    let host = Host {
        start: |_: &Path| Ok(stub),
        stopping: || false,
    };
    assert!(follow(&server, "you.hello", &args, &mut None, &mut Broken, host).is_err());
    assert!(stopped.get());
    let _gone = std::fs::remove_dir_all(&server);
}

#[test]
fn a_change_saved_before_the_server_started_builds_and_restarts_it() {
    let (server, stub, _) = fake_server("edited", false);
    let stopped = std::rc::Rc::clone(&stub.stopped);
    let pack = crate::pack::tests::pack("run-edited", ITEMS);
    let args = args(&pack, true);
    let mut sources = Some(fingerprint(&pack));
    std::fs::write(
        pack.join("items.toml"),
        format!("{ITEMS}# saved while it built\n"),
    )
    .unwrap();
    let host = Host {
        start: |_: &Path| Ok(stub),
        stopping: || false,
    };
    let mut out = Vec::new();
    let end = follow(&server, "you.lanterns", &args, &mut sources, &mut out, host).unwrap();
    assert!(
        matches!(&end, End::Changed(next) if next.id == "you.lanterns"),
        "{end:?}"
    );
    assert!(stopped.get());
    assert_eq!(sources, Some(fingerprint(&pack)));
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.contains("changed: building it again\nrestarting"),
        "{out}"
    );
    for dir in [server, pack] {
        let _gone = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_crash_fails_the_run_and_under_watch_the_next_change_that_builds_starts_it_again() {
    let crash = Exited {
        status: "exit code: 0xc0000005".to_owned(),
        success: false,
    };
    assert!(
        ended(&crash)
            .unwrap_err()
            .to_string()
            .contains("0xc0000005")
    );
    let clean = Exited {
        status: "exit code: 0".to_owned(),
        success: true,
    };
    assert!(ended(&clean).is_ok());
    let pack = crate::pack::tests::pack("run-crashed", ITEMS);
    let args = args(&pack, true);
    let mut seen = fingerprint(&pack);
    let mut out = Vec::new();
    assert!(
        rebuilt(&args, &mut seen, &mut out, || true)
            .unwrap()
            .is_none()
    );
    std::fs::write(pack.join("items.toml"), "not toml [").unwrap();
    let looks = std::cell::Cell::new(0);
    let gave_up = rebuilt(&args, &mut seen, &mut out, || {
        looks.set(looks.get() + 1);
        looks.get() > 2
    });
    assert!(gave_up.unwrap().is_none(), "a failed build waits on");
    std::fs::write(pack.join("items.toml"), ITEMS).unwrap();
    let next = rebuilt(&args, &mut seen, &mut out, || false)
        .unwrap()
        .unwrap();
    assert_eq!(next.id, "you.lanterns");
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.contains("error: ") && out.contains("it builds again on the next change"),
        "{out}"
    );
    let _gone = std::fs::remove_dir_all(&pack);
}

#[test]
fn a_world_without_a_key_counts_as_loaded() {
    let mut start = Start::default();
    start.see(&entry(Level::INFO, LOADER, &log_files::ready(0, 0)));
    start.see(&entry(Level::WARN, LOADER, log_files::NO_WORLD_KEY));
    assert_eq!(
        start.report().unwrap(),
        ["world loaded; ready: 0 of 0 mods running"]
    );
}

#[test]
fn the_run_says_when_enshwick_toml_keeps_fewer_lines_than_it_needs() {
    let server = crate::scratch("levels");
    assert!(level_notes(&server, "you.mod", Level::INFO).is_empty());
    std::fs::write(
        server.join(SETTINGS_FILE),
        "log_level = \"warn\"\nlog_levels = { \"you.mod\" = \"info\" }\nmotd = \"hi\"\n",
    )
    .unwrap();
    let notes = level_notes(&server, "you.mod", Level::DEBUG);
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes[0].contains("loader's lines up to warn"), "{notes:?}");
    assert!(
        notes[1].contains("log_levels = { \"you.mod\" = \"debug\" }"),
        "{notes:?}"
    );
    assert_eq!(level_notes(&server, "you.mod", Level::INFO).len(), 1);
    let _gone = std::fs::remove_dir_all(&server);
}

#[test]
fn the_state_of_each_mod_is_reported_once_the_world_has_loaded() {
    let mut start = Start::default();
    start.see(&entry(
        Level::INFO,
        LOADER,
        &log_files::initialised("you.hello-rust", "0.1.0"),
    ));
    start.see(&entry(Level::INFO, "you.hello-rust", "Hello, world"));
    start.see(&entry(Level::INFO, "you.hello-pack", "data pack: 0 ids"));
    start.see(&entry(
        Level::ERROR,
        "you.hello-pack",
        &log_files::off(
            "init returned 3",
            "E2003, see https://enshwick.com/errors/e2003/",
        ),
    ));
    start.see(&entry(
        Level::INFO,
        LOADER,
        "item drops ready: the pickup read",
    ));
    start.see(&entry(
        Level::ERROR,
        "enshwick-crash",
        &log_files::off("not a mod", "E2003"),
    ));
    assert_eq!(start.report(), None, "not ready");
    start.see(&entry(Level::INFO, LOADER, &log_files::ready(1, 2)));
    assert_eq!(start.report(), None, "no world yet");
    start.see(&entry(
        Level::INFO,
        LOADER,
        &log_files::world_settings("D:/s/world0"),
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
