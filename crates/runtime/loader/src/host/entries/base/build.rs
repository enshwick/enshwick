//! Host functions for the game build: symbols, systems, layouts and constants.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::{BuildInfo, Header, Level, ModHandle, Status, Str};

use crate::engine::kept_text::keep;
use crate::host::layer::{AnyMod, HeadedOut, Out, Text, entries, loaded};

entries! {
    fn build(out as HeadedOut<BuildInfo>) {
        let loader = loaded()?;
        out.put(BuildInfo {
            header: Header::of::<BuildInfo>(),
            side: loader.side,
            base: loader.base,
            fingerprint: enshwick_core::abi::Fingerprint {
                guid: loader.fingerprint.guid(),
                age: loader.fingerprint.age(),
            },
            table: if loader.build.complete() {
                enshwick_core::abi::BuildTable::EXACT
            } else {
                enshwick_core::abi::BuildTable::PATTERNS
            },
        })
    }

    fn symbol(me: ModHandle, name as Text, out as Out<u64>) -> Status {
        let loader = loaded()?;
        let index = loader.rows_of(me).ok_or(Status::BAD_ARGS)?;
        let address = loader
            .resolve_symbol(index.get(), name)
            .ok_or(Status::MISSING)?;
        crate::engine::diag::name(address, name);
        out.put(address)
    }

    fn system(name as Text, out as Out<u64>) -> Status {
        let found = loaded()?.reflection.system(name)?;
        if found.entry() == 0 {
            return Err(Status::MISSING.into());
        }
        crate::engine::diag::name(found.entry(), name);
        out.put(found.entry())
    }

    fn field_offset(type_name as Text, field as Text, out as Out<u64>) -> Status {
        let offset = loaded()?
            .reflection
            .field_offset(crate::engine::catalogue::type_name(type_name), field)?;
        out.put(offset)
    }

    fn type_size(type_name as Text, out as Out<u32>) -> Status {
        let info = loaded()?
            .reflection
            .type_info(crate::engine::catalogue::type_name(type_name))?;
        out.put(info.size())
    }

    fn caller_frame(out as Out<api_frame::CallerFrame>) {
        let frame = crate::engine::loader::loader()
            .map(|loader| (loader.caller_frame)())
            .unwrap_or_default();
        out.put(frame)
    }

    fn constant(me: ModHandle, name as Text, out as Out<u64>) -> Status {
        let loader = loaded()?;
        let index = loader.rows_of(me).ok_or(Status::BAD_ARGS)?;
        out.put(
            loader
                .resolve_constant(index.get(), name)
                .ok_or(Status::MISSING)?,
        )
    }

    fn data_dir(me as AnyMod, out as Out<Str>) -> Status {
        let key = crate::base::saves::world_key(me.loader).ok_or(Status::MISSING)?;
        let dir = crate::base::world_folders::data_of(key).join(&me.record.id);
        std::fs::create_dir_all(&dir).map_err(|_| Status::REFUSED)?;
        // The path lives for the process: the mod keeps it as long as it likes.
        out.put(Str::new(keep(&dir.to_string_lossy())))
    }

    fn mod_dir(me as AnyMod, out as Out<Str>) -> Status {
        let dir = me.loader.mods_dir.join(me.record.folder());
        // The path lives for the process: the mod keeps it as long as it likes.
        out.put(Str::new(keep(&dir.to_string_lossy())))
    }

    fn log_level(me: ModHandle) -> Level {
        // The loader's own copy of the API, as `log` names it.
        if me == crate::host::LOADER_HANDLE {
            return Ok(crate::engine::log::level(crate::engine::settings::LOADER_ID));
        }
        Ok(crate::engine::loader::loader()
            .and_then(|loader| loader.record(me))
            .map_or(Level::INFO, |(_, record)| crate::engine::log::level(&record.id)))
    }

    fn type_hash(type_name as Text, out as Out<u32>) -> Status {
        let info = loaded()?
            .reflection
            .type_info(crate::engine::catalogue::type_name(type_name))?;
        out.put(crate::engine::world::type_hash(info.va()).ok_or(Status::MISSING)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loader_s_own_copy_reads_the_loader_s_level() {
        crate::engine::log::set_levels(crate::engine::settings::Settings {
            log_level: Level::DEBUG,
            ..crate::engine::settings::Settings::default()
        });
        assert_eq!(log_level(crate::host::LOADER_HANDLE), Level::DEBUG);
    }
}
