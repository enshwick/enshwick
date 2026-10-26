//! The loader's start: identify the game, load the mods, arm their hooks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleHandleExW,
};

use crate::chains::Chains;
use crate::host::{self, Loader, ModRecord};
use crate::modlist;
use crate::patch::FrameSlot;
use crate::resolve::{self, Candidate};
use enshwick::abi::Side;

mod arm;
mod dry_run;
mod game;
mod join_check;
mod library;
mod lock;
mod pe;

pub use dry_run::{DryMod, DryRun, DryRunError, dry_run};
pub(crate) use join_check::TABLE_IDS;
pub(crate) use library::load_copy;

use arm::arm;
use game::{begin, game, wait_for_window};
use join_check::{link_join, names, pages};
use library::{Library, libraries, unload};
use lock::write_lock;

/// The client starts under Steam's DRM wrapper, which may check the game's code
/// before it runs. The game opens its window only once its own code runs, so
/// hooks wait for that window, and at most this long.
const CLIENT_ARM_DELAY: Duration = Duration::from_secs(15);
static FRAME: OnceLock<Option<FrameSlot>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

/// Runs once, on the stub's thread.
/// Seals what mods asked for and links the loader's own features; the
/// client links the server's too, for the games it hosts.
fn link_features(loader: &'static Loader) {
    crate::crash::link(loader);
    crate::events::link(loader);
    crate::world::link(loader);
    crate::vetoes::link(loader);
    crate::recipes::init(loader);
    crate::daytime::link(loader);
    crate::damage::link(loader);
    crate::deaths::link(loader);
    crate::knowledge::guard::link(loader);
    crate::weather::link(loader);
    crate::game_events::link(loader);
    crate::saves::link(loader);
    crate::inventory::link(loader);
    crate::item_data::link(loader);
    crate::net::link();
    crate::chat::link(loader);
    crate::label::link(loader);
    crate::altars::link(loader);
    crate::survivors::link(loader);
    crate::textures::link(loader);
    crate::interactions::link(loader);
    crate::progress::link(loader);
    crate::bases::link(loader);
    crate::ui::link(loader);
}

pub(crate) fn start(
    stub_abi: u32,
    missing_forwards: u32,
    release: Option<extern "system" fn() -> u32>,
) {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let began = std::time::Instant::now();
    let Some(mods_dir) = begin(stub_abi, missing_forwards) else {
        return;
    };
    let data_dir = mods_dir.with_file_name("mod-data");
    let Some(game) = game(&data_dir) else {
        return;
    };
    let (libraries, candidates) = libraries(&mods_dir);
    let lock_files: Vec<(String, String)> = libraries
        .iter()
        .zip(&candidates)
        .map(|(library, candidate)| (candidate.id.clone(), library.file.clone()))
        .collect();
    let plan = resolve::resolve(&candidates, game.side);
    skip(&libraries, &candidates, &plan);
    carry_former_ids(&mods_dir, &data_dir, &candidates, &plan);
    let frame = *FRAME.get_or_init(FrameSlot::allocate);
    let side = game.side;
    let loader = Loader {
        side,
        base: game.base,
        fingerprint: game.fingerprint,
        build: game.build,
        reflection: game.reflection,
        configs_dir: mods_dir.with_file_name("mod-configs"),
        data_dir,
        mods_dir,
        mods: records(&libraries, &candidates, &plan),
        chains: Mutex::new(Chains::default()),
        mids: Mutex::default(),
        caller_frame: read_frame,
        subscriptions: Mutex::default(),
        delivery: OnceLock::new(),
    };
    let Some(loader) = host::install(loader) else {
        return;
    };
    if crate::diag::counting() {
        crate::profile::start(loader.mods.len());
    }
    let order: Vec<&Library> = plan.order.iter().map(|&i| &libraries[i]).collect();
    // SAFETY: the loader's table lives for the process.
    unsafe {
        enshwick::__private::install_host(&raw const host::API, host::LOADER_HANDLE);
    }
    attribute_crashes(loader, &order);
    crate::reload::prepare(loader.mods.len());
    initialise(loader, &order);
    crate::stats::seal();
    crate::item_data::seal();
    loader.seal_subscriptions();
    crate::services::seal(loader);
    crate::points::seal(loader);
    crate::content::seal_and_link(loader);
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
    arm(loader, frame);
    release_server(release, began);
    crate::ui::armed(loader);
    let running: Vec<&str> = loader
        .mods
        .iter()
        .filter(|m| !m.off.load(Ordering::Acquire))
        .map(|m| m.id.as_str())
        .collect();
    crate::holds::enforce(&loader.data_dir, &running, false);
    ready(loader, &order);
    watch_copies(loader, &order);
    write_lock(loader, lock_files);
    crate::watch::start(loader);
    crate::status::start(loader);
    if side == Side::SERVER
        && let Some(game_dir) = loader.mods_dir.parent()
    {
        crate::advertise::start(
            game_dir,
            host::join_mods,
            crate::advertise::Lines {
                pages: pages(loader),
                names: names(loader),
            },
            host::join_values,
        );
    }
    crate::admission::start(
        loader,
        names(loader).into_iter().collect(),
        pages(loader).into_iter().collect(),
    );
}

/// Copies each loading mod's settings and data from the ids it had, once.
fn carry_former_ids(
    mods_dir: &Path,
    data_dir: &Path,
    candidates: &[Candidate],
    plan: &resolve::Plan,
) {
    let mods: Vec<(&str, Vec<&str>)> = plan
        .order
        .iter()
        .map(|&i| {
            let candidate = &candidates[i];
            (candidate.id.as_str(), candidate.former_ids().collect())
        })
        .collect();
    let configs = mods_dir.with_file_name("mod-configs");
    let dirs = crate::carry::Dirs {
        configs: &configs,
        data: data_dir,
    };
    for line in crate::carry::carry_all(dirs, &mods, crate::migrations::dry_run()) {
        log::write(line.level, &line.id, &line.text);
    }
}

/// Lets the dedicated server's own code run once the hooks are armed, and
/// logs whether the stub held it that long.
fn release_server(release: Option<extern "system" fn() -> u32>, began: std::time::Instant) {
    match release.map(|release| release()) {
        Some(1) => crate::log!(
            INFO,
            "the server's own code starts after the hooks, {} ms after the loader started",
            began.elapsed().as_millis()
        ),
        Some(2) => crate::log!(
            WARN,
            "the server's own code started before the hooks: the loader took {} ms, past the stub's hold",
            began.elapsed().as_millis()
        ),
        _ => {}
    }
}

/// Logs the mods `plan` skips; unloads the DLLs of those that present mods
/// replace, so their code never runs and the files can be deleted.
fn skip(libraries: &[Library], candidates: &[Candidate], plan: &resolve::Plan) {
    for (index, reason) in &plan.skipped {
        if plan.replaced.iter().any(|&(old, _)| old == *index) {
            continue;
        }
        if reason == resolve::OTHER_SIDE {
            crate::log!(INFO, "{} skipped: {reason}", libraries[*index].file);
        } else {
            crate::log!(WARN, "{} skipped: {reason}", libraries[*index].file);
        }
    }
    let replaced = plan
        .replaced
        .iter()
        .map(|&(old, by)| {
            let library = &libraries[old];
            unload(library);
            let replaced = modlist::Replaced {
                id: candidates[old].id.clone(),
                version: candidates[old].version.to_string(),
                file: library.file.clone(),
                by: candidates[by].id.clone(),
            };
            crate::log!(WARN, "{}", replaced.line());
            replaced
        })
        .collect();
    modlist::set_replaced(replaced);
}

/// With dev reload, watches the DLLs of the mods that run from copies.
fn watch_copies(loader: &'static Loader, order: &[&Library]) {
    if !crate::reload::copies() {
        return;
    }
    let copied = order
        .iter()
        .enumerate()
        .filter_map(|(index, library)| {
            let (original, copy) = library.copied.clone()?;
            Some((index, original, copy, library.module))
        })
        .collect();
    crate::reload::start(loader, copied);
}

/// The records of the mods `plan` loads, in load order, with their notes
/// logged.
fn records(
    libraries: &[Library],
    candidates: &[Candidate],
    plan: &resolve::Plan,
) -> Vec<ModRecord> {
    let notes = resolve::notes(candidates, plan);
    for (index, note) in &notes {
        crate::log!(INFO, "{} {note}", candidates[*index].id);
    }
    plan.order
        .iter()
        .map(|&i| ModRecord {
            id: candidates[i].id.clone(),
            version: candidates[i].version.to_string(),
            file: format!("mods/{}", libraries[i].file),
            manifest: libraries[i].manifest,
            // SAFETY: the manifest is static data inside its loaded DLL.
            meta: unsafe { crate::manifest::meta(libraries[i].manifest) },
            former: candidates[i].former_ids().map(str::to_owned).collect(),
            notes: notes
                .iter()
                .filter(|(index, _)| *index == i)
                .map(|(_, note)| note.clone())
                .collect(),
            off: AtomicBool::new(false),
            settings: host::Kept::default(),
            synced: Mutex::new(None),
            server_synced: Mutex::new(None),
            sync_out: host::Kept::default(),
            schema: Mutex::new(None),
            check: OnceLock::new(),
            texts: Mutex::new(None),
            pack: libraries[i].pack,
        })
        .collect()
}

/// Names the mod or the loader in `crash.log` when a fault involves its code.
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
    // SAFETY: an address inside this DLL; the handle is not counted.
    let found = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            (attribute_crashes as *const ()).cast(),
            &raw mut own,
        )
    };
    if found != 0
        && let Some((start, end)) = crate::crash::range_of(own)
    {
        modules.push((
            crate::crash::LOADER,
            crate::settings::LOADER_ID.to_owned(),
            start,
            end,
        ));
    }
    if let Some(game_dir) = loader.mods_dir.parent() {
        crate::crash::install(
            &modules,
            &game_dir.join("logs").join(crate::settings::LOADER_ID),
            &loader.data_dir,
        );
    }
}

/// Calls every mod's `init` in load order.
fn initialise(loader: &Loader, order: &[&Library]) {
    for (index, library) in order.iter().enumerate() {
        let record = &loader.mods[index];
        // SAFETY: the mod exports `init` with this signature; the table lives
        // for the process.
        let status = crate::reload::initialising(index, || unsafe {
            (library.init)(&raw const host::API, host::handle(index))
        });
        if status.is_ok() {
            crate::log!(INFO, "{} {} initialised", record.id, record.version);
        } else {
            loader.turn_off(index, &format!("init returned {}", status.0));
        }
    }
}

/// Calls `ready` on every mod still on.
fn ready(loader: &Loader, order: &[&Library]) {
    for (index, library) in order.iter().enumerate() {
        if !loader.mods[index].off.load(Ordering::Acquire) {
            // SAFETY: the mod exports `ready` with this signature.
            unsafe { (library.ready)(&raw const host::API, host::handle(index)) };
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
    crate::archive::trim_game();
}

/// The caller frame the frame slot holds now.
fn read_frame() -> enshwick::abi::CallerFrame {
    FRAME
        .get()
        .copied()
        .flatten()
        .map(FrameSlot::read)
        .unwrap_or_default()
}
