//! Host functions about the mods: logs, faults, settings and what is loaded.

use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use enshwick::abi as api_frame;
use enshwick::abi::{
    ContentRename, Header, Level, ManifestFlags, ModHandle, ModInfo, Side, Status, Str,
};
use enshwick::mods::LOADER_FILE;

use crate::host::join::JOIN_MISSING;
use crate::host::memory::{put, slice, text};
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

pub(crate) extern "system" fn settings(me: ModHandle, file: Str, out: *mut Str) -> Status {
    entry("settings", || {
        let (Some(loader), Some(file)) = (loader(), text(file)) else {
            return Status::BAD_ARGS;
        };
        let Some((_, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        if !record
            .settings_files
            .relock()
            .iter()
            .any(|f| f.file == file)
        {
            return Status::MISSING;
        }
        let content = crate::worlds::file_text(record, file);
        let kept = record.settings.keep(&content);
        // SAFETY: `HostApi::settings` has the mod pass `out` null or pointing
        // at a writable, aligned `Str` that no other thread touches during the
        // call.
        unsafe { put(out, kept) }
    })
}

pub(crate) extern "system" fn rename_content(
    me: ModHandle,
    renames: *const ContentRename,
    count: u32,
) -> Status {
    entry("rename_content", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.delivery.get().is_some() {
            return Status::TOO_LATE;
        }
        if renames.is_null() && count != 0 {
            return Status::BAD_ARGS;
        }
        // SAFETY: `HostApi::rename_content` has the mod pass `count` aligned
        // renames at `renames`, readable and unchanged for the call; they are
        // copied before it returns.
        let renames = unsafe { slice(renames, count as usize) };
        crate::content_renames::declare(index, renames)
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
            crate::record::replaced()
                .iter()
                .map(|old| ModInfo {
                    header: Header::of::<ModInfo>(),
                    id: Str::new(&old.id),
                    version: Str::new(&old.version),
                    off: 1,
                    file: text(format!("mods/{}/", old.folder)),
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

pub(crate) extern "system" fn config_schema(
    me: ModHandle,
    file: Str,
    schema: Str,
    template: Str,
) -> Status {
    entry("config_schema", || {
        let (Some(loader), Some(file), Some(schema), Some(template)) =
            (loader(), text(file), text(schema), text(template))
        else {
            return Status::BAD_ARGS;
        };
        let Some((_, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        let Ok(parsed) = enshwick::config::Schema::parse(schema) else {
            return Status::BAD_ARGS;
        };
        if !enshwick::config::__file_name(file) {
            return Status::BAD_ARGS;
        }
        let mut files = record.settings_files.relock();
        if let Some(known) = files.iter().find(|f| f.file == file) {
            return if known.schema == parsed {
                Status::OK
            } else {
                Status::REFUSED
            };
        }
        files.push(crate::worlds::SettingsFile {
            file: file.to_owned(),
            schema: parsed,
            template: template.to_owned(),
        });
        Status::OK
    })
}

pub(crate) extern "system" fn config_check(
    me: ModHandle,
    file: Str,
    callback: api_frame::ConfigCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("config_check", || match (calling(me), text(file)) {
        (Ok((_, index, _)), _) if !crate::host::in_init(index.get()) => Status::TOO_LATE,
        (Ok((_, _, record)), Some(file)) => {
            let mut checks = record.checks.relock();
            if checks.iter().any(|(known, _, _)| known == file) {
                return Status::REFUSED;
            }
            checks.push((file.to_owned(), callback, user.expose_provenance()));
            Status::OK
        }
        (Ok(_), None) => Status::BAD_ARGS,
        (Err(status), _) => status,
    })
}

/// Runs mod `index`'s check of `text`, a new text of its settings file
/// `file`; `Ok` for a file without one.
pub(crate) fn check_settings(
    index: usize,
    file: &str,
    text: &str,
) -> Result<(), CheckSettingsError> {
    let Some(record) = loader()
        .and_then(|loader| loader.mods.get(index))
        .filter(|record| !record.off.load(Ordering::Acquire))
    else {
        return Ok(());
    };
    let found = record
        .checks
        .relock()
        .iter()
        .find(|(known, _, _)| known == file)
        .map(|&(_, callback, user)| (callback, user));
    let Some((callback, user)) = found else {
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
    let why = unsafe { why.as_str() };
    Err(why.map_or(CheckSettingsError::Unexplained, |why| {
        CheckSettingsError::Refused {
            reason: crate::configure::mod_reason(record.texts.relock().as_ref(), why),
        }
    }))
}

/// Why a mod refuses a new settings text.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CheckSettingsError {
    /// The mod gave no reason.
    #[error("the mod refuses the settings")]
    Unexplained,
    /// The mod's reason, in the reader's language.
    #[error("the mod refuses the settings: {reason}")]
    Refused { reason: String },
}

impl CheckSettingsError {
    /// The refusal as the reader reads it.
    pub(crate) fn reply(&self) -> String {
        match self {
            Self::Unexplained => crate::texts::fill("refused.by-mod", &[]),
            Self::Refused { reason } => reason.clone(),
        }
    }
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
    if let Some(reader) = crate::texts::reader() {
        return reader.0;
    }
    loader()
        .filter(|loader| loader.side == Side::CLIENT)
        .map_or(0, |_| crate::content::language().0)
}
