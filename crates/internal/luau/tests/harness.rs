//! A Luau mod with the scripts a test gives, started on a fake host: the
//! one harness of the tests of each API module's Luau calls.

#![cfg(test)]

use std::sync::Mutex;

use enshwick::abi::Status;
use enshwick::config::Schema;
use enshwick::test::{FakeHost, Installed};
use enshwick::{Load, Mod, Off};
use enshwick_luau::{Limits, Runtime, Scripts};

static CASE: Mutex<Option<(Scripts, Option<Schema>, Limits)>> = Mutex::new(None);
static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);

struct Luau;

impl Mod for Luau {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let (scripts, settings, limits) = CASE.lock().unwrap().take().unwrap();
        let runtime = enshwick_luau::start(load, "you.luau", scripts, settings, limits)?;
        *RUNTIME.lock().unwrap() = Some(runtime);
        Ok(Self)
    }
}

enshwick::test::declare_mod!(
    Luau,
    r#"
    id = "you.luau"
    side = "both"
    "#
);

/// Starts `scripts` with `main` as the mod's `main.luau` and the settings
/// `settings` declares on `host`, with the status init returned.
pub(crate) fn start_with(
    host: FakeHost,
    main: &str,
    scripts: Scripts,
    settings: Option<Schema>,
) -> (Installed, Status) {
    start_with_limits(host, main, scripts, settings, Limits::default())
}

/// Starts as [`start_with`] does, under `limits`.
pub(crate) fn start_with_limits(
    host: FakeHost,
    main: &str,
    scripts: Scripts,
    settings: Option<Schema>,
    limits: Limits,
) -> (Installed, Status) {
    *CASE.lock().unwrap() = Some((scripts.with("main.luau", main), settings, limits));
    let loader = host.install();
    // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    (loader, status)
}

/// Starts `main` as the mod's `main.luau` on `host`, with the status init
/// returned.
pub(crate) fn start(host: FakeHost, main: &str) -> (Installed, Status) {
    start_with(host, main, Scripts::new(), None)
}

/// Starts `main` on `host` and expects the start to succeed.
pub(crate) fn run(host: FakeHost, main: &str) -> Installed {
    let (loader, status) = start(host, main);
    assert_eq!(status, Status::OK, "{:?}", loader.logs());
    loader
}

/// Whether a log line of `loader` holds `text`.
pub(crate) fn logged(loader: &Installed, text: &str) -> bool {
    loader.logs().iter().any(|(_, line)| line.contains(text))
}
