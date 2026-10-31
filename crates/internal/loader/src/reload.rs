//! Development hot reload, `dev_reload = true` on a dedicated server or a
//! client.
//!
//! Every mod is loaded from a copy under `mods/enshwick/reload/`, so its DLL
//! in `mods/` stays writable. A thread watches those DLLs; when one changes
//! and its mod allows it (`reload: true` in the manifest), the loader wires
//! the mod's hooks past it and drops its events, waits for calls in flight,
//! runs its `shutdown`, loads the new copy and initialises it again with the
//! same handle, then runs its `ready`. The old copy stays loaded until the
//! server stops, so a call still running in it never faults. A mod that used
//! anything a reload cannot hand over (content, actions, services, patches
//! and every other registration besides hooks, events, lookups, settings and
//! logging, and on a client the interface's frame and HUD callbacks and map
//! markers) is refused with the calls it made; so is a new copy whose
//! manifest changed beyond its version, or that hooks a function nothing
//! hooked at start, and on a client a mod the server's join turned off.
//! Those take a restart.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use arc_swap::ArcSwap;
use enshwick::abi::{Manifest, ManifestFlags, exports};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};

use crate::host::{self, Loader, Subscription};
use crate::locks::Relock;
use crate::mod_index::ModIndex;

/// The folder under `mods/enshwick/` that holds the copies mods run from.
pub(crate) const COPIES_DIR: &str = "reload";

/// What a mod may have called during `init` and still be reloaded: hooks,
/// events, lookups, settings and logging, which the loader unlinks or
/// which keep nothing of the mod.
const RELOADABLE: &[&str] = &[
    "build",
    "symbol",
    "system",
    "field_offset",
    "type_size",
    "type_hash",
    "constant",
    "hook",
    "caller_frame",
    "fault",
    "log",
    "log_level",
    "settings",
    "data_dir",
    "subscribe",
    "subscribe_tick",
    "set_tick_every",
    "mods",
    "hosting",
    "config_schema",
    "code_range",
    "content_read",
    "resource_read",
    "game_text",
    "ui_frame",
    "ui_hud",
    "ui_widget",
    "ui_markers",
];

/// How long calls already inside an unlinked mod may take to leave it.
const GRACE: Duration = Duration::from_millis(750);
/// How often the watcher looks at the DLLs.
const POLL: Duration = Duration::from_secs(1);

/// No mod.
const NONE: usize = usize::MAX;

/// Whether `dev_reload` is on in the settings.
static WANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Takes `dev_reload` from the settings.
pub(crate) fn configure(wanted: bool) {
    WANTED.store(wanted, Ordering::Release);
}

/// Whether mods load from copies for reloads: `dev_reload` on, in the
/// dedicated server or the client.
pub(crate) fn copies() -> bool {
    WANTED.load(Ordering::Acquire)
        && std::env::current_exe().is_ok_and(|exe| {
            exe.file_name().is_some_and(|n| {
                n.eq_ignore_ascii_case("enshrouded_server.exe")
                    || n.eq_ignore_ascii_case("enshrouded.exe")
            })
        })
}

/// The mods a server's join turned off on this client, which stay off
/// until the game restarts.
static HELD: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Keeps mod `id` off across reloads: the server it joined does not run it.
pub(crate) fn hold(id: &str) {
    let mut held = HELD.relock();
    if !held.iter().any(|h| h == id) {
        held.push(id.to_owned());
    }
}

fn held(id: &str) -> bool {
    HELD.relock().iter().any(|h| h == id)
}

/// The mod whose `init` runs now, whose calls are recorded.
static INITIALISING: AtomicUsize = AtomicUsize::new(NONE);
/// The mod a reload initialises again, whose hooks and events join the
/// sealed chains and deliveries.
static REINIT: AtomicUsize = AtomicUsize::new(NONE);

/// Per mod: its reload generation, which its events carry.
static GENERATIONS: OnceLock<Box<[AtomicU32]>> = OnceLock::new();
/// Per mod: the table calls it made during `init`.
static CALLS: Mutex<Vec<Vec<&'static str>>> = Mutex::new(Vec::new());
/// Events reloaded mods subscribed to after the deliveries were sealed.
static LATE: OnceLock<ArcSwap<Vec<Subscription>>> = OnceLock::new();

/// One watched mod.
#[derive(Debug)]
struct Watched {
    index: usize,
    /// The DLL in `mods/`.
    original: PathBuf,
    /// The copy it runs from.
    copy: PathBuf,
    module: HMODULE,
    /// How the original looked when last loaded or seen changing.
    seen: Option<(SystemTime, u64)>,
    /// The look of the original one poll ago, while it changes.
    changing: Option<(SystemTime, u64)>,
    /// How many copies were loaded.
    copies: u32,
}

// SAFETY: the only non-`Send` field is the `HMODULE`, a process-wide
// handle with no thread affinity, and only the watcher thread uses it.
unsafe impl Send for Watched {}

/// Readies the per-mod state for `mods` mods.
pub(crate) fn prepare(mods: usize) {
    let _first = GENERATIONS.set((0..mods).map(|_| AtomicU32::new(0)).collect());
    *CALLS.relock() = vec![Vec::new(); mods];
}

/// Runs `init` of mod `index` with its table calls recorded.
pub(crate) fn initialising<R>(index: usize, init: impl FnOnce() -> R) -> R {
    INITIALISING.store(index, Ordering::Release);
    let outcome = init();
    INITIALISING.store(NONE, Ordering::Release);
    outcome
}

/// Records that the mod initialising now called the table function `name`.
#[inline]
pub(crate) fn note(name: &'static str) {
    let index = INITIALISING.load(Ordering::Relaxed);
    if index != NONE {
        note_cold(index, name);
    }
}

#[cold]
fn note_cold(index: usize, name: &'static str) {
    let mut calls = CALLS.relock();
    if let Some(calls) = calls.get_mut(index)
        && !calls.contains(&name)
    {
        calls.push(name);
    }
}

/// Whether mod `index` is running its `init` now.
pub(crate) fn in_init(index: usize) -> bool {
    INITIALISING.load(Ordering::Acquire) == index
}

/// Whether mod `index` is being initialised again by a reload.
pub(crate) fn reinitialising(index: usize) -> bool {
    REINIT.load(Ordering::Acquire) == index
}

/// The reload generation of mod `index`.
pub(crate) fn generation(index: usize) -> u32 {
    GENERATIONS
        .get()
        .and_then(|g| g.get(index))
        .map_or(0, |g| g.load(Ordering::Acquire))
}

/// Adds an event of a reloaded mod after the deliveries were sealed.
pub(crate) fn subscribe_late(subscription: Subscription) {
    let late = LATE.get_or_init(|| ArcSwap::from_pointee(Vec::new()));
    late.rcu(|current| {
        let mut next = Vec::clone(current);
        next.push(subscription);
        next
    });
}

/// The events reloaded mods subscribed to after sealing.
pub(crate) fn late() -> Option<arc_swap::Guard<std::sync::Arc<Vec<Subscription>>>> {
    LATE.get().map(ArcSwap::load)
}

/// Where the copy number `n` of `original` goes.
fn copy_path(mods_dir: &Path, original: &Path, n: u32) -> Option<PathBuf> {
    let stem = original.file_stem()?.to_string_lossy();
    Some(
        mods_dir
            .join(crate::settings::LOADER_ID)
            .join(COPIES_DIR)
            .join(format!("{stem}-{n}.dll")),
    )
}

/// Copies `original` into the copies folder as copy number 0, for the
/// loader to load instead; `None`, logged, when it cannot.
pub(crate) fn first_copy(mods_dir: &Path, original: &Path) -> Option<PathBuf> {
    static CLEARED: std::sync::Once = std::sync::Once::new();
    let copy = copy_path(mods_dir, original, 0)?;
    CLEARED.call_once(|| {
        if let Some(dir) = copy.parent() {
            let _stale = std::fs::remove_dir_all(dir);
        }
    });
    let made = copy
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::copy(original, &copy));
    match made {
        Ok(_) => Some(copy),
        Err(e) => {
            crate::log!(WARN, "{} not copied for reloads: {e}", original.display());
            None
        }
    }
}

fn look(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Starts the watcher over `mods`: `(index, original, copy, module)` of each
/// mod loaded from a copy.
pub(crate) fn start(loader: &'static Loader, mods: Vec<(usize, PathBuf, PathBuf, HMODULE)>) {
    let mut watched: Vec<Watched> = mods
        .into_iter()
        .map(|(index, original, copy, module)| Watched {
            index,
            seen: look(&original),
            changing: None,
            original,
            copy,
            module,
            copies: 0,
        })
        .collect();
    crate::log!(
        INFO,
        "dev reload on: watching {} mod DLLs in mods/",
        watched.len()
    );
    let spawned = std::thread::Builder::new()
        .name("enshwick-reload".into())
        .spawn(move || {
            loop {
                std::thread::sleep(POLL);
                for mod_ in &mut watched {
                    poll(loader, mod_);
                }
            }
        });
    if let Err(e) = spawned {
        crate::log!(WARN, "dev reload off: {e}");
    }
}

/// Reloads `mod_` once its DLL changed and stayed the same for a poll.
fn poll(loader: &'static Loader, mod_: &mut Watched) {
    let now = look(&mod_.original);
    if now.is_none() || now == mod_.seen {
        mod_.changing = None;
        return;
    }
    if mod_.changing != now {
        mod_.changing = now;
        return;
    }
    mod_.seen = now;
    mod_.changing = None;
    let id = loader
        .mods
        .get(mod_.index)
        .map(|m| m.id.clone())
        .unwrap_or_default();
    match reload(loader, mod_) {
        Ok(note) => crate::log!(INFO, "{id} reloaded{note}"),
        Err(why) => crate::log!(WARN, "{id} changed but was not reloaded: {why}"),
    }
}

/// Why a changed mod DLL was not reloaded.
#[derive(Debug, thiserror::Error)]
enum ReloadModError {
    /// The loader has no mod of the index.
    #[error("no such mod")]
    NoMod,
    /// The manifest lacks `reload: true`.
    #[error("its manifest does not allow it (`reload: true`); restart the game")]
    NotAllowed,
    /// The server's join turned the mod off.
    #[error("the server's join turned it off; restart the game")]
    JoinHeld,
    /// The mod holds what a reload cannot hand over.
    #[error("it holds what a reload cannot hand over ({}); restart the game", calls.join(", "))]
    Holds { calls: Vec<String> },
    /// No path is left for another copy.
    #[error("no path for the new copy")]
    NoPath,
    /// The new copy was not written.
    #[error("the new copy: {0}")]
    Copy(#[source] std::io::Error),
    /// The new DLL did not load as a mod.
    #[error("the new DLL did not load as a mod (see above)")]
    NotAMod,
    /// The new DLL is another mod.
    #[error("the new DLL is another mod")]
    OtherMod,
    /// The manifest changed beyond its version.
    #[error("its manifest changed beyond its version; restart the game")]
    Shape,
    /// The new copy's init failed.
    #[error("its init returned {status}")]
    Init { status: u32 },
}

/// What in `manifest` must stay the same across a reload, its version
/// aside.
fn shape(manifest: &Manifest) -> (u8, u32, usize, usize) {
    (
        manifest.side.0,
        manifest.flags.0,
        manifest.dependency_count,
        manifest.build_count,
    )
}

fn reload(loader: &'static Loader, mod_: &mut Watched) -> Result<String, ReloadModError> {
    let index = mod_.index;
    let record = loader.mods.get(index).ok_or(ReloadModError::NoMod)?;
    if !record.manifest.flags.contains(ManifestFlags::RELOAD) {
        return Err(ReloadModError::NotAllowed);
    }
    if held(&record.id) {
        return Err(ReloadModError::JoinHeld);
    }
    let calls = CALLS.relock().get(index).cloned().unwrap_or_default();
    let held: Vec<&str> = calls
        .iter()
        .copied()
        .filter(|call| !RELOADABLE.contains(call))
        .collect();
    if !held.is_empty() {
        return Err(ReloadModError::Holds {
            calls: held.into_iter().map(str::to_owned).collect(),
        });
    }
    let copy = copy_path(&loader.mods_dir, &mod_.original, mod_.copies + 1)
        .ok_or(ReloadModError::NoPath)?;
    std::fs::copy(&mod_.original, &copy).map_err(ReloadModError::Copy)?;
    let Some(new) = crate::start::load_copy(&copy) else {
        let _gone = std::fs::remove_file(&copy);
        return Err(ReloadModError::NotAMod);
    };
    // SAFETY: `load_copy` returned the loader's copy of the manifest, whose
    // strings point into the new DLL's static data; the DLL stays loaded
    // until the `FreeLibrary` below, after the last use of `new_id`.
    let new_id = unsafe { new.manifest.id.as_str() }.unwrap_or_default();
    let refused = if new_id != record.id {
        Some(ReloadModError::OtherMod)
    } else if shape(new.manifest) != shape(record.manifest) {
        Some(ReloadModError::Shape)
    } else {
        None
    };
    if let Some(why) = refused {
        // SAFETY: `load_copy` loaded `new.module` and none of its code ran but
        // its query exports, which start nothing; nothing reads `new` after
        // this.
        unsafe { FreeLibrary(new.module) };
        let _gone = std::fs::remove_file(&copy);
        return Err(why);
    }
    unlink(loader, index);
    shutdown(mod_.module);
    loader.chains.relock().remove_owner(ModIndex::new(index));
    if let Some(calls) = CALLS.relock().get_mut(index) {
        calls.clear();
    }
    // The old copy stays loaded: a call still in flight in it, or a thread
    // it started, returns into mapped code. The next start clears the
    // copies folder.
    mod_.module = new.module;
    mod_.copy = copy;
    mod_.copies += 1;
    record.off.store(false, Ordering::Release);
    REINIT.store(index, Ordering::Release);
    let status = initialising(index, || {
        // SAFETY: `load_copy` typed `new.init` from the DLL's `init` export,
        // which `enshwick::abi::exports` declares with this signature; the
        // DLL stays loaded for the process, and `host::API` is a `static`.
        unsafe { (new.init)(&raw const host::API, host::handle(ModIndex::new(index))) }
    });
    REINIT.store(NONE, Ordering::Release);
    loader.chains.relock().rewire(host::is_off);
    if !status.is_ok() {
        loader.turn_off(
            ModIndex::new(index),
            &format!("reload: init returned {}", status.0),
        );
        return Err(ReloadModError::Init { status: status.0 });
    }
    // SAFETY: `load_copy` typed `new.ready` from the DLL's `ready` export,
    // which `enshwick::abi::exports` declares with this signature; it runs
    // after `init` returned OK, checked by the guard before it, the DLL stays
    // loaded for the process, and `host::API` is a `static`.
    unsafe { (new.ready)(&raw const host::API, host::handle(ModIndex::new(index))) };
    // SAFETY: `load_copy` returned the loader's copy of the manifest, whose
    // strings point into the new DLL's static data, loaded for the process.
    let version = unsafe { new.manifest.version.as_str() }.unwrap_or_default();
    Ok(if version == record.version {
        String::new()
    } else {
        format!(
            " as {version}; the loader lists it as {} until a restart",
            record.version
        )
    })
}

/// Wires the hooks of mod `index` past it, drops its events by moving it to
/// the next generation, and waits for calls in flight.
fn unlink(loader: &Loader, index: usize) {
    if let Some(generation) = GENERATIONS.get().and_then(|g| g.get(index)) {
        generation.fetch_add(1, Ordering::AcqRel);
    }
    loader.mods[index].off.store(true, Ordering::Release);
    loader.chains.relock().rewire(host::is_off);
    if loader.side == enshwick::abi::Side::CLIENT {
        // The new copy sets its own markers; the old copy's go.
        let _cleared = crate::ui::set_markers(ModIndex::new(index), &[]);
    }
    std::thread::sleep(GRACE);
}

/// Runs the `shutdown` export of `module`, when it has one.
fn shutdown(module: HMODULE) {
    // SAFETY: `module` is the non-null handle of the copy the watcher loaded
    // and never frees.
    let module = unsafe { crate::module::Module::from_handle(module) };
    let Some(found) = module.export(exports::SHUTDOWN) else {
        return;
    };
    // SAFETY: `enshwick::abi::exports::SHUTDOWN` documents the export as an
    // `extern "system" fn()`, which aborts rather than unwinds.
    let shutdown =
        unsafe { core::mem::transmute::<crate::module::Export, extern "system" fn()>(found) };
    shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_go_under_the_loader_folder_by_number() {
        let path = copy_path(
            Path::new("game/mods"),
            Path::new("game/mods/enshwick-x.dll"),
            3,
        );
        assert_eq!(
            path,
            Some(PathBuf::from("game/mods/enshwick/reload/enshwick-x-3.dll"))
        );
    }

    #[test]
    fn only_unlinkable_calls_allow_a_reload() {
        for call in [
            "hook",
            "subscribe",
            "subscribe_tick",
            "set_tick_every",
            "log",
            "settings",
            "ui_frame",
            "ui_hud",
            "game_text",
        ] {
            assert!(RELOADABLE.contains(&call));
        }
        for call in [
            "actions",
            "serve_resource",
            "patch_code",
            "publish",
            "hook_at",
        ] {
            assert!(!RELOADABLE.contains(&call));
        }
    }

    #[test]
    fn a_mod_the_join_turned_off_stays_held() {
        assert!(!held("held-mod"));
        hold("held-mod");
        hold("held-mod");
        assert!(held("held-mod"));
        assert_eq!(
            HELD.lock()
                .unwrap()
                .iter()
                .filter(|h| *h == "held-mod")
                .count(),
            1
        );
    }
}
