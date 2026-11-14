//! Host functions about the mods: logs, faults, settings and what is loaded.

use std::sync::atomic::Ordering;

use enshwick_base::Relock;
use enshwick_core::abi as api_frame;
use enshwick_core::abi::{
    ContentRename, Header, Level, ManifestFlags, ModHandle, ModInfo, Side, Status, Str,
};
use enshwick_core::mods::LOADER_FILE;

use crate::engine::loader::{ModRecord, loader};
use crate::host::LOADER_HANDLE;
use crate::host::layer::guards::{before_delivery, in_init};
use crate::host::layer::{AnyMod, List, Mod, Out, SizedListOut, Text, entries};
use crate::host::memory::text;

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

entries! {
    fn fault(me as AnyMod, reason: Str) {
        me.loader.turn_off(me.index, text(reason).unwrap_or("a fault"));
        Ok(())
    }

    fn log(me: ModHandle, level: Level, message as Text) {
        let Some(loader) = loader() else {
            return Ok(());
        };
        let id = if me == LOADER_HANDLE {
            Some(crate::engine::settings::LOADER_ID)
        } else {
            loader.record(me).map(|(_, record)| record.id.as_str())
        };
        if let Some(id) = id {
            crate::engine::log::write(level, id, message);
        }
        Ok(())
    }

    fn settings(me as AnyMod, file as Text, out as Out<Str>) -> Status {
        if !me
            .record
            .settings_files
            .relock()
            .iter()
            .any(|f| f.file == file)
        {
            return Err(Status::MISSING.into());
        }
        let content = crate::base::worlds::file_text(me.record, file);
        out.put(Str::new(me.record.settings.relock().keep(&content).unwrap_or_default()))
    }

    fn rename_content(me as Mod, [renames, count] as List<ContentRename>) -> Status {
        before_delivery(me.loader)?;
        Ok(crate::content::content_renames::declare(me.index, renames))
    }

    fn mods(me as AnyMod, [out, entry_size, capacity, count] as SizedListOut<ModInfo>) -> Status {
        let infos: Vec<ModInfo> = core::iter::once(loader_info())
            .chain(me.loader.mods.iter().map(mod_info))
            .collect();
        out.put(&infos)
    }

    fn join_missing(
        _me as AnyMod,
        [out, entry_size, capacity, count] as SizedListOut<ModInfo>,
    ) -> Status {
        let held = crate::base::join::client::join_missing();
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
        out.put(&infos)
    }

    fn config_schema(me as AnyMod, file as Text, schema as Text, template as Text) -> Status {
        let parsed =
            enshwick_core::config::Schema::parse(schema).map_err(|_| Status::BAD_ARGS)?;
        if !enshwick_core::config::__file_name(file) {
            return Err(Status::BAD_ARGS.into());
        }
        let mut files = me.record.settings_files.relock();
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
        callback: api_frame::ConfigCheckFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        in_init(me.index)?;
        let mut checks = me.record.checks.relock();
        if checks.iter().any(|(known, _, _)| known == file) {
            return Err(Status::REFUSED.into());
        }
        checks.push((file.to_owned(), callback, user.expose_provenance()));
        Ok(())
    }

    fn config_set(me as Mod, file as Text, key as Text, value as Text) -> Status {
        crate::base::configure::apply_own(me.loader, me.index.get(), file, key, value).map_err(
            |error| {
                crate::engine::log::write(
                    Level::WARN,
                    &me.record.id,
                    &format!("{file}.toml {key} not written: {}", error.logged()),
                );
                error.status()
            },
        )?;
        Ok(())
    }

    fn texts(me as Mod, toml as Text) -> Status {
        let parsed = enshwick_core::i18n::Texts::parse(toml).map_err(|_| Status::BAD_ARGS)?;
        *me.record.texts.relock() = Some(parsed);
        Ok(())
    }

    fn language(_me: ModHandle) -> u32 {
        if let Some(reader) = crate::engine::texts::reader() {
            return Ok(reader.0);
        }
        Ok(loader()
            .filter(|loader| loader.side == Side::CLIENT)
            .map_or(0, |_| crate::content::served::language().0))
    }
}
