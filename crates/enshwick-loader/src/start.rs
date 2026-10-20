//! The loader's start: identify the game, load the mods, arm their hooks.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use enshwick_api::abi::{ABI_MAJOR, HostApi, Manifest, ModHandle, Side, Status, exports};
use enshwick_reflect::{Image, Layout, Reflection};
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
use crate::{builds, log};

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
    fingerprint: enshwick_reflect::Fingerprint,
    build: &'static builds::KnownBuild,
    reflection: Reflection<'static>,
}

static FRAME: OnceLock<Option<FrameSlot>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

/// Runs once, on the stub's thread.
pub(crate) fn start(stub_abi: u32, missing_forwards: u32) {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let Some(mods_dir) = begin(stub_abi, missing_forwards) else {
        return;
    };
    let Some(game) = game() else {
        return;
    };
    let (libraries, candidates) = libraries(&mods_dir);
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
        data_dir: mods_dir.with_file_name("mod-data"),
        mods_dir,
        mods: plan
            .order
            .iter()
            .map(|&i| ModRecord {
                id: candidates[i].id.clone(),
                version: candidates[i].version.to_string(),
                file: format!("mods/{}", libraries[i].file),
                updates: updates_of(&libraries[i]),
                manifest: libraries[i].manifest,
                off: AtomicBool::new(false),
                settings: Mutex::new(None),
            })
            .collect(),
        chains: Mutex::new(Chains::default()),
        services: Mutex::default(),
        caller_frame: read_frame,
        subscriptions: Mutex::default(),
        delivery: OnceLock::new(),
    };
    let Some(loader) = host::install(loader) else {
        return;
    };
    let order: Vec<&Library> = plan.order.iter().map(|&i| &libraries[i]).collect();
    attribute_crashes(loader, &order);
    initialise(loader, &order);
    loader.seal_subscriptions();
    crate::content::seal_and_link(loader);
    if side == Side::SERVER {
        crate::events::link(loader);
    }
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
    ready(loader, &order);
    crate::watch::start(loader);
    if side == Side::SERVER
        && let Some(game_dir) = loader.mods_dir.parent()
    {
        crate::advertise::start(game_dir, &joined(loader));
    }
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
    let mut modules: Vec<(String, usize, usize)> = order
        .iter()
        .zip(&loader.mods)
        .filter_map(|(library, record)| library.range.map(|(s, e)| (record.id.clone(), s, e)))
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
        modules.push((crate::settings::LOADER_ID.to_owned(), start, end));
    }
    if let Some(game_dir) = loader.mods_dir.parent() {
        crate::crash::install(
            modules,
            game_dir
                .join("logs")
                .join(crate::settings::LOADER_ID)
                .join("crash.log"),
        );
    }
}

/// Links the loader's join check on the client's server change.
fn link_join(loader: &Loader) {
    let Some(rva) = loader.build.symbol("net.server_change") else {
        return;
    };
    let mods = joined(loader);
    crate::join::set_ours(
        crate::advertise::fingerprint(env!("CARGO_PKG_VERSION"), &mods),
        crate::advertise::mod_list(&mods),
    );
    let link = crate::chains::Link {
        owner: crate::chains::LOADER,
        detour: (crate::join::detour as *const ()).addr(),
        next: &crate::join::NEXT,
        priority: i32::MIN,
        flags: enshwick_api::abi::HookFlags::NONE,
    };
    if let Ok(mut chains) = loader.chains.lock() {
        let status = chains.add(loader.base + rva, link);
        if !status.is_ok() {
            crate::log!(WARN, "join check off: its hook was refused ({})", status.0);
        }
    }
}

/// The running mods that count in the join check, as `(id, version)`.
fn joined(loader: &Loader) -> Vec<(String, String)> {
    loader
        .mods
        .iter()
        .filter(|m| !m.off.load(Ordering::Acquire))
        .filter(|m| {
            m.manifest
                .flags
                .contains(enshwick_api::abi::ManifestFlags::JOIN)
        })
        .map(|m| (m.id.clone(), m.version.clone()))
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
fn game() -> Option<Game> {
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
        return None;
    };
    for &(name, rva) in build.checks {
        let entry = reflection
            .system(name)
            .map_or(0, enshwick_reflect::System::entry);
        if entry != base + rva {
            crate::log!(
                ERROR,
                "{name} is not where the build table says; nothing loads"
            );
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

/// The mods in `mods/` with readable manifests, and their manifests.
fn libraries(mods_dir: &Path) -> (Vec<Library>, Vec<Candidate>) {
    let mut libraries = Vec::new();
    let mut candidates = Vec::new();
    for library in discover(mods_dir) {
        // SAFETY: the manifest is static data inside its loaded DLL.
        match unsafe { crate::manifest::parse(library.manifest) } {
            Ok(candidate) => {
                candidates.push(candidate);
                libraries.push(library);
            }
            Err(reason) => crate::log!(WARN, "{} skipped: {reason}", library.file),
        }
    }
    (libraries, candidates)
}

/// The update source a library declares; none, logged, when it is malformed.
fn updates_of(library: &Library) -> enshwick_api::abi::UpdateSource {
    // SAFETY: the manifest is static data inside its loaded DLL.
    unsafe { crate::manifest::updates(library.manifest) }.unwrap_or_else(|reason| {
        crate::log!(WARN, "{}: no update source: {reason}", library.file);
        enshwick_api::abi::UpdateSource::NONE
    })
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

/// Arms every chain past the mods that are off; a target that fails turns off
/// the mods linked on it.
fn arm(loader: &Loader, frame: Option<FrameSlot>) {
    let off = |owner: usize| {
        owner != crate::chains::LOADER
            && loader
                .mods
                .get(owner)
                .is_none_or(|m| m.off.load(Ordering::Acquire))
    };
    let Ok(mut chains) = loader.chains.lock() else {
        return;
    };
    chains.seal();
    let failures = patch::arm(&mut chains, frame, off);
    let armed = chains.len() - failures.len();
    let owners: Vec<(usize, u64)> = failures
        .iter()
        .flat_map(|f| {
            chains
                .owners(f.target)
                .into_iter()
                .map(move |owner| (owner, f.target))
        })
        .collect();
    drop(chains);
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

fn read_frame() -> enshwick_api::abi::CallerFrame {
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
fn discover(mods_dir: &Path) -> Vec<Library> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll")))
        .collect();
    files.sort();
    files.iter().filter_map(|path| load(path)).collect()
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
    // SAFETY: each export has the signature `enshwick_api::abi::exports` documents.
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
    let manifest = unsafe { query().as_ref::<'static>() }?;
    Some(Library {
        file: file.to_owned(),
        range: crate::crash::range_of(module),
        manifest,
        init,
        ready,
    })
}
