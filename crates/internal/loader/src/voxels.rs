//! Voxel reads of the server's world for mods.
//!
//! The world's ground and building blocks are a voxel store of bricks of 8³
//! voxels, half a metre a voxel. The game's box read copies a box at one
//! level of detail into a buffer of voxel words, brick by brick under the
//! store's own lock, and answers false when a brick of the box is not
//! resident (far from every player, or still streaming). The loader reads
//! at full detail on the game thread and decodes each word for the mod, so
//! the word's layout stays in the loader.

use enshwick::abi::{Status, Voxel};

use crate::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::terrain::EDITOR.id(),
    pinned::voxel::READ_BOX.id(),
    pinned::voxel::STORE.id(),
];

/// The first building material of a voxel word's low byte; below it, from
/// 1, terrain, and 0 is air (server 399, client 426).
const BUILDING_FIRST: u8 = 0x80;

/// The end the game's read takes on any axis: it compares coordinates as
/// `i32` and rounds a box's end up to its 8-voxel brick, so a larger end
/// wraps to below the world (server 399, client 426).
const SIGNED_END: u32 = i32::MAX.cast_unsigned() & !7;

const _: () = assert!(Voxel::END <= SIGNED_END);

/// A box a mod may read: within [`Voxel::END`], not empty, at most
/// [`Voxel::READ_MAX`] voxels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReadBox {
    /// The lowest corner, in voxels.
    pub(crate) min: [u32; 3],
    /// The voxels along each axis.
    pub(crate) size: [u32; 3],
    /// The voxels in the box.
    pub(crate) count: usize,
}

impl ReadBox {
    /// The box of `size` from `min` for an output of `capacity` voxels;
    /// [`Status::BAD_ARGS`] for an empty box, one past [`Voxel::END`],
    /// more than [`Voxel::READ_MAX`] voxels or a `capacity` below them.
    pub(crate) fn new(min: [u32; 3], size: [u32; 3], capacity: u64) -> Result<Self, Status> {
        let count = Voxel::read_count(min, size)
            .filter(|&count| count <= capacity)
            .and_then(|count| usize::try_from(count).ok())
            .ok_or(Status::BAD_ARGS)?;
        Ok(Self { min, size, count })
    }
}

/// The voxel a game voxel word stands for: its low byte the material, its
/// high byte the fill (server 399, client 426).
pub(crate) const fn decode(word: u16) -> Voxel {
    let [material, fill] = word.to_le_bytes();
    let kind = match material {
        0 => Voxel::AIR,
        1..BUILDING_FIRST => Voxel::TERRAIN,
        _ => Voxel::BUILDING,
    };
    Voxel {
        kind,
        material,
        fill,
        reserved: 0,
    }
}

/// The game's box read: `out` takes the buffer, `size` and `min` are in
/// voxels at level of detail `detail`, and the low byte of the result is 0
/// when a brick of the box is not resident. It reads only `out.data`,
/// `size` voxels of it, x fastest, then y (up), then z (server 399, client
/// 426).
#[cfg(windows)]
type ReadBoxFn = unsafe extern "system" fn(
    out: *mut Words,
    size: *const [u32; 3],
    editor: *mut u8,
    detail: u32,
    min: *const [u32; 3],
) -> u8;

/// The buffer the game's box read fills: words and their number.
#[cfg(windows)]
#[repr(C)]
struct Words {
    data: *mut u16,
    count: u64,
}

/// The full level of detail, half a metre a voxel.
#[cfg(windows)]
const FULL_DETAIL: u32 = 0;

/// The voxel words of `request` at full detail, x fastest, then y, then z;
/// on the game thread only. [`Status::MISSING`] while a brick of the box
/// is not resident, before a world runs or on a build without the read.
#[cfg(windows)]
pub(crate) fn read(loader: &crate::host::Loader, request: &ReadBox) -> Result<Vec<u16>, Status> {
    use crate::memory::read;

    let server = crate::world::reader()?;
    let function = loader
        .address(pinned::voxel::READ_BOX)
        .and_then(|address| usize::try_from(address).ok())
        .ok_or(Status::MISSING)?;
    let editor_at = loader
        .offset(pinned::terrain::EDITOR)
        .ok_or(Status::MISSING)?;
    let store_at = loader.offset(pinned::voxel::STORE).ok_or(Status::MISSING)?;
    // SAFETY: `server` comes from `crate::world::reader`, on the game thread
    // that alone writes it; the table's `terrain.editor` offset holds its
    // voxel editor pointer.
    let editor: usize = unsafe { read(server, editor_at) };
    if editor == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `editor` is non-zero (`editor == 0` returns first), the
    // server's voxel editor, read on the game thread that alone replaces
    // it; the table's `voxel.store` offset holds its store pointer.
    let store: usize = unsafe { read(editor, store_at) };
    if store == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the table's `voxel.read_box` is the address of the game
    // function with the `ReadBoxFn` signature on this build, in the image,
    // which stays mapped.
    let read_box = unsafe { core::mem::transmute::<usize, ReadBoxFn>(function) };
    let mut words = vec![0u16; request.count];
    let mut out = Words {
        data: words.as_mut_ptr(),
        count: words.len() as u64,
    };
    // SAFETY: `read_box` is the table's `voxel.read_box`, called on the game
    // thread (checked by `crate::world::reader`) with the server's non-zero
    // voxel editor whose store is non-zero (both checked above), which the
    // read dereferences unconditionally; `out.data` holds `request.count`
    // words, exactly the voxels of `request.size`, which the read trusts;
    // `ReadBox::new` keeps every end within `Voxel::END` and
    // `SIGNED_END`, where the read's signed loops neither wrap nor run past
    // the box.
    let resident = unsafe {
        read_box(
            &raw mut out,
            &raw const request.size,
            core::ptr::with_exposed_provenance_mut(editor),
            FULL_DETAIL,
            &raw const request.min,
        )
    };
    if resident == 0 {
        return Err(Status::MISSING);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_splits_into_material_and_fill() {
        for word in 0..=u16::MAX {
            let voxel = decode(word);
            assert_eq!(
                u16::from_le_bytes([voxel.material, voxel.fill]),
                word,
                "{word:#06x}"
            );
            assert_eq!(voxel.reserved, 0);
        }
    }

    #[test]
    fn the_low_byte_names_air_terrain_or_building() {
        assert_eq!(decode(0xff00).kind, Voxel::AIR);
        assert_eq!(decode(0x0001).kind, Voxel::TERRAIN);
        assert_eq!(decode(0x407f).kind, Voxel::TERRAIN);
        assert_eq!(decode(0x0080).kind, Voxel::BUILDING);
        assert_eq!(decode(0xffff).kind, Voxel::BUILDING);
    }

    #[test]
    fn a_box_within_bounds_counts_its_voxels() {
        assert_eq!(
            ReadBox::new([10, 20, 30], [2, 3, 4], 24).map(|b| b.count),
            Ok(24)
        );
        assert_eq!(
            ReadBox::new([0; 3], [64; 3], Voxel::READ_MAX).map(|b| b.count),
            Ok(64 * 64 * 64)
        );
    }

    #[test]
    fn an_empty_or_oversized_box_is_refused() {
        assert_eq!(ReadBox::new([0; 3], [0, 1, 1], 1), Err(Status::BAD_ARGS));
        assert_eq!(
            ReadBox::new([0; 3], [65, 64, 64], u64::MAX),
            Err(Status::BAD_ARGS)
        );
        assert_eq!(
            ReadBox::new([0; 3], [u32::MAX, u32::MAX, 2], u64::MAX),
            Err(Status::BAD_ARGS)
        );
    }

    #[test]
    fn a_capacity_below_the_box_is_refused() {
        assert_eq!(ReadBox::new([0; 3], [2, 2, 2], 7), Err(Status::BAD_ARGS));
    }

    #[test]
    fn a_box_past_the_end_is_refused() {
        let end = Voxel::END;
        assert!(ReadBox::new([end - 1, 0, 0], [1, 1, 1], 1).is_ok());
        assert_eq!(
            ReadBox::new([end, 0, 0], [1, 1, 1], 1),
            Err(Status::BAD_ARGS)
        );
        assert_eq!(
            ReadBox::new([0, u32::MAX, 0], [1, 1, 1], 1),
            Err(Status::BAD_ARGS)
        );
    }
}
