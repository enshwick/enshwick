//! Host functions about the mods: logs, faults, settings and what is loaded.

use crate::engine::mod_index::ModIndex;
use crate::host::memory::{put, slice, text};
use crate::host::{LOADER_HANDLE, ModRecord, calling, entry, loader};
use enshwick_base::Relock;
use enshwick_core::abi as api_frame;
use enshwick_core::abi::{
    ContentRename, Header, Level, ManifestFlags, ModHandle, ModInfo, Side, Status, Str,
};
use enshwick_core::mods::LOADER_FILE;
use std::sync::atomic::Ordering;

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
            Some(crate::engine::settings::LOADER_ID)
        } else {
            loader.record(me).map(|(_, record)| record.id.as_str())
        };
        if let (Some(id), Some(message)) = (id, text(message)) {
            crate::engine::log::write(level, id, message);
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
        let content = crate::base::worlds::file_text(record, file);
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
        crate::content::content_renames::declare(index, renames)
    })
}

/// The listing entry of the loader itself.
pub(crate) fn loader_info() -> ModInfo {
    ModInfo {
        header: Header::of::<ModInfo>(),
        id: Str::new(crate::engine::settings::LOADER_ID),
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
        unsafe { crate::host::memory::list(&infos, out, entry_size, capacity, count) }
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
        let held = crate::host::join_missing();
        let infos: Vec<ModInfo> = held
            .iter()
            .map(|missing| ModInfo {
                header: Header::of::<ModInfo>(),
                id: Str::new(missing.id),
                version: Str::new(missing.version),
                side: Side::BOTH,
                off: 1,
                file: Str::new(missing.file),
                ..ModInfo::EMPTY
            })
            .collect();
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { crate::host::memory::list(&infos, out, entry_size, capacity, count) }
    })
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
        let Ok(parsed) = enshwick_core::config::Schema::parse(schema) else {
            return Status::BAD_ARGS;
        };
        if !enshwick_core::config::__file_name(file) {
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
        files.push(crate::base::worlds::SettingsFile {
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
    let status = crate::engine::profile::call(
        ModIndex::new(index),
        crate::engine::profile::Feature::Events,
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
            reason: crate::base::configure::mod_reason(record.texts.relock().as_ref(), why),
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
            Self::Unexplained => crate::engine::texts::fill("refused.by-mod", &[]),
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
        let Ok(parsed) = enshwick_core::i18n::Texts::parse(toml) else {
            return Status::BAD_ARGS;
        };
        let mut slot = record.texts.relock();
        *slot = Some(parsed);
        Status::OK
    })
}

pub(crate) extern "system" fn language(_me: ModHandle) -> u32 {
    if let Some(reader) = crate::engine::texts::reader() {
        return reader.0;
    }
    loader()
        .filter(|loader| loader.side == Side::CLIENT)
        .map_or(0, |_| crate::content::served::language().0)
}
