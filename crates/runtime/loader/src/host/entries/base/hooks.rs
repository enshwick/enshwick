//! Host functions for hooks and code patches.

use enshwick_base::Relock;
use enshwick_core::abi::{HookSpec, MidHookSpec, Status};

use crate::engine::chains::Link;
use crate::engine::mid::MidLink;
use crate::engine::mod_index::ModIndex;
use crate::host::layer::{In, Mod, Out, entries};
use crate::host::memory::slice;

entries! {
    fn hook(me as Mod, spec as In<HookSpec>) -> Status {
        // SAFETY: `next` is a static inside the mod's DLL, which is never unloaded.
        let next = unsafe { spec.next.as_ref::<'static>() }.ok_or(Status::BAD_ARGS)?;
        // A pack's links are the API's own code in the loader, shared by
        // every pack: they stay when one pack turns off.
        let owner = if me.record.pack {
            ModIndex::LOADER
        } else {
            me.index
        };
        let link = Link {
            owner,
            detour: spec.detour,
            next,
            priority: spec.priority,
            flags: spec.flags,
            hop: None,
            feature: None,
        };
        Ok(me.loader.chains.relock().add(spec.target, link))
    }

    fn hook_at(me as Mod, spec as In<MidHookSpec>) -> Status {
        let link = MidLink {
            owner: me.index,
            handler: spec.handler,
            user: spec.user.expose_provenance(),
            priority: spec.priority,
            flags: spec.flags,
        };
        Ok(me.loader.mids.relock().add(spec.address, link))
    }

    fn code_range(start as Out<u64>, len as Out<u64>) -> Status {
        let (address, size) = crate::engine::loader::loader()
            .and_then(|loader| crate::engine::code::text(loader.base))
            .ok_or(Status::MISSING)?;
        start.put(address as u64)?;
        len.put(size as u64)
    }

    fn patch_code(
        me as Mod,
        address: u64,
        expected: *const u8,
        replacement: *const u8,
        len: usize,
    ) -> Status {
        let address = usize::try_from(address).map_err(|_| Status::BAD_ARGS)?;
        let text = crate::engine::code::text(me.loader.base).ok_or(Status::BAD_ARGS)?;
        // SAFETY: the ABI has the mod pass `len` readable bytes at a non-null
        // `expected`, unchanged for the call; `register` copies them into a box
        // before it returns.
        let expected = unsafe { slice(expected, len) };
        // SAFETY: the ABI has the mod pass `len` readable bytes at a non-null
        // `replacement`, unchanged for the call; `register` copies them into a
        // box before it returns.
        let replacement = unsafe { slice(replacement, len) };
        Ok(crate::engine::code::register(
            crate::engine::code::CodePatch {
                owner: me.index,
                feature: crate::engine::code::WHOLE_MOD,
                address,
                expected: expected.into(),
                replacement: replacement.into(),
                needs: Box::default(),
            },
            text,
        ))
    }

    fn workshop_recipes() -> u32 {
        Ok(crate::engine::code::workshop_recipes())
    }
}
