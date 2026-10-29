//! The static halves of ECS component types: the set-up values an entity's
//! template holds and every entity of it shares.
//!
//! One ECS type can have two reflected halves, such as `FuelSetup` and
//! `DynamicFuelSetup`, and both hashes find the same type index. The raw
//! component read always returns the dynamic half, kept in the entity's
//! row; the static half is read with the game's static read. Which half a
//! hash names is in the registry's record of the type (server 399, client
//! 426).

use std::sync::OnceLock;

use enshwick::abi::Status;

use crate::host::Loader;
use crate::memory::read;
use crate::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::COMPONENT_READ_STATIC.id(),
    pinned::ecs::REGISTRY_TYPE_COUNT.id(),
    pinned::ecs::REGISTRY_TYPE_RECORDS.id(),
    pinned::ecs::REGISTRY_TYPE_RECORD_SIZE.id(),
    pinned::ecs::TYPE_INFO_HASH.id(),
    pinned::ecs::TYPE_RECORD_STATIC.id(),
];

type ReadStatic =
    unsafe extern "system" fn(world: *const u8, index: *const u16, entity: u32) -> *const u8;

/// The registry's layout of its type records, which tells the halves
/// apart, and the game's static read, from the build table.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StaticHalves {
    /// The static read, on builds whose table has it; without it a static
    /// half is still recognised, and its reads fail.
    read_static: Option<ReadStatic>,
    records: usize,
    record_size: usize,
    count: usize,
    record_static: usize,
    info_hash: usize,
}

static HALVES: OnceLock<Option<StaticHalves>> = OnceLock::new();

/// The halves on this build; `None` on a build whose table lacks the
/// registry's layout, where a static half reads as the dynamic half of its
/// type.
pub(crate) fn halves(loader: &Loader) -> Option<&'static StaticHalves> {
    HALVES
        .get_or_init(|| {
            let halves = StaticHalves::resolve(loader);
            match halves {
                None => crate::log!(
                    WARN,
                    "this build's table lacks the component registry's layout: a static half reads as its type's dynamic half"
                ),
                Some(halves) if halves.read_static.is_none() => crate::log!(
                    WARN,
                    "this build's table lacks the static component read: static halves are not read"
                ),
                Some(_) => {}
            }
            halves
        })
        .as_ref()
}

impl StaticHalves {
    fn resolve(loader: &Loader) -> Option<Self> {
        let constant = |id: pinned::Constant| {
            loader
                .build
                .constant(id.id())
                .and_then(|value| usize::try_from(value).ok())
        };
        let read_static = loader
            .build
            .symbol(pinned::ecs::COMPONENT_READ_STATIC.id())
            .and_then(|rva| usize::try_from(loader.base + rva).ok())
            .map(|address| {
                // SAFETY: the table's `ecs.component_read_static` is the
                // address of the game function with the `ReadStatic`
                // signature on this build, in the image, which stays mapped.
                unsafe { core::mem::transmute::<usize, ReadStatic>(address) }
            });
        Some(Self {
            read_static,
            records: constant(pinned::ecs::REGISTRY_TYPE_RECORDS)?,
            record_size: constant(pinned::ecs::REGISTRY_TYPE_RECORD_SIZE)?,
            count: constant(pinned::ecs::REGISTRY_TYPE_COUNT)?,
            record_static: constant(pinned::ecs::TYPE_RECORD_STATIC)?,
            info_hash: constant(pinned::ecs::TYPE_INFO_HASH)?,
        })
    }

    /// Whether `hash` names the static half of the type at `index` in
    /// `registry`: the registry's record of the type points at the static
    /// half's type info, which carries the hash.
    ///
    /// # Safety
    ///
    /// - `registry` is the non-zero component registry of a live entity
    ///   world, read on a thread where the world is not torn down.
    /// - `index` is the non-null type index the registry's lookup returned
    ///   for `hash`.
    pub(crate) unsafe fn is_static(&self, registry: usize, index: *const u16, hash: u32) -> bool {
        // SAFETY: `index` is the non-null type index of the registry's
        // lookup, by the caller's contract, a u16.
        let index = usize::from(unsafe { index.read_unaligned() });
        // SAFETY: `registry` is a live component registry, by the caller's
        // contract; the table's `ecs.registry_type_count` offset holds the
        // number of its type records.
        let count: usize = unsafe { read(registry, self.count) };
        if index >= count {
            return false;
        }
        // SAFETY: `registry` is a live component registry; the table's
        // `ecs.registry_type_records` offset holds its record array, filled
        // when the world's ECS is set up and not grown after.
        let records: usize = unsafe { read(registry, self.records) };
        if records == 0 {
            return false;
        }
        let record = records + index * self.record_size;
        // SAFETY: `record` is the record of the type at `index`, below the
        // registry's count (checked above), which indexes the record array
        // as it does the registry's other tables; the table's
        // `ecs.type_record_static` offset holds its static half's type
        // info, null for a type without one.
        let info: usize = unsafe { read(record, self.record_static) };
        if info == 0 {
            return false;
        }
        // SAFETY: `info` is the static half's non-null type info in the
        // game's image, which stays mapped; the table's `ecs.type_info_hash`
        // offset holds its type hash, a u32, as the game reads it.
        let static_hash: u32 = unsafe { read(info, self.info_hash) };
        static_hash == hash
    }

    /// The static half of `entity`'s component of the type at `index`: the
    /// template's set-up data, read-only. [`Status::MISSING`] when the entity
    /// is not in the world or lacks the type, or on a build without the
    /// static read.
    ///
    /// # Safety
    ///
    /// - `world` is a live entity world, read on a thread where it is not
    ///   torn down.
    /// - [`StaticHalves::is_static`] returned true for `index` in `world`'s
    ///   registry: the game's read does not check that the type has a
    ///   static half.
    pub(crate) unsafe fn read(
        &self,
        world: usize,
        index: *const u16,
        entity: u32,
    ) -> Result<*const u8, Status> {
        let read_static = self.read_static.ok_or(Status::MISSING)?;
        // SAFETY: `read_static` is the table's `ecs.component_read_static`,
        // which reads the world's entity map and the entity's record without
        // a lock; the caller guarantees a live world and the index of a type
        // with a static half.
        let data = unsafe { read_static(core::ptr::with_exposed_provenance(world), index, entity) };
        if data.is_null() {
            Err(Status::MISSING)
        } else {
            Ok(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD_SIZE: usize = 0x100; // bound, not a game offset: a test's memory
    const SETUP: u32 = 0x139a_fbec;

    /// A component registry with two type records: index 1 with a static
    /// half hashed [`SETUP`], index 0 without; its count says 2.
    struct Registry {
        types: Box<[usize; 4]>,
        _records: Box<[u8; 2 * RECORD_SIZE]>,
        _info: Box<[u32; 32]>,
    }

    impl Registry {
        fn address(&self) -> usize {
            self.types.as_ptr().expose_provenance()
        }
    }

    fn registry() -> Registry {
        let mut info = Box::new([0u32; 32]);
        info[0x50 / 4] = SETUP;
        info[0x54 / 4] = 0xdead_beef;
        let mut records = Box::new([0u8; 2 * RECORD_SIZE]);
        let at = RECORD_SIZE + 0x30;
        records[at..at + 8].copy_from_slice(&info.as_ptr().expose_provenance().to_le_bytes());
        let mut types = Box::new([0usize; 4]);
        types[1] = records.as_ptr().expose_provenance();
        types[2] = 2;
        Registry {
            types,
            _records: records,
            _info: info,
        }
    }

    extern "system" fn no_read(_: *const u8, _: *const u16, _: u32) -> *const u8 {
        core::ptr::null()
    }

    fn halves(read_static: Option<ReadStatic>) -> StaticHalves {
        StaticHalves {
            read_static,
            records: 0x8,
            record_size: RECORD_SIZE,
            count: 0x10,
            record_static: 0x30,
            info_hash: 0x50,
        }
    }

    /// Whether `hash` names the static half at `index` in the test's
    /// registry.
    fn names(registry: &Registry, index: u16, hash: u32) -> bool {
        // SAFETY: the test's registry, its records and its type info live on
        // the heap for the call, laid out as the halves' layout says.
        unsafe { halves(Some(no_read)).is_static(registry.address(), &raw const index, hash) }
    }

    #[test]
    fn a_hash_names_the_static_half_only_when_the_record_carries_it() {
        let registry = registry();
        assert!(names(&registry, 1, SETUP));
        assert!(!names(&registry, 1, 0x0418_2abb), "the dynamic half");
        assert!(!names(&registry, 0, SETUP), "a type without a static half");
    }

    #[test]
    fn an_index_past_the_registry_s_records_is_no_static_half() {
        let registry = registry();
        assert!(!names(&registry, 2, SETUP));
        assert!(!names(&registry, u16::MAX, SETUP));
    }

    #[test]
    fn a_build_without_the_static_read_refuses_the_read() {
        let index = 1u16;
        // SAFETY: the read is refused before any call, so neither the world
        // nor the index is read.
        let read = unsafe { halves(None).read(0, &raw const index, 7) };
        assert_eq!(read, Err(Status::MISSING));
        // SAFETY: `no_read` reads nothing and answers null.
        let read = unsafe { halves(Some(no_read)).read(0, &raw const index, 7) };
        assert_eq!(read, Err(Status::MISSING), "an entity without the type");
    }
}
