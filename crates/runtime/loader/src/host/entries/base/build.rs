//! Host functions for the game build: symbols, systems, layouts and constants.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::{BuildInfo, Level, Status, Str};

use crate::engine::kept_text::keep;
use crate::host::layer::{AnyMod, Caller, HeadedOut, Out, Text, Writer, entries, loaded};

entries! {
    fn build(loader as Caller, out as HeadedOut<BuildInfo>) -> Status {
        out.put(enshwick_abi::headed!(BuildInfo {
            side: loader.side,
            base: loader.base,
            fingerprint: enshwick_core::abi::Fingerprint {
                guid: enshwick_core::content::Guid(loader.fingerprint.guid()),
                age: loader.fingerprint.age(),
            },
            complete: u8::from(loader.build.complete()),
        }))
    }

    fn symbol(me as Writer, name as Text, out as Out<u64>) -> Status {
        let loader = loaded()?;
        let index = loader.rows_of(me.handle).ok_or(Status::BAD_ARGS)?;
        let address = loader
            .resolve_symbol(index.get(), name)
            .ok_or(Status::MISSING)?;
        crate::engine::diag::name(address, name);
        out.put(address)
    }

    fn system(loader as Caller, name as Text, out as Out<u64>) -> Status {
        let found = loader.reflection.system(name)?;
        if found.entry() == 0 {
            return Err(Status::MISSING.into());
        }
        crate::engine::diag::name(found.entry(), name);
        out.put(found.entry())
    }

    fn field_offset(
        loader as Caller,
        type_name as Text,
        field as Text,
        out as Out<u64>,
    ) -> Status {
        let offset = loader
            .reflection
            .field_offset(crate::engine::catalogue::type_name(type_name), field)?;
        out.put(offset)
    }

    fn type_size(loader as Caller, type_name as Text, out as Out<u32>) -> Status {
        let info = loader
            .reflection
            .type_info(crate::engine::catalogue::type_name(type_name))?;
        out.put(info.size())
    }

    fn caller_frame(loader as Caller, out as Out<api_frame::CallerFrame>) -> Status {
        out.put((loader.caller_frame)())
    }

    fn constant(me as Writer, name as Text, out as Out<u64>) -> Status {
        let loader = loaded()?;
        let index = loader.rows_of(me.handle).ok_or(Status::BAD_ARGS)?;
        out.put(
            loader
                .resolve_constant(index.get(), name)
                .ok_or(Status::MISSING)?,
        )
    }

    fn data_dir(me as AnyMod, out as Out<Str>) -> Status {
        let key = crate::base::saves::world_key(me.loader).ok_or(Status::UNAVAILABLE)?;
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

    fn log_level(me as Writer, out as Out<Level>) -> Status {
        out.put(crate::engine::log::level(me.id))
    }

    fn type_hash(loader as Caller, type_name as Text, out as Out<u32>) -> Status {
        let info = loader
            .reflection
            .type_info(crate::engine::catalogue::type_name(type_name))?;
        out.put(crate::engine::world::type_hash(info.va()).ok_or(Status::MISSING)?)
    }
}

#[cfg(test)]
mod tests {
    use enshwick_core::abi::ModHandle;

    use super::*;

    #[test]
    fn the_loader_s_own_copy_reads_the_loader_s_level() {
        crate::engine::log::set_levels(crate::engine::settings::Settings {
            log_level: Level::DEBUG,
            ..crate::engine::settings::Settings::default()
        });
        let mut level = Level::ERROR;
        assert_eq!(
            log_level(crate::host::LOADER_HANDLE, &raw mut level),
            Status::OK
        );
        assert_eq!(level, Level::DEBUG);
        assert_eq!(
            log_level(ModHandle(77), &raw mut level),
            Status::BAD_ARGS,
            "a handle the loader never gave out"
        );
    }
}
