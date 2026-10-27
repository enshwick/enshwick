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

use crate::host::Loader;
use crate::memory::read;
use crate::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::COMPONENT_READ_STATIC.id(),
    pinned::ecs::REGISTRY_TYPE_RECORDS.id(),
    pinned::ecs::REGISTRY_TYPE_RECORD_SIZE.id(),
    pinned::ecs::TYPE_INFO_HASH.id(),
    pinned::ecs::TYPE_RECORD_STATIC.id(),
];

type ReadStatic =
    unsafe extern "system" fn(world: *const u8, index: *const u16, entity: u32) -> *const u8;

/// The game's static read and the registry's layout, from the build table.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StaticHalves {
    read_static: ReadStatic,
    records: usize,
    record_size: usize,
    record_static: usize,
    info_hash: usize,
}

static HALVES: OnceLock<Option<StaticHalves>> = OnceLock::new();

/// The static reads on this build; `None` on a build whose table lacks
/// them, where a static half reads as the dynamic half of its type.
pub(crate) fn halves(loader: &Loader) -> Option<&'static StaticHalves> {
    HALVES
        .get_or_init(|| {
            let halves = StaticHalves::resolve(loader);
            if halves.is_none() {
                crate::log!(
                    WARN,
                    "this build's table lacks the static component read: a static half reads as its type's dynamic half"
                );
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
        let address = loader
            .build
            .symbol(pinned::ecs::COMPONENT_READ_STATIC.id())
            .and_then(|rva| usize::try_from(loader.base + rva).ok())?;
        Some(Self {
            // SAFETY: the table's `ecs.component_read_static` is the address
            // of the game function with the `ReadStatic` signature on this
            // build, in the image, which stays mapped.
            read_static: unsafe { core::mem::transmute::<usize, ReadStatic>(address) },
            records: constant(pinned::ecs::REGISTRY_TYPE_RECORDS)?,
            record_size: constant(pinned::ecs::REGISTRY_TYPE_RECORD_SIZE)?,
            record_static: constant(pinned::ecs::TYPE_RECORD_STATIC)?,
            info_hash: constant(pinned::ecs::TYPE_INFO_HASH)?,
        })
    }

    /// Whether `hash` names the static half of the type at `index` in
    /// `world`'s registry: the registry's record of the type points at the
    /// static half's type info, which carries the hash.
    ///
    /// # Safety
    ///
    /// - `world` is a live entity world, whose first word is its component
    ///   registry, read on a thread where the world is not torn down.
    /// - `index` is the non-null type index the registry's lookup returned
    ///   for `hash`.
    pub(crate) unsafe fn is_static(&self, world: usize, index: *const u16, hash: u32) -> bool {
        // SAFETY: the caller guarantees a live entity world, which starts
        // with its component registry pointer.
        let registry: usize = unsafe { read(world, 0) };
        if registry == 0 {
            return false;
        }
        // SAFETY: `index` is the non-null type index of the registry's
        // lookup, by the caller's contract, a u16.
        let index = usize::from(unsafe { index.read_unaligned() });
        // SAFETY: `registry` is the world's non-zero component registry;
        // the table's `ecs.registry_type_records` offset holds its record
        // array, filled when the world's ECS is set up and not grown after.
        let records: usize = unsafe { read(registry, self.records) };
        if records == 0 {
            return false;
        }
        let record = records + index * self.record_size;
        // SAFETY: `record` is the record of the type at `index`, which
        // indexes the record array as it does the registry's other tables;
        // the table's `ecs.type_record_static` offset holds its static
        // half's type info, null for a type without one.
        let info: usize = unsafe { read(record, self.record_static) };
        if info == 0 {
            return false;
        }
        // SAFETY: `info` is the static half's non-null type info in the
        // game's image, which stays mapped; the table's `ecs.type_info_hash`
        // offset holds its eight-byte qualified hash, the type hash in its
        // low half.
        let qualified: u64 = unsafe { read(info, self.info_hash) };
        static_hash(qualified) == hash
    }

    /// The static half of `entity`'s component of the type at `index`: the
    /// template's set-up data, read-only; null when the entity is not in
    /// the world or lacks the type.
    ///
    /// # Safety
    ///
    /// - As for [`StaticHalves::is_static`], which returned true for
    ///   `index`: the game's read does not check that the type has a
    ///   static half.
    pub(crate) unsafe fn read(&self, world: usize, index: *const u16, entity: u32) -> *const u8 {
        // SAFETY: `read_static` is the table's `ecs.component_read_static`,
        // which reads the world's entity map and the entity's record without
        // a lock; the caller guarantees a live world and the index of a type
        // with a static half.
        unsafe { (self.read_static)(core::ptr::with_exposed_provenance(world), index, entity) }
    }
}

/// The type hash in a type info's qualified hash: its low half.
#[expect(clippy::cast_possible_truncation, reason = "the hash is the low half")]
const fn static_hash(qualified: u64) -> u32 {
    qualified as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD_SIZE: usize = 0x100;

    /// An entity world whose registry has records for two type indices:
    /// index 1 with a static half hashed `static_hash`, index 0 without.
    struct Registry {
        world: Box<[usize; 1]>,
        _types: Box<[usize; 4]>,
        _records: Box<[u8; 2 * RECORD_SIZE]>,
        _info: Box<[u64; 16]>,
    }

    fn registry(static_hash: u32) -> Registry {
        let mut info = Box::new([0u64; 16]);
        info[0x50 / 8] = 0xdead_beef_0000_0000 | u64::from(static_hash);
        let mut records = Box::new([0u8; 2 * RECORD_SIZE]);
        let at = RECORD_SIZE + 0x30;
        records[at..at + 8].copy_from_slice(&(info.as_ptr().expose_provenance()).to_le_bytes());
        let mut registry = Box::new([0usize; 4]);
        registry[1] = records.as_ptr().expose_provenance();
        let world = Box::new([registry.as_ptr().expose_provenance()]);
        Registry {
            world,
            _types: registry,
            _records: records,
            _info: info,
        }
    }

    extern "system" fn no_read(_: *const u8, _: *const u16, _: u32) -> *const u8 {
        core::ptr::null()
    }

    fn halves() -> StaticHalves {
        StaticHalves {
            read_static: no_read,
            records: 0x8,
            record_size: RECORD_SIZE,
            record_static: 0x30,
            info_hash: 0x50,
        }
    }

    #[test]
    fn a_hash_names_the_static_half_only_when_the_record_carries_it() {
        let registry = registry(0x139a_fbec);
        let world = registry.world.as_ptr().expose_provenance();
        let halves = halves();
        let (with, without) = (1u16, 0u16);
        // SAFETY: `world` is the test's entity world; its registry, records
        // and type info live on the heap for the test, and `with` names its
        // record with a static half.
        assert!(unsafe { halves.is_static(world, &raw const with, 0x139a_fbec) });
        // SAFETY: `world` is the test's entity world, whose registry, records
        // and type info live on the heap for the test; `with` names a record.
        let dynamic = unsafe { halves.is_static(world, &raw const with, 0x0418_2abb) };
        assert!(!dynamic, "the dynamic half");
        // SAFETY: `world` is the test's entity world, whose registry and
        // records live on the heap for the test; `without` names a record.
        let plain = unsafe { halves.is_static(world, &raw const without, 0x139a_fbec) };
        assert!(!plain, "no static half");
    }

    #[test]
    fn the_type_hash_is_the_low_half() {
        assert_eq!(static_hash(0xdead_beef_e41a_0d1f), 0xe41a_0d1f);
    }
}
