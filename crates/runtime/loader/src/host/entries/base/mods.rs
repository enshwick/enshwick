//! Host functions about the mods: logs, faults, settings and what is loaded.

use std::sync::atomic::Ordering;

use enshwick_base::Relock;
use enshwick_core::abi as api_frame;
use enshwick_core::abi::{Level, ManifestFlags, ModInfo, Side, Status, Str};
use enshwick_core::mods::LOADER_FILE;

use crate::engine::loader::{ModRecord, loader};
use crate::host::layer::guards::in_init;
use crate::host::layer::{AnyMod, Callback, Mod, Out, SizedListOut, Text, Writer, entries};
use crate::host::memory::text;

/// The listing entry of the loader itself.
pub(crate) fn loader_info() -> ModInfo {
    enshwick_abi::headed!(ModInfo {
        id: Str::new(crate::engine::settings::LOADER_ID),
        version: Str::new(crate::base::releases::STATED),
        side: Side::BOTH,
        flags: ManifestFlags::NONE,
        off: 0,
        file: Str::new(LOADER_FILE),
        name: Str::new("Enshwick"),
        description: Str::new("The mod loader and modding API for Enshrouded"),
        authors: LOADER_AUTHORS.as_ptr(),
        author_count: u32::try_from(LOADER_AUTHORS.len()).unwrap_or(u32::MAX),
        license: Str::new(env!("CARGO_PKG_LICENSE")),
        homepage: Str::new(env!("CARGO_PKG_REPOSITORY")),
    })
}

/// The loader's authors, as its listing names them.
pub(crate) static LOADER_AUTHORS: [Str; 1] = [Str::new("themaiby")];

/// The listing entry of a loaded mod; its strings live as long as the loader.
pub(crate) fn mod_info(record: &'static ModRecord) -> ModInfo {
    enshwick_abi::headed!(ModInfo {
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
    })
}

entries! {
    fn fault(me as AnyMod, reason: Str) -> Status {
        me.loader.turn_off(me.index, text(reason).unwrap_or("a fault"));
        Ok(())
    }

    fn log(me as Writer, level: Level, message as Text) -> Status {
        crate::engine::log::write(level, me.id, message);
        Ok(())
    }

    fn settings(me as AnyMod, file as Text, out as Out<Str>) -> Status {
        let settings = crate::base::worlds::settings_of(me.record).ok_or(Status::UNAVAILABLE)?;
        if !settings.files.relock().iter().any(|f| f.file == file) {
            return Err(Status::MISSING.into());
        }
        let content = crate::base::worlds::file_text(me.record, file);
        out.put(Str::new(settings.settings.relock().keep(&content).unwrap_or_default()))
    }

    fn mods(me as AnyMod, [out, entry_size, capacity, count] as SizedListOut<ModInfo>) -> Status {
        let infos: Vec<ModInfo> = core::iter::once(loader_info())
            .chain(me.loader.mods.iter().map(mod_info))
            .collect();
        out.put(&infos)
    }

    fn loader_off(
        _me as AnyMod,
        [out, entry_size, capacity, count] as SizedListOut<api_frame::LoaderOff>,
    ) -> Status {
        out.put(&crate::engine::diag::parts_off_listed())
    }

    fn join_missing(
        _me as AnyMod,
        [out, entry_size, capacity, count] as SizedListOut<ModInfo>,
    ) -> Status {
        let held = crate::base::join::client::join_missing();
        let infos: Vec<ModInfo> = held
            .iter()
            .map(|missing| enshwick_abi::headed!(ModInfo {
                id: Str::new(missing.id),
                version: Str::new(missing.version),
                side: Side::BOTH,
                off: 1,
                file: Str::new(missing.file),
                ..ModInfo::EMPTY
            }))
            .collect();
        out.put(&infos)
    }

    fn config_schema(me as AnyMod, file as Text, schema as Text, template as Text) -> Status {
        let parsed =
            enshwick_core::config::Schema::parse(schema).map_err(|_| Status::BAD_ARGS)?;
        if !enshwick_core::config::__file_name(file) {
            return Err(Status::BAD_ARGS.into());
        }
        let settings = crate::base::worlds::settings_of(me.record).ok_or(Status::UNAVAILABLE)?;
        let mut files = settings.files.relock();
        if let Some(known) = files.iter().find(|f| f.file == file) {
            return if known.schema == parsed {
                Ok(())
            } else {
                Err(Status::REFUSED.into())
            };
        }
        files.push(crate::base::worlds::SettingsFile {
            file: file.to_owned(),
            schema: parsed,
            template: template.to_owned(),
        });
        Ok(())
    }

    fn config_check(
        me as Mod,
        file as Text,
        callback as Callback<api_frame::ConfigCheckFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        in_init(me.index)?;
        let settings = crate::base::worlds::settings_of(me.record).ok_or(Status::UNAVAILABLE)?;
        let mut checks = settings.checks.relock();
        if checks.iter().any(|(known, _, _)| known == file) {
            return Err(Status::REFUSED.into());
        }
        checks.push((file.to_owned(), callback, user.expose_provenance()));
        Ok(())
    }

    fn set_config(me as Mod, file as Text, key as Text, value as Text) -> Status {
        crate::base::configure::apply_own(me.loader, me.index.get(), file, key, value)?;
        Ok(())
    }

    fn texts(me as Mod, toml as Text) -> Status {
        let parsed = enshwick_core::i18n::Texts::parse(toml).map_err(|_| Status::BAD_ARGS)?;
        *me.record.texts.relock() = Some(parsed);
        Ok(())
    }

    fn language(_me as Writer, out as Out<u32>) -> Status {
        if let Some(reader) = crate::engine::texts::reader() {
            return out.put(reader.0);
        }
        out.put(
            loader()
                .filter(|loader| loader.side == Side::CLIENT)
                .map_or(0, |_| crate::engine::texts::game_language().0),
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_mods_screen_s_own_row_states_the_running_version() {
        // SAFETY: the view points at the static text of `STATED`.
        let version = unsafe { super::loader_info().version.as_str() };
        assert_eq!(version, Some(crate::base::releases::STATED));
    }
}
