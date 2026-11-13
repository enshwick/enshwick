//! The loader's start: identify the game, load the mods, arm their hooks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use enshwick_core::abi::Side;
use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleHandleExW,
};

use crate::base::resolve::{self, Candidate};
use crate::engine::chains::Chains;
use crate::engine::code::Watching;
use crate::engine::code_watch::Stage;
use crate::engine::loader::{Loader, ModRecord};
use crate::engine::mod_index::ModIndex;
use crate::engine::parts::{Part, Parts, part};
use crate::engine::patch::FrameSlot;
use crate::host;

mod arm;
mod dry_run;
mod game;
mod join_check;
mod library;
pub(crate) mod record;
#[cfg(test)]
pub(crate) mod tests;

pub use dry_run::{DryMod, DryRun, DryRunError, dry_run};

/// The start's own part: the build table ids of the join check.
pub(crate) static PART: Part = Part {
    table_ids: join_check::TABLE_IDS,
    ..part!("start")
};

use arm::arm;
use game::{begin, game, wait_for_window};
use join_check::{link_join, names};
use library::{Library, Run, libraries};

/// The client starts under Steam's DRM wrapper, which may check the game's code
/// before it runs. The game opens its window only once its own code runs, so
/// hooks wait for that window, and at most this long.
const CLIENT_ARM_DELAY: Duration = Duration::from_secs(15);
static FRAME: OnceLock<Option<FrameSlot>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

/// Stops the game when a mod that holds a world is not running.
fn enforce_holds(loader: &Loader) {
    let running = crate::base::holds::running(loader);
    crate::base::holds::enforce(&loader.mods_dir, &running, false, Some(loader));
}

/// Seals the registries mods filled during init: the subscriptions, then
/// each part's, in the order of the parts.
fn seal(loader: &'static Loader) {
    loader.seal_subscriptions();
    for seal in loader.parts.iter().filter_map(|part| part.seal) {
        seal(loader);
    }
}

/// Links the loader's own features, in the order of the parts; the client
/// links the server's too, for the games it hosts.
fn link_features(loader: &'static Loader) {
    for link in loader.parts.iter().filter_map(|part| part.link) {
        link(loader);
    }
}

pub(crate) fn start(missing_forwards: u32, release: extern "system" fn() -> u32) {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    // The caller frame needs TLS indices below 64, which the client's own
    // start uses up within seconds, so they are taken before anything else.
    let frame = *FRAME.get_or_init(FrameSlot::allocate);
    let began = std::time::Instant::now();
    let Some(game::Begun {
        mods_dir,
        settings_problems,
        developer_tools,
        release_check,
    }) = begin(missing_forwards)
    else {
        return;
    };
    let Some(game) = game(&mods_dir) else {
        return;
    };
    let mut watching = Watching::start(game.base);
    let found = libraries(&mods_dir, Run::Game, &mut |file, image| {
        if let Some(w) = watching.as_mut() {
            w.check(file, image, Stage::Load);
        }
    });
    let (libraries, candidates) = (found.libraries, found.candidates);
    let folders: Vec<(String, String, String)> = libraries
        .iter()
        .zip(&candidates)
        .map(|(library, candidate)| {
            (
                candidate.id.clone(),
                library.folder.clone(),
                library.file.clone(),
            )
        })
        .collect();
    let plan = resolve::resolve(&candidates);
    skip(&libraries, &plan);
    if let Some(w) = watching.as_mut() {
        w.check("", None, Stage::Skip);
    }
    let side = game.side;
    let loader = Loader {
        side,
        base: game.base,
        fingerprint: game.fingerprint,
        build: game.build,
        reflection: game.reflection,
        developer_tools,
        mods_dir,
        mods: records(&libraries, &candidates, &plan),
        others: found.others,
        chains: Mutex::new(Chains::default()),
        mids: Mutex::default(),
        caller_frame: read_frame,
        subscriptions: Mutex::default(),
        delivery: OnceLock::new(),
        settings_problems,
        newer_release: OnceLock::new(),
        retry: crate::engine::retry::Retry::default(),
        skipped: skipped(&libraries, &candidates, &plan, found.copies),
        parts: Parts::new(crate::parts::LAYERS),
    };
    let Some(loader) = crate::engine::loader::install(loader) else {
        crate::engine::shipped::release();
        return;
    };
    if release_check {
        crate::base::releases::start(loader);
    }
    if crate::engine::diag::counting() {
        crate::engine::profile::start(loader.mods.len());
    }
    let order: Vec<&Library> = plan.order.iter().map(|&i| &libraries[i]).collect();
    // SAFETY: `host::API` is the loader's process-wide table, a static that
    // lives for the process.
    unsafe {
        enshwick_base::__internal::install_host(&raw const host::API, host::LOADER_HANDLE);
    }
    attribute_crashes(loader, &order);
    initialise(loader, &order, watching.as_mut());
    seal(loader);
    link_features(loader);
    if side == Side::CLIENT {
        link_join(loader);
        let waited = wait_for_window(CLIENT_ARM_DELAY);
        crate::log!(
            INFO,
            "arming {} ms after the loader started",
            waited.as_millis()
        );
    }
    arm_watched(loader, frame, watching.as_mut());
    release_server(release, began);
    crate::ui::interface::armed(loader);
    enforce_holds(loader);
    ready(loader, &order, watching.as_mut());
    if let Some(w) = watching {
        w.finish();
    }
    record::take(loader, folders);
    start_services(loader);
}

/// Starts what runs once every mod is ready: the file watch, the status
/// summary, the server's own services and the join's guards.
fn start_services(loader: &'static Loader) {
    crate::base::watch::start(loader);
    crate::base::status::start();
    if loader.side == Side::SERVER {
        serve(loader);
    }
    crate::base::admission::start(loader, names(loader).into_iter().collect());
}

/// Starts what only a dedicated server runs: the advertised join mods and
/// the console.
fn serve(loader: &'static Loader) {
    if let Some(game_dir) = loader.mods_dir.parent() {
        crate::base::advertise::start(game_dir, crate::base::join::client::join_mods);
    }
    crate::actions::console::start();
}

/// Lets the dedicated server's own code run once the hooks are armed, and
/// logs whether the stub held it that long.
fn release_server(release: extern "system" fn() -> u32, began: std::time::Instant) {
    match release() {
        1 => crate::log!(
            INFO,
            "the server's own code starts after the hooks, {} ms after the loader started",
            began.elapsed().as_millis()
        ),
        2 => crate::log!(
            WARN,
            "the server's own code started before the hooks: the loader took {} ms, past the stub's hold",
            began.elapsed().as_millis()
        ),
        _ => {}
    }
}

/// Logs the mods `plan` skips.
fn skip(libraries: &[Library], plan: &resolve::Plan) {
    for (index, reason) in &plan.skipped {
        crate::log!(
            WARN,
            "{} skipped: {reason} ({})",
            libraries[*index].file,
            crate::engine::errors::UNMET_RELATIONS
        );
    }
}

/// The mods `plan` skips, then the other `copies` of a mod id, as the screens
/// name them.
fn skipped(
    libraries: &[Library],
    candidates: &[Candidate],
    plan: &resolve::Plan,
    copies: Vec<resolve::Skipped>,
) -> Vec<resolve::Skipped> {
    plan.skipped
        .iter()
        .map(|(i, reason)| resolve::Skipped {
            id: candidates[*i].id.clone(),
            version: candidates[*i].version.to_string(),
            // SAFETY: the manifest is the copy `manifest::read` made for the
            // process or a pack's manifest built for the process, its strings
            // and lists readable and never written; a skipped mod's DLL stays
            // loaded through start, and `meta` copies what it reads.
            label: unsafe { crate::base::manifest::meta(libraries[*i].manifest) }
                .label(&candidates[*i].id),
            file: libraries[*i].file.clone(),
            reason: reason.clone(),
        })
        .chain(copies)
        .collect()
}

/// The records of the mods `plan` loads, in load order.
fn records(
    libraries: &[Library],
    candidates: &[Candidate],
    plan: &resolve::Plan,
) -> Vec<ModRecord> {
    plan.order
        .iter()
        .map(|&i| ModRecord {
            id: candidates[i].id.clone(),
            version: candidates[i].version.to_string(),
            file: format!("mods/{}", libraries[i].file),
            manifest: libraries[i].manifest,
            // SAFETY: the manifest is the copy `manifest::read` made for the
            // process or a pack's manifest built for the process, its
            // strings and lists readable and never written; a string `read`
            // left in the DLL is read while the planned mod's DLL is loaded.
            meta: unsafe { crate::base::manifest::meta(libraries[i].manifest) },
            off: AtomicBool::new(false),
            off_reason: Mutex::new(None),
            waiting: AtomicBool::new(false),
            settings: crate::engine::loader::Kept::default(),
            server_values: Mutex::new(None),
            settings_files: Mutex::new(Vec::new()),
            checks: Mutex::new(Vec::new()),
            texts: Mutex::new(None),
            pack: libraries[i].pack,
            features: Mutex::new(Vec::new()),
        })
        .collect()
}

/// Names the mod or the loader in the crash report when a fault involves its
/// code.
fn attribute_crashes(loader: &Loader, order: &[&Library]) {
    let mut modules: Vec<(usize, String, usize, usize)> = order
        .iter()
        .zip(&loader.mods)
        .enumerate()
        .filter_map(|(index, (library, record))| {
            library.range.map(|(s, e)| (index, record.id.clone(), s, e))
        })
        .collect();
    let mut own: HMODULE = core::ptr::null_mut();
    // SAFETY: `attribute_crashes` is an address inside this DLL, `own` is a
    // writable local, and the unchanged refcount means nothing is to free.
    let found = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            (attribute_crashes as *const ()).cast(),
            &raw mut own,
        )
    };
    if found != 0
        && let Some((start, end)) = crate::engine::crash::range_of(own)
    {
        modules.push((
            (ModIndex::LOADER).get(),
            crate::engine::settings::LOADER_ID.to_owned(),
            start,
            end,
        ));
    }
    if let Some(game_dir) = loader.mods_dir.parent() {
        crate::engine::crash::install(
            &modules,
            &game_dir.join(crate::engine::log::LOGS),
            &loader.mods_dir,
        );
    }
}

/// Arms the hooks and code patches; game code changed since the last init
/// is put down to no mod, and what arming writes to none.
fn arm_watched(loader: &Loader, frame: Option<FrameSlot>, watching: Option<&mut Watching>) {
    match watching {
        Some(w) => {
            w.check("", None, Stage::BeforeArming);
            arm(loader, frame);
            w.absorb();
        }
        None => arm(loader, frame),
    }
}

/// Calls the `init` of every mod still on in load order, putting game code each one
/// changes outside the API down to it.
fn initialise(loader: &Loader, order: &[&Library], mut watching: Option<&mut Watching>) {
    for (index, library) in order.iter().enumerate() {
        let record = &loader.mods[index];
        if record.off.load(Ordering::Acquire) {
            continue;
        }
        if crate::base::developer_tools::refused(loader.developer_tools, record.manifest.flags)
            .is_some()
        {
            loader.turn_off(
                ModIndex::new(index),
                crate::base::developer_tools::off_reason(),
            );
            continue;
        }
        // SAFETY: `library.init` is the mod's `INIT` export, typed `InitFn`
        // as `enshwick_core::abi::exports` documents, called with the static host
        // table and the mod's own handle.
        let status = crate::engine::loader::initialising(index, || unsafe {
            (library.init)(&raw const host::API, host::handle(ModIndex::new(index)))
        });
        if let Some(w) = watching.as_deref_mut() {
            w.check(&record.id, library.range, Stage::Init);
        }
        if status.is_ok() {
            crate::log!(INFO, "{} {} initialised", record.id, record.version);
        } else {
            loader.turn_off(ModIndex::new(index), &format!("init returned {}", status.0));
        }
    }
}

/// Calls `ready` on every mod still on, putting game code each one changes
/// outside the API down to it.
fn ready(loader: &Loader, order: &[&Library], mut watching: Option<&mut Watching>) {
    for (index, library) in order.iter().enumerate() {
        if !loader.mods[index].off.load(Ordering::Acquire) {
            // SAFETY: `library.ready` is the mod's `READY` export, typed
            // `ReadyFn` as `enshwick_core::abi::exports` documents, called with the
            // static host table after the mod's init succeeded.
            unsafe { (library.ready)(&raw const host::API, host::handle(ModIndex::new(index))) };
            if let Some(w) = watching.as_deref_mut() {
                w.check(&loader.mods[index].id, library.range, Stage::Ready);
            }
        }
    }
    let running = loader
        .mods
        .iter()
        .filter(|m| !m.off.load(Ordering::Acquire))
        .count();
    crate::log!(
        INFO,
        "ready: {running} of {} mods running",
        loader.mods.len()
    );
    // Mods read vanilla resources at init; the chunks unpacked for them
    // are not needed after.
    crate::engine::archive::trim_game();
}

/// The caller frame the frame slot holds now.
fn read_frame() -> enshwick_core::abi::CallerFrame {
    FRAME
        .get()
        .copied()
        .flatten()
        .map(FrameSlot::read)
        .unwrap_or_default()
}
