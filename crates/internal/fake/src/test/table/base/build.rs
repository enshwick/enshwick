//! The fake's answers about the game build: symbols, systems, layouts and constants.

use crate::test::FakeHost;
use crate::test::table::base::args;
use crate::test::table::entries;
use crate::test::table::{found, put, text, with};
use enshwick_core::abi::{BuildInfo, CallerFrame, Level, Str};

entries! {
    pub(crate) fn build(me as Caller, out: *mut BuildInfo as args::HeadedOut) -> Status {
        let host = with(FakeHost::new(), |s| s.host.clone());
        crate::test::table::put_headed(
            out,
            enshwick_core::headed!(BuildInfo {
                side: host.base.side,
                base: host.base.module_base,
                fingerprint: host.base.fingerprint,
                complete: u8::from(host.base.build_complete),
            }),
        )
    }

    pub(crate) fn symbol(
        me as Writer,
        name: Str as args::Text,
        out: *mut u64 as args::Out,
    ) -> Status {
        let name = text(name);
        found(with(None, |s| s.host.base.symbols.get(&name).copied()), out)
    }

    pub(crate) fn system(
        me as Caller,
        name: Str as args::Text,
        out: *mut u64 as args::Out,
    ) -> Status {
        let name = text(name);
        found(with(None, |s| s.host.base.systems.get(&name).copied()), out)
    }

    pub(crate) fn field_offset(
        me as Caller,
        type_name: Str as args::Text,
        field: Str as args::Text,
        out: *mut u64 as args::Out,
    ) -> Status {
        let key = (text(type_name), text(field));
        found(
            with(None, |s| s.host.base.field_offsets.get(&key).copied()),
            out,
        )
    }

    pub(crate) fn type_size(
        me as Caller,
        type_name: Str as args::Text,
        out: *mut u32 as args::Out,
    ) -> Status {
        let name = text(type_name);
        found(
            with(None, |s| s.host.base.type_sizes.get(&name).copied()),
            out,
        )
    }

    pub(crate) fn caller_frame(me as Caller, out: *mut CallerFrame as args::Out) -> Status {
        let frame = with(CallerFrame::default(), |s| s.host.base.caller_frame);
        put(out, frame)
    }

    pub(crate) fn constant(
        me as Writer,
        name: Str as args::Text,
        out: *mut u64 as args::Out,
    ) -> Status {
        let name = text(name);
        found(
            with(None, |s| s.host.base.constants.get(&name).copied()),
            out,
        )
    }

    pub(crate) fn data_dir(me as AnyMod, out: *mut Str as args::Out) -> Status {
        let dir = with(None, |s| {
            s.host
                .base
                .world_key
                .is_some()
                .then(|| s.host.base.data_dir.clone())
        });
        // The loader's path lives for the process; a test leaks a few bytes for it.
        found(
            dir.map(|dir| Str::new(Box::leak(dir.into_boxed_str()))),
            out,
        )
    }

    pub(crate) fn mod_dir(me as AnyMod, out: *mut Str as args::Out) -> Status {
        let dir = with(None, |s| Some(s.host.base.mod_dir.clone()));
        // The loader's path lives for the process; a test leaks a few bytes for it.
        found(
            dir.map(|dir| Str::new(Box::leak(dir.into_boxed_str()))),
            out,
        )
    }

    pub(crate) fn log_level(me as Writer, out: *mut Level as args::Out) -> Status {
        let level = with(Level::INFO, |s| {
            s.host
                .base
                .pack_log_levels
                .iter()
                .find(|(pack, _)| *pack == me)
                .map_or(s.host.base.log_level, |(_, level)| *level)
        });
        put(out, level)
    }

    pub(crate) fn type_hash(
        me as Caller,
        type_name: Str as args::Text,
        out: *mut u32 as args::Out,
    ) -> Status {
        let name = text(type_name);
        found(
            with(None, |s| s.host.base.type_hashes.get(&name).copied()),
            out,
        )
    }
}
