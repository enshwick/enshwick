//! The status summary: what the loader knows of every mod, the lines the
//! mods' status sources give, what mods serve and patch, and the hook
//! counters. Written to `logs/enshwick/status.log` a minute after start,
//! then every `status_minutes`, and whenever an administrator asks with
//! `/enshwick status`; each write replaces the last.

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use enshwick_loader_api::abi::{Side, Status, StatusFn, Str};

use crate::diag::{HookStat, ModStatus, Summary};
use crate::host::Loader;
use crate::settings::LOADER_ID;

/// How often the writer looks at the clock and for requests.
const POLL: Duration = Duration::from_secs(1);
/// The first summary after start, so a fresh server shows one soon.
const FIRST: Duration = Duration::from_secs(60);

/// A mod's status source.
#[derive(Clone, Copy, Debug)]
struct Source {
    owner: usize,
    callback: StatusFn,
    user: usize,
}

static SOURCES: Mutex<Vec<Source>> = Mutex::new(Vec::new());
/// Set by `/enshwick status`; the writer writes on its next look.
static REQUESTED: AtomicBool = AtomicBool::new(false);
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Marks the loader's start, for the uptime.
pub(crate) fn mark_start() {
    let _first = STARTED.set(Instant::now());
}

/// Adds mod `owner`'s status source.
pub(crate) fn register(owner: usize, callback: StatusFn, user: *mut c_void) -> Status {
    SOURCES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Source {
            owner,
            callback,
            user: user.expose_provenance(),
        });
    Status::OK
}

/// Asks the writer to write the summary now.
pub(crate) fn request() {
    REQUESTED.store(true, Ordering::Release);
}

/// The lines mod `owner`'s sources give now.
fn lines_of(sources: &[Source], owner: usize) -> Vec<String> {
    sources
        .iter()
        .filter(|s| s.owner == owner)
        .flat_map(|source| {
            let mut out = Str::EMPTY;
            // SAFETY: the source lives in the mod's DLL, which stays loaded;
            // the safe layer catches its panics.
            unsafe {
                (source.callback)(
                    core::ptr::with_exposed_provenance_mut(source.user),
                    &raw mut out,
                );
            };
            // SAFETY: the text stays valid until the source runs again.
            let text = unsafe { out.as_str() }.unwrap_or("");
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The name of a hooked function: as a mod or the loader found it, else its
/// place in the game.
fn hook_name(loader: &Loader, target: u64) -> String {
    crate::diag::name_of(target)
        .or_else(|| {
            loader
                .build
                .symbols
                .iter()
                .find(|&&(_, rva)| loader.base + rva == target)
                .map(|&(name, _)| name.to_owned())
        })
        .unwrap_or_else(|| format!("game+{:#x}", target.wrapping_sub(loader.base)))
}

/// Takes the summary now.
pub(crate) fn summary(loader: &Loader) -> Summary {
    let sources = SOURCES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let mods = loader
        .mods
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let running = !record.off.load(Ordering::Acquire);
            ModStatus {
                id: record.id.clone(),
                version: record.version.clone(),
                running,
                lines: if running {
                    lines_of(&sources, index)
                } else {
                    Vec::new()
                },
            }
        })
        .collect();
    let owner_name = |owner: usize| {
        if owner == crate::chains::LOADER {
            LOADER_ID.to_owned()
        } else {
            loader
                .mods
                .get(owner)
                .map_or_else(|| "?".to_owned(), |m| m.id.clone())
        }
    };
    let (targets, hooks) = match loader.chains.lock() {
        Ok(chains) => {
            let targets = chains.iter().filter(|c| c.head.is_some()).count();
            let hooks = crate::diag::counting().then(|| {
                crate::patch::counters()
                    .iter()
                    .map(|&(target, counter)| HookStat {
                        name: hook_name(loader, target),
                        owners: chains.owners(target).into_iter().map(owner_name).collect(),
                        calls: counter.calls.load(Ordering::Relaxed),
                        samples: counter.samples.load(Ordering::Relaxed),
                        ticks: counter.ticks.load(Ordering::Relaxed),
                    })
                    .collect()
            });
            (targets, hooks)
        }
        Err(_) => (0, None),
    };
    Summary {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        game: loader.build.name.to_owned(),
        hosting: loader.side == Side::SERVER || crate::world::hosting(),
        uptime: STARTED.get().map_or(Duration::ZERO, Instant::elapsed),
        at: crate::diag::now(),
        mods,
        turned_off: crate::diag::history(),
        served: crate::content::summary(),
        patches: crate::diag::patches(),
        targets,
        hooks,
        timed: crate::diag::timing(),
        tsc_hz: crate::patch::tsc_hz(),
    }
}

/// Writes the summary to `status.log`.
fn write(loader: &Loader) {
    let Some(game_dir) = loader.mods_dir.parent() else {
        return;
    };
    let path = game_dir.join("logs").join(LOADER_ID).join("status.log");
    if let Err(e) = std::fs::write(&path, summary(loader).render()) {
        crate::log!(WARN, "status summary not written: {e}");
    }
}

/// Starts the writer on a thread of its own.
pub(crate) fn start(loader: &'static Loader) {
    let started = std::thread::Builder::new()
        .name("enshwick-status".into())
        .spawn(move || {
            let mut next = Instant::now() + FIRST;
            loop {
                std::thread::sleep(POLL);
                let minutes = crate::diag::status_minutes();
                let due = minutes > 0 && Instant::now() >= next;
                if REQUESTED.swap(false, Ordering::AcqRel) || due {
                    write(loader);
                    next = Instant::now() + Duration::from_secs(u64::from(minutes.max(1)) * 60);
                }
            }
        });
    if let Err(e) = started {
        crate::log!(WARN, "no status summaries: {e}");
    }
}
