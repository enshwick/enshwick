//! Host functions about the mods: logs, faults, settings and what is loaded.

use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use enshwick::abi as api_frame;
use enshwick::abi::{Header, Level, ManifestFlags, ModHandle, ModInfo, Side, Status, Str};
use enshwick::mods::LOADER_FILE;

use crate::host::join::JOIN_MISSING;
use crate::host::memory::{get_headed, put, text};
use crate::host::{LOADER_HANDLE, ModRecord, calling, entry, loader};
use crate::locks::Relock;
use crate::mod_index::ModIndex;

pub(crate) extern "system" fn fault(me: ModHandle, reason: Str) {
    entry("fault", || {
        let Some(loader) = loader() else {
            return;
        };
        if let Some((index, _)) = loader.record(me) {
            loader.turn_off(index, text(reason).unwrap_or("a fault"));
        }
    });
}

pub(crate) extern "system" fn log(me: ModHandle, level: Level, message: Str) {
    entry("log", || {
        let Some(loader) = loader() else {
            return;
        };
        let id = if me == LOADER_HANDLE {
            Some(crate::settings::LOADER_ID)
        } else {
            loader.record(me).map(|(_, record)| record.id.as_str())
        };
        if let (Some(id), Some(message)) = (id, text(message)) {
            crate::log::write(level, id, message);
        }
    });
}

pub(crate) extern "system" fn settings(me: ModHandle, defaults: Str, out: *mut Str) -> Status {
    entry("settings", || {
        let (Some(loader), Some(defaults)) = (loader(), text(defaults)) else {
            return Status::BAD_ARGS;
        };
        let Some((index, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        let path = loader.configs_dir.join(format!("{}.toml", record.id));
        let old = loader.mods_dir.join(format!("{}.toml", record.id));
        if !path.exists() && old.exists() && std::fs::rename(&old, &path).is_ok() {
            crate::log::write(Level::INFO, &record.id, "settings moved to mod-configs");
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if std::fs::write(&path, defaults).is_err() {
                    crate::log::write(
                        Level::WARN,
                        &record.id,
                        "could not write the default settings",
                    );
                    defaults.to_owned()
                } else {
                    crate::migrations::settings_written(loader, index.get());
                    std::fs::read_to_string(&path).unwrap_or_else(|_| defaults.to_owned())
                }
            }
            // The player's file stays as it is, for them to fix.
            Err(e) => {
                crate::log::write(
                    Level::WARN,
                    &record.id,
                    &format!("{}: {e}; the defaults apply", path.display()),
                );
                defaults.to_owned()
            }
        };
        let content = crate::worlds::effective(loader, record, &content);
        let kept = record.settings.keep(&content);
        // SAFETY: `HostApi::settings` has the mod pass `out` null or pointing
        // at a writable, aligned `Str` that no other thread touches during the
        // call.
        unsafe { put(out, kept) }
    })
}

pub(crate) extern "system" fn migrate(
    me: ModHandle,
    set: *const api_frame::MigrationSet,
) -> Status {
    entry("migrate", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.delivery.get().is_some() {
            return Status::TOO_LATE;
        }
        // SAFETY: a mod passes null or its version of the set, and all of the
        // set, its migration function included, is required.
        match unsafe { get_headed(set, size_of::<api_frame::MigrationSet>()) } {
            Some(set) => crate::migrations::register(loader, index.get(), &set),
            None => Status::BAD_ARGS,
        }
    })
}

/// The listing entry of the loader itself.
pub(crate) fn loader_info() -> ModInfo {
    ModInfo {
        header: Header::of::<ModInfo>(),
        id: Str::new(crate::settings::LOADER_ID),
        version: Str::new(env!("CARGO_PKG_VERSION")),
        side: Side::BOTH,
        flags: ManifestFlags::NONE,
        off: 0,
        file: Str::new(LOADER_FILE),
        name: Str::new("Enshwick"),
        description: Str::new("The mod loader and modding API for Enshrouded"),
        authors: LOADER_AUTHORS.as_ptr(),
        author_count: LOADER_AUTHORS.len(),
        license: Str::new(env!("CARGO_PKG_LICENSE")),
        homepage: Str::new(env!("CARGO_PKG_REPOSITORY")),
        replaced_by: Str::EMPTY,
    }
}

/// The loader's authors, as its listing names them.
pub(crate) static LOADER_AUTHORS: [Str; 1] = [Str::new("themaiby")];

/// The listing entry of a loaded mod; its strings live as long as the loader.
pub(crate) fn mod_info(record: &'static ModRecord) -> ModInfo {
    ModInfo {
        header: Header::of::<ModInfo>(),
        id: Str::new(&record.id),
        version: Str::new(&record.version),
        side: record.manifest.side,
        flags: record.manifest.flags,
        off: u8::from(record.off.load(Ordering::Acquire)),
        file: Str::new(&record.file),
        name: record.manifest.name,
        description: record.manifest.description,
        authors: record.manifest.authors,
        author_count: record.manifest.author_count,
        license: record.manifest.license,
        homepage: record.manifest.homepage,
        replaced_by: Str::EMPTY,
    }
}

pub(crate) extern "system" fn mods(
    me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("mods", || {
        let Some(loader) = loader() else {
            return Status::BAD_ARGS;
        };
        if loader.record(me).is_none() {
            return Status::BAD_ARGS;
        }
        let infos: Vec<ModInfo> = core::iter::once(loader_info())
            .chain(loader.mods.iter().map(mod_info))
            .collect();
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { list(&infos, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn join_missing(
    me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("join_missing", || {
        let Some(loader) = loader() else {
            return Status::BAD_ARGS;
        };
        if loader.record(me).is_none() {
            return Status::BAD_ARGS;
        }
        let held = JOIN_MISSING.relock().clone();
        let infos: Vec<ModInfo> = held
            .iter()
            .map(|missing| ModInfo {
                header: Header::of::<ModInfo>(),
                id: Str::new(missing.id),
                version: Str::new(missing.version),
                side: Side::BOTH,
                flags: ManifestFlags::JOIN,
                off: 1,
                file: Str::new(missing.file),
                name: Str::new(missing.name),
                homepage: Str::new(missing.page),
                ..ModInfo::EMPTY
            })
            .collect();
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { list(&infos, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn replaced(
    me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    /// The listing entries, built once: the mods replaced at start never
    /// change.
    static INFOS: OnceLock<Vec<ModInfo>> = OnceLock::new();
    entry("replaced", || {
        let Some(loader) = loader() else {
            return Status::BAD_ARGS;
        };
        if loader.record(me).is_none() {
            return Status::BAD_ARGS;
        }
        let infos = INFOS.get_or_init(|| {
            let text = |s: String| Str::new(Box::leak(s.into_boxed_str()));
            crate::modlist::replaced()
                .iter()
                .map(|old| ModInfo {
                    header: Header::of::<ModInfo>(),
                    id: Str::new(&old.id),
                    version: Str::new(&old.version),
                    off: 1,
                    file: text(format!("mods/{}", old.file)),
                    replaced_by: Str::new(&old.by),
                    ..ModInfo::EMPTY
                })
                .collect()
        });
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { list(infos, out, entry_size, capacity, count) }
    })
}

/// Copies up to `capacity` of `infos` to `out`, `entry_size` bytes apart, and
/// writes how many there are.
///
/// # Safety
///
/// - A non-null `out` is writable for `capacity` entries of `entry_size`
///   bytes each; no alignment is needed.
/// - A non-null `count` points at a writable, aligned `u32`.
/// - Nothing else reads or writes those entries during the call.
pub(crate) unsafe fn list<T: Copy>(
    infos: &[T],
    out: *mut T,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if !out.is_null() {
        let stride = entry_size as usize;
        if stride < size_of::<Header>() {
            return Status::BAD_ARGS;
        }
        let copied = stride.min(size_of::<T>());
        let size = u32::try_from(copied).unwrap_or(u32::MAX);
        for (index, info) in infos.iter().take(capacity as usize).enumerate() {
            // SAFETY: `index` is below `capacity`, and the caller guarantees
            // `capacity` entries of `stride` bytes at the non-null `out`, so
            // entry `index` starts inside that list.
            let entry = unsafe { out.cast::<u8>().add(index * stride) };
            // SAFETY: the caller guarantees `entry` writable for `stride` bytes,
            // `info` is readable for `size_of::<T>()`, `copied` is at most
            // both, and the loader's `infos` do not overlap the mod's list.
            unsafe {
                core::ptr::copy_nonoverlapping((&raw const *info).cast::<u8>(), entry, copied);
            };
            // The header says what the entry holds, never more than it has.
            // SAFETY: the header's `size` is the entry's first field, inside the
            // `stride` bytes, at least a `Header` as checked before the loop.
            unsafe { entry.cast::<u32>().write_unaligned(size) };
        }
    }
    // SAFETY: `HostApi::replaced` has the mod pass `count` null or pointing at
    // a writable, aligned `u32` that no other thread touches during the call.
    unsafe { put(count, u32::try_from(infos.len()).unwrap_or(u32::MAX)) }
}

pub(crate) extern "system" fn config_schema(me: ModHandle, schema: Str) -> Status {
    entry("config_schema", || {
        let (Some(loader), Some(schema)) = (loader(), text(schema)) else {
            return Status::BAD_ARGS;
        };
        let Some((_, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        let Ok(schema) = enshwick::config::Schema::parse(schema) else {
            return Status::BAD_ARGS;
        };
        let mut slot = record.schema.relock();
        *slot = Some(schema);
        Status::OK
    })
}

pub(crate) extern "system" fn sync(me: ModHandle, synced: Str, out: *mut Str) -> Status {
    entry("sync", || {
        let (Some(loader), Some(synced)) = (loader(), text(synced)) else {
            return Status::BAD_ARGS;
        };
        let Some((_, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        {
            let mut reported = record.synced.relock();
            let now = (!synced.is_empty()).then(|| synced.to_owned());
            // A server whose join values changed tells the joined clients at
            // once; the first report, at init, finds none joined.
            if reported.is_some()
                && *reported != now
                && (loader.side == Side::SERVER || crate::world::hosting())
            {
                crate::remote::push_sync(&record.id, now.as_deref().unwrap_or("{}"));
                crate::advertise::wake();
            }
            *reported = now;
        }
        let server = record.server_synced.relock().clone().unwrap_or_default();
        let kept = record.sync_out.keep(&server);
        // SAFETY: `HostApi::sync` has the mod pass `out` null or pointing at a
        // writable, aligned `Str` that no other thread touches during the call.
        unsafe { put(out, kept) }
    })
}

pub(crate) extern "system" fn config_check(
    me: ModHandle,
    callback: api_frame::ConfigCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("config_check", || match calling(me) {
        Ok((_, index, _)) if !crate::reload::in_init(index.get()) => Status::TOO_LATE,
        Ok((_, _, record)) => {
            if record
                .check
                .set((callback, user.expose_provenance()))
                .is_ok()
            {
                Status::OK
            } else {
                Status::REFUSED
            }
        }
        Err(status) => status,
    })
}

/// Runs mod `index`'s check of the new settings text `text`; `Ok` for a mod
/// without one.
pub(crate) fn check_settings(index: usize, text: &str) -> Result<(), String> {
    let Some(&(callback, user)) = loader()
        .and_then(|loader| loader.mods.get(index))
        .filter(|record| !record.off.load(Ordering::Acquire))
        .and_then(|record| record.check.get())
    else {
        return Ok(());
    };
    let mut why = Str::EMPTY;
    // SAFETY: the mod's check with its own pointer; the text lives for the
    // call, and the safe layer catches its panics.
    let status = crate::profile::call(
        ModIndex::new(index),
        crate::profile::Feature::Events,
        || unsafe {
            callback(
                Str::new(text),
                &raw mut why,
                core::ptr::with_exposed_provenance_mut(user),
            )
        },
    );
    if status.is_ok() {
        return Ok(());
    }
    // SAFETY: the reason stays valid until the mod's next check.
    Err(unsafe { why.as_str() }.map_or_else(
        || crate::texts::fill("refused.by-mod", &[]),
        ToOwned::to_owned,
    ))
}

pub(crate) extern "system" fn texts(me: ModHandle, toml: Str) -> Status {
    entry("texts", || {
        let (_, _, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let Some(toml) = text(toml) else {
            return Status::BAD_ARGS;
        };
        let Ok(parsed) = enshwick::i18n::Texts::parse(toml) else {
            return Status::BAD_ARGS;
        };
        let mut slot = record.texts.relock();
        *slot = Some(parsed);
        Status::OK
    })
}

pub(crate) extern "system" fn language(_me: ModHandle) -> u32 {
    loader()
        .filter(|loader| loader.side == Side::CLIENT)
        .map_or(0, |_| crate::content::language().0)
}
