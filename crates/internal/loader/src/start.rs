//! The loader's start: identify the game, load the mods, arm their hooks.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use enshwick_loader_api::abi::{ABI_MAJOR, HostApi, Manifest, ModHandle, Side, Status, exports};
use enshwick_loader_reflect::{Image, Layout, Reflection};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW, GetModuleHandleW, GetProcAddress,
    LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW,
};

use crate::chains::Chains;
use crate::host::{self, Loader, ModRecord};
use crate::patch::{self, FrameSlot};
use crate::resolve::{self, Candidate};
use crate::{builds, log, modlist};

/// The client starts under Steam's DRM wrapper, which may check the game's code
/// before it runs. The game opens its window only once its own code runs, so
/// hooks wait for that window, and at most this long.
const CLIENT_ARM_DELAY: Duration = Duration::from_secs(15);
/// How often the client looks for its window.
const WINDOW_POLL: Duration = Duration::from_millis(100);

/// The stub contract the loader speaks.
const STUB_ABI: u32 = 1;

type AbiFn = extern "system" fn() -> u32;
type QueryFn = extern "system" fn() -> *const Manifest;
type InitFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;
type ReadyFn = unsafe extern "system" fn(*const HostApi, ModHandle);
type Export = unsafe extern "system" fn() -> isize;

/// A mod DLL with its exports.
struct Library {
    file: String,
    /// The loaded DLL, freed again when the mod list turns its id off.
    module: HMODULE,
    /// Where the DLL is mapped, for crash attribution.
    range: Option<(usize, usize)>,
    manifest: &'static Manifest,
    init: InitFn,
    ready: ReadyFn,
}

/// The game as the loader identified it.
struct Game {
    side: Side,
    base: u64,
    fingerprint: enshwick_loader_reflect::Fingerprint,
    build: &'static builds::KnownBuild,
    reflection: Reflection<'static>,
}

static FRAME: OnceLock<Option<FrameSlot>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

/// Runs once, on the stub's thread.
/// Seals what mods asked for and links the loader's own features; the
/// client links the server's too, for the games it hosts.
fn link_features(loader: &'static Loader) {
    crate::crash::link(loader);
    crate::events::link(loader);
    crate::vetoes::link(loader);
    crate::daytime::link(loader);
    crate::damage::link(loader);
    crate::weather::link(loader);
    crate::game_events::link(loader);
    crate::saves::link(loader);
    crate::inventory::link(loader);
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

pub(crate) fn start(stub_abi: u32, missing_forwards: u32) {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
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
    for (index, reason) in &plan.skipped {
        crate::log!(WARN, "{} skipped: {reason}", libraries[*index].file);
    }
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
    attribute_crashes(loader, &order);
    initialise(loader, &order);
    crate::stats::seal();
    crate::item_data::seal();
    loader.seal_subscriptions();
    crate::services::seal(loader);
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
    crate::ui::armed(loader);
    let running: Vec<&str> = loader
        .mods
        .iter()
        .filter(|m| !m.off.load(Ordering::Acquire))
        .map(|m| m.id.as_str())
        .collect();
    crate::holds::enforce(&loader.data_dir, &running, false);
    ready(loader, &order);
    write_lock(loader, lock_files);
    crate::watch::start(loader);
    crate::status::start(loader);
    if side == Side::SERVER
        && let Some(game_dir) = loader.mods_dir.parent()
    {
        crate::advertise::start(
            game_dir,
            joined(loader),
            crate::advertise::Lines {
                sources: sources(loader),
                keys: keys(loader),
                names: names(loader),
            },
            host::join_values,
        );
    }
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
            updates: updates_of(&libraries[i]),
            signed_by: signed_by_of(&libraries[i]),
            manifest: libraries[i].manifest,
            // SAFETY: the manifest is static data inside its loaded DLL.
            meta: unsafe { crate::manifest::meta(libraries[i].manifest) },
            notes: notes
                .iter()
                .filter(|(index, _)| *index == i)
                .map(|(_, note)| note.clone())
                .collect(),
            off: AtomicBool::new(false),
            settings: Mutex::new(None),
            synced: Mutex::new(None),
            server_synced: Mutex::new(None),
            sync_out: Mutex::new(String::new()),
            schema: Mutex::new(None),
            check: OnceLock::new(),
            texts: Mutex::new(None),
        })
        .collect()
}

/// Waits until this process shows a window or `limit` passes; how long it
/// waited.
fn wait_for_window(limit: Duration) -> Duration {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn ours(window: HWND, found: LPARAM) -> i32 {
        let mut process = 0u32;
        // SAFETY: a window Windows enumerates; the out word is ours.
        unsafe { GetWindowThreadProcessId(window, &raw mut process) };
        // SAFETY: as above.
        let visible = unsafe { IsWindowVisible(window) } != 0;
        // SAFETY: no preconditions.
        let me = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
        if visible && process == me {
            let flag = core::ptr::with_exposed_provenance_mut::<bool>(found.cast_unsigned());
            // SAFETY: `found` is the address of the caller's flag.
            unsafe { flag.write(true) };
            return 0;
        }
        1
    }

    let started = std::time::Instant::now();
    while started.elapsed() < limit {
        let mut found = false;
        // SAFETY: the callback only writes the flag, which outlives the call.
        unsafe {
            EnumWindows(
                Some(ours),
                (&raw mut found).expose_provenance().cast_signed(),
            )
        };
        if found {
            break;
        }
        std::thread::sleep(WINDOW_POLL);
    }
    started.elapsed()
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

/// Links the loader's join check on the client's server change.
fn link_join(loader: &Loader) {
    let Some(rva) = loader.build.symbol("net.server_change") else {
        return;
    };
    let names = loader
        .mods
        .iter()
        .filter(|m| !m.meta.name.is_empty())
        .map(|m| (m.id.clone(), m.meta.name.clone()))
        .collect();
    crate::join::set_ours(joined(loader), names);
    let status = loader.link(
        loader.base + rva,
        &crate::join::NEXT,
        crate::join::detour,
        enshwick_loader_api::abi::HookFlags::NONE,
    );
    if !status.is_ok() {
        crate::log!(WARN, "join check off: its hook was refused ({})", status.0);
    }
}

/// The running mods that count in the join check, as `(id, version)`.
fn joined(loader: &Loader) -> Vec<(String, String)> {
    loader
        .mods
        .iter()
        .filter(|m| m.joins())
        .map(|m| (m.id.clone(), m.version.clone()))
        .collect()
}

/// The update sources of the running join mods, `(id, source line)`.
fn sources(loader: &Loader) -> Vec<(String, String)> {
    loader
        .mods
        .iter()
        .filter(|m| m.joins())
        .filter_map(|m| {
            // SAFETY: the loader keeps every source's strings for the process.
            let updates = unsafe { enshwick_loader_api::mods::Updates::from_abi(&m.updates) }?;
            Some((m.id.clone(), updates.to_line()))
        })
        .collect()
}

/// The author keys of the running join mods that declare any, `(id, keys
/// line)`.
fn keys(loader: &Loader) -> Vec<(String, String)> {
    loader
        .mods
        .iter()
        .filter(|m| m.joins())
        .filter_map(|m| {
            // SAFETY: the loader keeps every key's string for the process.
            let keys = unsafe { enshwick_loader_api::mods::keys_from_abi(&m.signed_by) };
            (!keys.is_empty()).then(|| (m.id.clone(), enshwick_loader_api::mods::keys_line(&keys)))
        })
        .collect()
}

/// The names of the running join mods that declare one, `(id, name)`.
fn names(loader: &Loader) -> Vec<(String, String)> {
    loader
        .mods
        .iter()
        .filter(|m| m.joins() && !m.meta.name.is_empty())
        .map(|m| (m.id.clone(), m.meta.name.clone()))
        .collect()
}

/// Creates `mods/` and opens the log; `None` when the folder cannot be made.
fn begin(stub_abi: u32, missing_forwards: u32) -> Option<PathBuf> {
    let game_dir = game_dir()?;
    let mods_dir = game_dir.join("mods");
    for dir in ["mods", "mod-configs", "mod-data", "logs"] {
        std::fs::create_dir_all(game_dir.join(dir)).ok()?;
    }
    let settings_file = game_dir
        .join("mod-configs")
        .join(format!("{}.toml", crate::settings::LOADER_ID));
    let (settings, problem) = crate::settings::Settings::read(&settings_file);
    crate::status::mark_start();
    crate::diag::configure(&settings);
    log::set_levels(settings);
    log::open(&game_dir.join("logs").join(crate::settings::LOADER_ID));
    crate::log!(INFO, "Enshwick {} starting", env!("CARGO_PKG_VERSION"));
    if let Some(problem) = problem {
        crate::log!(WARN, "loader settings left at their defaults: {problem}");
    }
    if stub_abi != STUB_ABI {
        crate::log!(
            WARN,
            "stub contract {stub_abi}, the loader speaks {STUB_ABI}"
        );
    }
    if missing_forwards == u32::MAX {
        crate::log!(WARN, "the system powrprof.dll could not be loaded");
    } else if missing_forwards > 0 {
        crate::log!(
            INFO,
            "{missing_forwards} powrprof.dll exports are missing on this system"
        );
    }
    Some(mods_dir)
}

/// Identifies the running game; `None`, logged, when the loader cannot serve it.
/// On a build it does not know, a mod's hold under `data_dir` stops the game.
fn game(data_dir: &Path) -> Option<Game> {
    let Some((side, base, image_len)) = identify() else {
        crate::log!(
            ERROR,
            "this is not Enshrouded or its dedicated server; nothing loads"
        );
        return None;
    };
    // SAFETY: the game's image stays mapped for the life of the process.
    let bytes: &'static [u8] = unsafe { core::slice::from_raw_parts(base as *const u8, image_len) };
    let Ok(image) = Image::parse(bytes, Layout::Mapped, Some(base)) else {
        crate::log!(ERROR, "the game's image could not be read; nothing loads");
        return None;
    };
    let image: &'static Image<'static> = Box::leak(Box::new(image));
    let Some(fingerprint) = image.fingerprint() else {
        crate::log!(ERROR, "the game build has no fingerprint; nothing loads");
        return None;
    };
    let reflection = Reflection::scan(image);
    crate::log!(
        INFO,
        "{} types and {} systems reflected",
        reflection.type_count(),
        reflection.system_count()
    );
    let Some(build) = builds::find(side, fingerprint) else {
        crate::log!(
            ERROR,
            "unknown game build {}/{}; the game runs vanilla until Enshwick supports it",
            hex(&fingerprint.guid()),
            fingerprint.age()
        );
        crate::holds::enforce(data_dir, &[], true);
        return None;
    };
    for &(name, rva) in build.checks {
        let entry = reflection
            .system(name)
            .map_or(0, enshwick_loader_reflect::System::entry);
        if entry != base + rva {
            crate::log!(
                ERROR,
                "{name} is not where the build table says; nothing loads"
            );
            crate::holds::enforce(data_dir, &[], true);
            return None;
        }
    }
    crate::log!(INFO, "game: {}", build.name);
    Some(Game {
        side,
        base,
        fingerprint,
        build,
        reflection,
    })
}

/// The mods in `mods/` with readable manifests that the mod list leaves on,
/// and their manifests; the list gains the mods it lacked.
fn libraries(mods_dir: &Path) -> (Vec<Library>, Vec<Candidate>) {
    let list_file = mods_dir
        .join(crate::settings::LOADER_ID)
        .join(modlist::LIST_FILE);
    let (mut list, writable) = match std::fs::read_to_string(&list_file) {
        Ok(text) => match modlist::ModList::parse(&text) {
            Ok(list) => (list, true),
            Err(reason) => {
                crate::log!(
                    WARN,
                    "{} is not read, every mod is on: {reason}",
                    modlist::LIST_PATH
                );
                (modlist::ModList::default(), false)
            }
        },
        Err(_) => (modlist::ModList::default(), true),
    };
    let mut changed = !list_file.exists();
    let mut libraries = Vec::new();
    let mut candidates = Vec::new();
    for library in discover(mods_dir, &list) {
        // SAFETY: the manifest is static data inside its loaded DLL.
        match unsafe { crate::manifest::parse(library.manifest) } {
            Ok(candidate) => {
                changed |= list.note(&candidate.id, &library.file);
                if list.is_off(&candidate.id) {
                    crate::log!(
                        INFO,
                        "{} skipped: {} is off in {}",
                        library.file,
                        candidate.id,
                        modlist::LIST_PATH
                    );
                    // SAFETY: loaded by `discover`; none of its code is in use.
                    unsafe { FreeLibrary(library.module) };
                    continue;
                }
                candidates.push(candidate);
                libraries.push(library);
            }
            Err(reason) => crate::log!(WARN, "{} skipped: {reason}", library.file),
        }
    }
    modlist::set_disabled(list.off());
    if changed
        && writable
        && let Err(e) = std::fs::write(&list_file, list.render())
    {
        crate::log!(WARN, "{} not written: {e}", modlist::LIST_PATH);
    }
    (libraries, candidates)
}

/// Writes `enshwick.lock` on a thread of its own: the loader and the mods
/// that run, `files` naming each mod's DLL by id; logs what changed since
/// the last start.
fn write_lock(loader: &'static Loader, files: Vec<(String, String)>) {
    let spawned = std::thread::Builder::new()
        .name("enshwick-lock".into())
        .spawn(move || {
            let dir = loader.mods_dir.join(crate::settings::LOADER_ID);
            let hash = |path: &Path| {
                std::fs::read(path)
                    .map(|bytes| modlist::sha256(&bytes))
                    .unwrap_or_default()
            };
            let mods = loader
                .mods
                .iter()
                .filter(|m| !m.off.load(Ordering::Acquire))
                .filter_map(|m| {
                    let (_, file) = files.iter().find(|(id, _)| *id == m.id)?;
                    Some(modlist::Locked {
                        id: m.id.clone(),
                        version: m.version.clone(),
                        file: file.clone(),
                        sha256: hash(&loader.mods_dir.join(file)),
                    })
                })
                .collect();
            let lock = modlist::Lock {
                loader: env!("CARGO_PKG_VERSION").to_owned(),
                loader_sha256: hash(&dir.join(format!("{}.dll", crate::settings::LOADER_ID))),
                game: loader.build.name.to_owned(),
                mods,
            };
            let path = dir.join(modlist::LOCK_FILE);
            let old = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| modlist::Lock::parse(&text));
            if let Some(old) = &old {
                for change in lock.changes(old) {
                    crate::log!(INFO, "since the last start: {change}");
                }
            }
            if old.as_ref() != Some(&lock)
                && let Err(e) = std::fs::write(&path, lock.render())
            {
                crate::log!(WARN, "{} not written: {e}", modlist::LOCK_FILE);
            }
        });
    if let Err(e) = spawned {
        crate::log!(WARN, "{} not written: {e}", modlist::LOCK_FILE);
    }
}

/// The update source a library declares; none, logged, when it is malformed.
fn updates_of(library: &Library) -> enshwick_loader_api::abi::UpdateSource {
    // SAFETY: the manifest is static data inside its loaded DLL.
    unsafe { crate::manifest::updates(library.manifest) }.unwrap_or_else(|reason| {
        crate::log!(WARN, "{}: no update source: {reason}", library.file);
        enshwick_loader_api::abi::UpdateSource::NONE
    })
}

/// The author keys a library declares; a malformed one is left out, logged.
fn signed_by_of(library: &Library) -> [enshwick_loader_api::abi::Str; 2] {
    // SAFETY: the manifest is static data inside its loaded DLL.
    let (keys, refused) = unsafe { crate::manifest::signed_by(library.manifest) };
    for reason in refused {
        crate::log!(WARN, "{}: key ignored: {reason}", library.file);
    }
    keys
}

/// Calls every mod's `init` in load order.
fn initialise(loader: &Loader, order: &[&Library]) {
    for (index, library) in order.iter().enumerate() {
        let record = &loader.mods[index];
        // SAFETY: the mod exports `init` with this signature; the table lives
        // for the process.
        let status = unsafe { (library.init)(&raw const host::API, host::handle(index)) };
        if status.is_ok() {
            crate::log!(INFO, "{} {} initialised", record.id, record.version);
        } else {
            loader.turn_off(index, &format!("init returned {}", status.0));
        }
    }
}

/// Arms every chain and mid-function hook past the mods that are off; an
/// address that fails turns off the mods hooked on it.
fn arm(loader: &Loader, frame: Option<FrameSlot>) {
    let (Ok(mut chains), Ok(mut mids)) = (loader.chains.lock(), loader.mids.lock()) else {
        return;
    };
    chains.seal();
    mids.seal();
    let patches = crate::code::seal();
    let patch::Armed {
        failures,
        code: verdicts,
    } = patch::arm(
        &mut chains,
        &mids,
        frame,
        host::is_off,
        &patches,
        patch::Count::from_settings(),
    );
    let armed = (chains.len() + mids.len()).saturating_sub(failures.len());
    let owners: Vec<(usize, u64)> = failures
        .iter()
        .flat_map(|f| {
            let owners = if f.inside {
                mids.owners(f.target)
            } else {
                chains.owners(f.target)
            };
            owners.into_iter().map(move |owner| (owner, f.target))
        })
        .collect();
    drop(mids);
    drop(chains);
    crate::inventory::settle(
        loader,
        failures.iter().filter(|f| f.inside).map(|f| f.target),
    );
    for failure in &failures {
        crate::log!(
            ERROR,
            "hook at {:#x} not armed: {}",
            failure.target,
            failure.reason
        );
    }
    for (owner, target) in owners {
        loader.turn_off(
            owner,
            &format!("its hook at {target:#x} could not be armed"),
        );
    }
    crate::log!(INFO, "{armed} hook targets armed");
    if crate::diag::timing() {
        crate::log!(INFO, "diagnostics on: hook calls counted, one in 256 timed");
    } else if crate::diag::counting() {
        crate::log!(INFO, "diagnostics on: hook calls counted");
    }
    report_patches(loader, &patches, &verdicts);
}

/// Logs what became of each code patch and turns off the owners of skipped
/// ones.
fn report_patches(
    loader: &Loader,
    patches: &[crate::code::CodePatch],
    verdicts: &[crate::code::Verdict],
) {
    let mut tally = crate::diag::Patches::default();
    for verdict in verdicts {
        match verdict {
            crate::code::Verdict::Write => tally.written += 1,
            crate::code::Verdict::Off => tally.off += 1,
            crate::code::Verdict::Skip(_) => tally.skipped += 1,
        }
    }
    crate::diag::record_patches(tally);
    for (patch, verdict) in patches.iter().zip(verdicts) {
        let owner = loader
            .mods
            .get(patch.owner)
            .map_or(crate::settings::LOADER_ID, |m| m.id.as_str());
        let len = patch.replacement.len();
        match verdict {
            crate::code::Verdict::Write => crate::log!(
                INFO,
                "code patch of {owner} at {:#x} written, {len} bytes",
                patch.address
            ),
            crate::code::Verdict::Off => crate::log!(
                INFO,
                "code patch of {owner} at {:#x} left out: the mod is off",
                patch.address
            ),
            crate::code::Verdict::Skip(reason) => {
                crate::log!(
                    ERROR,
                    "code patch of {owner} at {:#x} skipped: {reason}",
                    patch.address
                );
                loader.turn_off(
                    patch.owner,
                    &format!("its code patch at {:#x} was skipped", patch.address),
                );
            }
        }
    }
    if crate::code::lifted() {
        crate::log!(INFO, "workshop recipe limit lifted");
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
}

fn read_frame() -> enshwick_loader_api::abi::CallerFrame {
    FRAME
        .get()
        .copied()
        .flatten()
        .map(FrameSlot::read)
        .unwrap_or_default()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _written = write!(out, "{b:02x}");
            out
        })
}

/// The path of the game's executable.
fn exe_path() -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    #[expect(clippy::cast_possible_truncation, reason = "the buffer is 1024 units")]
    // SAFETY: the buffer and its length match; a null module is the executable.
    let len = unsafe {
        GetModuleFileNameW(
            core::ptr::null_mut(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    if len == 0 || len >= buffer.len() {
        return None;
    }
    String::from_utf16(&buffer[..len]).ok()
}

/// The folder of the game's executable.
fn game_dir() -> Option<PathBuf> {
    PathBuf::from(exe_path()?).parent().map(Path::to_path_buf)
}

/// The side, image base and image size of the running game.
fn identify() -> Option<(Side, u64, usize)> {
    let exe = exe_path()?;
    let name = exe.rsplit('\\').next()?.to_ascii_lowercase();
    let side = match name.as_str() {
        "enshrouded_server.exe" => Side::SERVER,
        "enshrouded.exe" => Side::CLIENT,
        _ => return None,
    };
    // SAFETY: a null name is the executable's module.
    let module = unsafe { GetModuleHandleW(core::ptr::null()) };
    if module.is_null() {
        return None;
    }
    let base = module.addr();
    // SAFETY: the executable's headers are mapped at its base.
    let header = unsafe { core::slice::from_raw_parts(base as *const u8, 0x400) };
    let pe = u32::from_le_bytes(header.get(0x3c..0x40)?.try_into().ok()?) as usize;
    let size_of_image = header.get(pe + 24 + 56..pe + 24 + 60)?;
    let len = u32::from_le_bytes(size_of_image.try_into().ok()?) as usize;
    Some((side, u64::try_from(base).ok()?, len))
}

/// Loads every `mods/*.dll` that exports the Enshwick interface.
fn discover(mods_dir: &Path, list: &modlist::ModList) -> Vec<Library> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll")))
        .collect();
    files.sort();
    files
        .iter()
        .filter(|path| {
            let file = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            let off = list.is_file_off(&file);
            if off {
                crate::log!(INFO, "{file} skipped: off in {}", modlist::LIST_PATH);
            }
            !off
        })
        .filter_map(|path| load(path))
        .collect()
}

fn load(path: &Path) -> Option<Library> {
    let file = path.file_name()?.to_string_lossy().into_owned();
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain([0])
        .collect();
    // SAFETY: `wide` is NUL-terminated.
    let module = unsafe {
        LoadLibraryExW(
            wide.as_ptr(),
            core::ptr::null_mut(),
            LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    };
    if module.is_null() {
        crate::log!(WARN, "{file} could not be loaded");
        return None;
    }
    let library = exports_of(module, &file);
    if library.is_none() {
        // SAFETY: the module was loaded above and nothing of it is in use.
        unsafe { FreeLibrary(module) };
    }
    library
}

fn exports_of(module: HMODULE, file: &str) -> Option<Library> {
    let find = |name: &str| {
        let name = format!("{name}\0");
        // SAFETY: `module` is loaded and `name` is NUL-terminated.
        unsafe { GetProcAddress(module, name.as_ptr()) }
    };
    let (Some(abi), Some(query), Some(init), Some(ready)) = (
        find(exports::ABI),
        find(exports::QUERY),
        find(exports::INIT),
        find(exports::READY),
    ) else {
        crate::log!(WARN, "{file} is not an Enshwick mod");
        return None;
    };
    // SAFETY: each export has the signature `enshwick_loader_api::abi::exports` documents.
    let abi = unsafe { core::mem::transmute::<Export, AbiFn>(abi) };
    // SAFETY: as above.
    let query = unsafe { core::mem::transmute::<Export, QueryFn>(query) };
    // SAFETY: as above.
    let init = unsafe { core::mem::transmute::<Export, InitFn>(init) };
    // SAFETY: as above.
    let ready = unsafe { core::mem::transmute::<Export, ReadyFn>(ready) };
    if abi() != u32::from(ABI_MAJOR) {
        crate::log!(
            WARN,
            "{file} is built for ABI {}, the loader speaks {ABI_MAJOR}",
            abi()
        );
        return None;
    }
    // SAFETY: the manifest is static data inside the DLL, which stays loaded.
    let manifest = match unsafe { crate::manifest::read(query()) } {
        Ok(manifest) => manifest,
        Err(reason) => {
            crate::log!(WARN, "{file} skipped: {reason}");
            return None;
        }
    };
    Some(Library {
        file: file.to_owned(),
        module,
        range: crate::crash::range_of(module),
        manifest,
        init,
        ready,
    })
}
