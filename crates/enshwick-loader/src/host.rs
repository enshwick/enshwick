//! The loader's side of the API: its state and the `HostApi` table.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_api::abi as api_frame;
use enshwick_api::abi::{
    BuildInfo, Header, HookSpec, HostApi, Level, Manifest, ModHandle, Side, Status, Str,
};
use enshwick_reflect::{Fingerprint, Reflection};
use rustc_hash::FxHashMap;

use crate::builds::KnownBuild;
use crate::chains::{Chains, Link};

/// A loaded mod.
#[derive(Debug)]
pub(crate) struct ModRecord {
    /// The mod id.
    pub(crate) id: String,
    /// Its version.
    pub(crate) version: String,
    /// Its static manifest, inside its DLL.
    pub(crate) manifest: &'static Manifest,
    /// Set when the mod is off: refused at init or faulted later.
    pub(crate) off: AtomicBool,
    /// The settings text, kept alive while the mod initialises.
    pub(crate) settings: Mutex<Option<String>>,
}

/// Everything the loader knows once mods are resolved.
#[derive(Debug)]
pub(crate) struct Loader {
    /// Server or client.
    pub(crate) side: Side,
    /// The game's image base.
    pub(crate) base: u64,
    /// The game build.
    pub(crate) fingerprint: Fingerprint,
    /// The build table row.
    pub(crate) build: &'static KnownBuild,
    /// The game's reflection.
    pub(crate) reflection: Reflection<'static>,
    /// The `mods` folder.
    pub(crate) mods_dir: PathBuf,
    /// The `mod-configs` folder.
    pub(crate) configs_dir: PathBuf,
    /// The `mod-data` folder.
    pub(crate) data_dir: PathBuf,
    /// Loaded mods, in load order; handle `n` is index `n - 1`.
    pub(crate) mods: Vec<ModRecord>,
    /// Every hook chain.
    pub(crate) chains: Mutex<Chains>,
    /// Published services: `(name, version)` to `(owner, table)`.
    pub(crate) services: Mutex<FxHashMap<(String, u32), (usize, usize)>>,
    /// Reads the caller frame of the running chain.
    pub(crate) caller_frame: fn() -> api_frame::CallerFrame,
}

static LOADER: OnceLock<Loader> = OnceLock::new();

/// Makes `loader` the one the API table answers for.
pub(crate) fn install(loader: Loader) -> Option<&'static Loader> {
    LOADER.set(loader).ok()?;
    LOADER.get()
}

fn loader() -> Option<&'static Loader> {
    LOADER.get()
}

impl Loader {
    fn record(&self, me: ModHandle) -> Option<(usize, &ModRecord)> {
        let index = me.0.checked_sub(1)?;
        self.mods.get(index).map(|record| (index, record))
    }

    /// Turns a mod off: unlinks its hooks and logs why.
    pub(crate) fn turn_off(&self, index: usize, reason: &str) {
        let Some(record) = self.mods.get(index) else {
            return;
        };
        if record.off.swap(true, Ordering::AcqRel) {
            return;
        }
        crate::log::write(Level::ERROR, &record.id, &format!("off: {reason}"));
        if let Ok(chains) = self.chains.lock() {
            chains.rewire(|owner| {
                owner != crate::chains::LOADER
                    && self
                        .mods
                        .get(owner)
                        .is_none_or(|m| m.off.load(Ordering::Acquire))
            });
        }
    }

    /// The mod's own build rows that match the running build.
    fn own_rows(
        &self,
        index: usize,
    ) -> impl Iterator<Item = &'static enshwick_api::abi::BuildRow> + '_ {
        let manifest = self.mods.get(index).map(|record| record.manifest);
        let rows: &'static [enshwick_api::abi::BuildRow] = match manifest {
            Some(m)
                if m.header
                    .covers(core::mem::offset_of!(Manifest, build_count) + 8)
                    && !m.builds.is_null() =>
            {
                // SAFETY: the manifest points at `build_count` static rows.
                unsafe { core::slice::from_raw_parts(m.builds, m.build_count) }
            }
            _ => &[],
        };
        rows.iter().filter(|row| {
            row.side == self.side
                && row.fingerprint.guid == self.fingerprint.guid()
                && row.fingerprint.age == self.fingerprint.age()
        })
    }

    fn resolve_symbol(&self, index: usize, name: &str) -> Option<u64> {
        let own = self.own_rows(index).find_map(|row| {
            // SAFETY: the row points at `symbol_count` static symbols.
            let symbols = unsafe { slice(row.symbols, row.symbol_count) };
            find(symbols, name)
        });
        own.or_else(|| self.build.symbol(name))
            .map(|rva| self.base + rva)
    }

    fn resolve_constant(&self, index: usize, name: &str) -> Option<u64> {
        let own = self.own_rows(index).find_map(|row| {
            // SAFETY: the row points at `constant_count` static constants.
            let constants = unsafe { slice(row.constants, row.constant_count) };
            find(constants, name)
        });
        own.or_else(|| self.build.constant(name))
    }
}

/// The table handed to every mod.
pub(crate) static API: HostApi = HostApi {
    header: Header::of::<HostApi>(),
    host_version: Str::new(env!("CARGO_PKG_VERSION")),
    build,
    symbol,
    system,
    field_offset,
    type_size,
    hook,
    caller_frame,
    fault,
    log,
    settings,
    publish,
    service,
    constant,
    data_dir,
};

/// A static slice from a pointer and a count, empty for a null pointer.
///
/// # Safety
///
/// A non-null `ptr` points at `count` static values.
unsafe fn slice<T>(ptr: *const T, count: usize) -> &'static [T] {
    if ptr.is_null() || count == 0 {
        return &[];
    }
    // SAFETY: forwarded from the caller.
    unsafe { core::slice::from_raw_parts(ptr, count) }
}

/// The value of the symbol named `name`.
fn find(symbols: &[enshwick_api::abi::Symbol], name: &str) -> Option<u64> {
    symbols
        .iter()
        .find_map(|symbol| (text(symbol.name) == Some(name)).then_some(symbol.rva))
}

/// Reads a borrowed string, or `None` when it is not UTF-8.
fn text<'a>(value: Str) -> Option<&'a str> {
    // SAFETY: every `Str` a mod passes is borrowed for the call, which outlives
    // this use.
    unsafe { value.as_str() }
}

/// Writes `value` through `out` when `out` is not null.
fn put<T>(out: *mut T, value: T) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: a non-null out pointer from a mod points at a writable `T`.
    unsafe { out.write(value) };
    Status::OK
}

extern "system" fn build(out: *mut BuildInfo) {
    let Some(loader) = loader() else {
        return;
    };
    let _written = put(
        out,
        BuildInfo {
            header: Header::of::<BuildInfo>(),
            side: loader.side,
            base: loader.base,
            fingerprint: enshwick_api::abi::Fingerprint {
                guid: loader.fingerprint.guid(),
                age: loader.fingerprint.age(),
            },
        },
    );
}

extern "system" fn symbol(me: ModHandle, name: Str, out: *mut u64) -> Status {
    let (Some(loader), Some(name)) = (loader(), text(name)) else {
        return Status::BAD_ARGS;
    };
    let Some((index, _)) = loader.record(me) else {
        return Status::BAD_ARGS;
    };
    loader
        .resolve_symbol(index, name)
        .map_or(Status::MISSING, |address| put(out, address))
}

fn lookup(error: &enshwick_reflect::LookupError) -> Status {
    match error {
        enshwick_reflect::LookupError::Ambiguous(_) => Status::AMBIGUOUS,
        _ => Status::MISSING,
    }
}

extern "system" fn system(name: Str, out: *mut u64) -> Status {
    let (Some(loader), Some(name)) = (loader(), text(name)) else {
        return Status::BAD_ARGS;
    };
    match loader.reflection.system(name) {
        Ok(found) if found.entry() != 0 => put(out, found.entry()),
        Ok(_) => Status::MISSING,
        Err(error) => lookup(&error),
    }
}

extern "system" fn field_offset(type_name: Str, field: Str, out: *mut u64) -> Status {
    let (Some(loader), Some(type_name), Some(field)) = (loader(), text(type_name), text(field))
    else {
        return Status::BAD_ARGS;
    };
    match loader.reflection.field_offset(type_name, field) {
        Ok(offset) => put(out, offset),
        Err(error) => lookup(&error),
    }
}

extern "system" fn type_size(type_name: Str, out: *mut u32) -> Status {
    let (Some(loader), Some(type_name)) = (loader(), text(type_name)) else {
        return Status::BAD_ARGS;
    };
    match loader.reflection.type_info(type_name) {
        Ok(info) => put(out, info.size()),
        Err(error) => lookup(&error),
    }
}

extern "system" fn hook(me: ModHandle, spec: *const HookSpec) -> Status {
    let Some(loader) = loader() else {
        return Status::BAD_ARGS;
    };
    let Some((index, record)) = loader.record(me) else {
        return Status::BAD_ARGS;
    };
    if record.off.load(Ordering::Acquire) {
        return Status::FAULTED;
    }
    // SAFETY: a non-null spec from a mod points at a valid `HookSpec`.
    let Some(spec) = (unsafe { spec.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    // SAFETY: `next` is a static inside the mod's DLL, which is never unloaded.
    let Some(next) = (unsafe { spec.next.as_ref::<'static>() }) else {
        return Status::BAD_ARGS;
    };
    let link = Link {
        owner: index,
        detour: spec.detour,
        next,
        priority: spec.priority,
        flags: spec.flags,
    };
    match loader.chains.lock() {
        Ok(mut chains) => chains.add(spec.target, link),
        Err(_) => Status::REFUSED,
    }
}

extern "system" fn caller_frame(out: *mut api_frame::CallerFrame) {
    let frame = loader()
        .map(|loader| (loader.caller_frame)())
        .unwrap_or_default();
    let _written = put(out, frame);
}

extern "system" fn fault(me: ModHandle, reason: Str) {
    let Some(loader) = loader() else {
        return;
    };
    if let Some((index, _)) = loader.record(me) {
        loader.turn_off(index, text(reason).unwrap_or("a fault"));
    }
}

extern "system" fn log(me: ModHandle, level: Level, message: Str) {
    let Some(loader) = loader() else {
        return;
    };
    if let (Some((_, record)), Some(message)) = (loader.record(me), text(message)) {
        crate::log::write(level, &record.id, message);
    }
}

extern "system" fn settings(me: ModHandle, defaults: Str, out: *mut Str) -> Status {
    let (Some(loader), Some(defaults)) = (loader(), text(defaults)) else {
        return Status::BAD_ARGS;
    };
    let Some((_, record)) = loader.record(me) else {
        return Status::BAD_ARGS;
    };
    let path = loader.configs_dir.join(format!("{}.toml", record.id));
    let old = loader.mods_dir.join(format!("{}.toml", record.id));
    if !path.exists() && old.exists() && std::fs::rename(&old, &path).is_ok() {
        crate::log::write(Level::INFO, &record.id, "settings moved to mod-configs");
    }
    let content = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        if std::fs::write(&path, defaults).is_err() {
            crate::log::write(
                Level::WARN,
                &record.id,
                "could not write the default settings",
            );
        }
        defaults.to_owned()
    });
    let Ok(mut slot) = record.settings.lock() else {
        return Status::REFUSED;
    };
    let kept = slot.insert(content);
    // The text stays in the record until the loader drops it after init.
    put(out, Str::new(kept))
}

extern "system" fn publish(
    me: ModHandle,
    name: Str,
    version: u32,
    table: *const core::ffi::c_void,
) -> Status {
    let (Some(loader), Some(name)) = (loader(), text(name)) else {
        return Status::BAD_ARGS;
    };
    let Some((index, _)) = loader.record(me) else {
        return Status::BAD_ARGS;
    };
    if table.is_null() {
        return Status::BAD_ARGS;
    }
    let Ok(mut services) = loader.services.lock() else {
        return Status::REFUSED;
    };
    let key = (name.to_owned(), version);
    if services.contains_key(&key) {
        return Status::REFUSED;
    }
    services.insert(key, (index, table.expose_provenance()));
    Status::OK
}

extern "system" fn service(
    me: ModHandle,
    name: Str,
    version: u32,
    out: *mut *const core::ffi::c_void,
) -> Status {
    let (Some(loader), Some(name)) = (loader(), text(name)) else {
        return Status::BAD_ARGS;
    };
    if loader.record(me).is_none() {
        return Status::BAD_ARGS;
    }
    let Ok(services) = loader.services.lock() else {
        return Status::REFUSED;
    };
    let Some(&(owner, table)) = services.get(&(name.to_owned(), version)) else {
        return Status::MISSING;
    };
    if loader
        .mods
        .get(owner)
        .is_none_or(|m| m.off.load(Ordering::Acquire))
    {
        return Status::MISSING;
    }
    put(out, core::ptr::with_exposed_provenance(table))
}

extern "system" fn constant(me: ModHandle, name: Str, out: *mut u64) -> Status {
    let (Some(loader), Some(name)) = (loader(), text(name)) else {
        return Status::BAD_ARGS;
    };
    let Some((index, _)) = loader.record(me) else {
        return Status::BAD_ARGS;
    };
    loader
        .resolve_constant(index, name)
        .map_or(Status::MISSING, |value| put(out, value))
}

extern "system" fn data_dir(me: ModHandle, out: *mut Str) -> Status {
    let Some(loader) = loader() else {
        return Status::BAD_ARGS;
    };
    let Some((_, record)) = loader.record(me) else {
        return Status::BAD_ARGS;
    };
    let dir = loader.data_dir.join(&record.id);
    if std::fs::create_dir_all(&dir).is_err() {
        return Status::REFUSED;
    }
    // The path lives for the process: the mod keeps it as long as it likes.
    let path: &'static str = Box::leak(dir.to_string_lossy().into_owned().into_boxed_str());
    put(out, Str::new(path))
}

/// Mod handles start at 1; 0 is no mod.
#[must_use]
pub(crate) fn handle(index: usize) -> ModHandle {
    ModHandle(index + 1)
}
