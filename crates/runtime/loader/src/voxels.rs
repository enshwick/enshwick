//! Voxel reads of the server's world for mods.
//!
//! The world's ground and building blocks are a voxel store of bricks of 8³
//! voxels, half a metre a voxel. The game's box read copies a box at one
//! level of detail into a buffer of voxel words, brick by brick under the
//! store's own lock, and answers false when a brick of the box is not
//! resident; that read also makes the server load the column, about a
//! second later (server 399, server copy). The store numbers
//! its levels coarse to fine; the loader reads at the finest, half a metre
//! a voxel, on the game thread at a quiet moment, since the read takes the
//! editor's lock for each column and a step may hold it, and decodes each
//! word for the mod, so the word's layout stays in the loader. A column
//! past the editor's column grid reads as air with no error, so the loader
//! refuses a box that leaves the grid, or rises above [`HEIGHT`], itself.

pub(crate) mod changes;

use enshwick_core::abi::{Status, Voxel};

use crate::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::terrain::EDITOR.id(),
    pinned::voxel::COMMIT.id(),
    pinned::voxel::FINEST_LEVEL.id(),
    pinned::voxel::GRIDS.id(),
    pinned::voxel::GRID_DEPTH.id(),
    pinned::voxel::GRID_PAGES.id(),
    pinned::voxel::GRID_SLOTS.id(),
    pinned::voxel::GRID_STRIDE.id(),
    pinned::voxel::PAGE_DROPPED.id(),
    pinned::voxel::PAGE_LOADED.id(),
    pinned::voxel::READ_BOX.id(),
    pinned::voxel::STORE.id(),
];

/// The materials a voxel word's low byte names, as the build table's
/// `voxel.materials` counts them; the per-material arrays the game reads are
/// this long.
pub(crate) const MATERIALS: usize = 0x100; // bound, not a game offset: an array length that `crate::terrain` checks against the table before any use
/// The terrain materials: above air, 0, and below the first building
/// material, the build table's `voxel.building_first` (server 399, client
/// 426); `crate::terrain` checks both against the running build.
pub(crate) const TERRAIN: core::ops::RangeInclusive<u8> = 1..=0x7f;
/// The first building material; building configs number theirs from it,
/// 0-based.
pub(crate) const BUILDING_FIRST: u8 = *TERRAIN.end() + 1;

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

/// The height of the world a box may reach, in voxels: 1,024 metres.
/// The game keeps no height: each column holds as many bricks as its
/// ground reaches and reads as air above (server 399, client 426). The
/// cap is Enshwick's, above every place the game builds.
pub(crate) const HEIGHT: u32 = 2048;

/// The voxels of the world on each axis, x, y and z: the game's own edge
/// across, the loader's [`HEIGHT`] up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorldExtent([u32; 3]);

impl WorldExtent {
    /// The extent of a column grid of `width` by `depth` pages, `packed`
    /// holding the bricks of 8 voxels along a page's x at bits 3 to 31 and
    /// along its z from bit 35 (server 399, client 426); `None` for an
    /// empty grid or one past [`Voxel::END`].
    pub(crate) fn from_grid(packed: u64, width: u32, depth: u32) -> Option<Self> {
        let across = |pages: u32, bricks: u64| {
            u64::from(pages)
                .checked_mul(bricks)?
                .checked_mul(8)
                .filter(|&voxels| voxels > 0 && voxels <= u64::from(Voxel::END))
                .and_then(|voxels| u32::try_from(voxels).ok())
        };
        let x = across(width, (packed >> 3) & 0x1fff_ffff)?;
        let z = across(depth, packed >> 35)?;
        Some(Self([x, HEIGHT, z]))
    }

    /// Whether the box of `size` from `min` lies wholly inside.
    pub(crate) fn holds(self, min: [u32; 3], size: [u32; 3]) -> bool {
        (0..3).all(|axis| {
            min[axis]
                .checked_add(size[axis])
                .is_some_and(|end| end <= self.0[axis])
        })
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

/// The game's voxel word for `voxel`, the inverse of [`decode`];
/// [`Status::BAD_ARGS`] when its kind is not the one its material names,
/// or its reserved byte is set.
pub(crate) const fn encode(voxel: Voxel) -> Result<u16, Status> {
    if decode(voxel.material as u16).kind != voxel.kind || voxel.reserved != 0 {
        return Err(Status::BAD_ARGS);
    }
    Ok(u16::from_le_bytes([voxel.material, voxel.fill]))
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

/// Where the editor holds the field of `bytes` a build table places at
/// `field` in its first column grid, at `grids`, for the grid at `grid`;
/// `None` when the field does not lie inside one grid of `stride` bytes.
fn grid_field(
    grids: usize,
    stride: usize,
    grid: usize,
    field: usize,
    bytes: usize,
) -> Option<usize> {
    field
        .checked_sub(grids)
        .filter(|&field| stride.checked_sub(field) >= Some(bytes))
        .and_then(|field| field.checked_add(grid))
}

/// The store's finest level, half a metre a voxel, and where the editor
/// holds its column grid, from the build table: the grid starts with the
/// bricks a page holds, packed in a `u64`, then its pages along x and z.
#[cfg(windows)]
struct Finest {
    level: u32,
    grid: usize,
    width: usize,
    depth: usize,
    /// The pointer to the grid's slots, `None` on a build without
    /// `voxel.grid_slots`, which only the page events need.
    slots: Option<usize>,
}

#[cfg(windows)]
fn finest(loader: &crate::host::Loader) -> Result<Finest, Status> {
    let constant = |id| loader.offset(id).ok_or(Status::MISSING);
    let level = constant(pinned::voxel::FINEST_LEVEL)?;
    let grids = constant(pinned::voxel::GRIDS)?;
    let pages = constant(pinned::voxel::GRID_PAGES)?;
    let stride = constant(pinned::voxel::GRID_STRIDE)?;
    let grid = level
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(grids))
        .ok_or(Status::MISSING)?;
    let in_grid =
        |field| grid_field(grids, stride, grid, field, size_of::<u32>()).ok_or(Status::MISSING);
    Ok(Finest {
        level: u32::try_from(level).map_err(|_| Status::MISSING)?,
        grid,
        width: in_grid(pages)?,
        depth: in_grid(constant(pinned::voxel::GRID_DEPTH)?)?,
        slots: loader
            .offset(pinned::voxel::GRID_SLOTS)
            .and_then(|field| grid_field(grids, stride, grid, field, size_of::<u64>())),
    })
}

/// The server's voxel editor, on the game thread at a quiet moment.
#[cfg(windows)]
fn editor(loader: &crate::host::Loader) -> Result<usize, Status> {
    use crate::memory::read;

    let server = crate::world::server()?;
    let editor_at = loader
        .offset(pinned::terrain::EDITOR)
        .ok_or(Status::MISSING)?;
    // SAFETY: `server` comes from `crate::world::server`, on the game thread
    // that alone writes it; the table's `terrain.editor` offset holds its
    // voxel editor pointer.
    let editor: usize = unsafe { read(server, editor_at) };
    if editor == 0 {
        return Err(Status::MISSING);
    }
    Ok(editor)
}

/// The world's extent from the editor's finest column grid, the bounds the
/// game's own column lookup takes; on the game thread at a quiet moment.
#[cfg(windows)]
pub(crate) fn extent(loader: &crate::host::Loader) -> Result<WorldExtent, Status> {
    use crate::memory::read;

    let finest = finest(loader)?;
    let editor = editor(loader)?;
    // SAFETY: `editor` is the server's non-zero voxel editor, read on the
    // game thread that alone replaces it; the table's `voxel.grids`,
    // `voxel.grid_stride` and `voxel.finest_level` place its finest column
    // grid at `finest.grid`, which starts with its packed bricks per page.
    let packed: u64 = unsafe { read(editor, finest.grid) };
    // SAFETY: `editor` is the server's non-zero voxel editor, read on the
    // game thread; the table's `voxel.grid_pages` places the finest grid's
    // pages along x at `finest.width`.
    let width: u32 = unsafe { read(editor, finest.width) };
    // SAFETY: `editor` is the server's non-zero voxel editor, read on the
    // game thread; the table's `voxel.grid_depth` places the finest grid's
    // pages along z at `finest.depth`.
    let depth: u32 = unsafe { read(editor, finest.depth) };
    WorldExtent::from_grid(packed, width, depth).ok_or(Status::MISSING)
}

/// The pages a finest column grid may hold for the page events to scan it:
/// 80 by 80 live (server 26430253, server copy), so a grid read wrong is
/// refused, not scanned.
const MAX_PAGES: u32 = 1 << 16;

/// Voxels per metre on every axis.
const PER_METRE: u32 = 2;

/// The finest column grid's slots, one cache handle per page at
/// `z * width + x`, for the page events.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(
    not(any(windows, test)),
    expect(dead_code, reason = "only the Windows page events read the slots")
)]
pub(crate) struct Slots {
    /// The aligned array of `count` `u32` slots.
    pub(crate) at: usize,
    /// Pages along x.
    pub(crate) width: u32,
    /// Pages in the grid.
    pub(crate) count: u32,
    /// A page's side in metres.
    pub(crate) metres: u32,
}

impl Slots {
    /// The slots at `at` of a grid of `width` by `depth` pages, `packed` as
    /// [`WorldExtent::from_grid`] reads it; `None` unless the pages are
    /// square, whole metres and at most [`MAX_PAGES`], and `at` is a
    /// non-zero aligned pointer.
    #[cfg_attr(
        not(any(windows, test)),
        expect(dead_code, reason = "only the Windows page events read the slots")
    )]
    fn from_grid(at: usize, packed: u64, width: u32, depth: u32) -> Option<Self> {
        let count = width
            .checked_mul(depth)
            .filter(|&count| count > 0 && count <= MAX_PAGES)?;
        let [across, _, along] = WorldExtent::from_grid(packed, width, depth)?.0;
        let side = across / width;
        let fits = at != 0
            && at.is_multiple_of(size_of::<u32>())
            && along / depth == side
            && side.is_multiple_of(PER_METRE);
        fits.then_some(Self {
            at,
            width,
            count,
            metres: side / PER_METRE,
        })
    }
}

/// `Ok` when the build table places the finest column grid's slots, else
/// the first entry it lacks for them.
#[cfg(windows)]
pub(crate) fn finest_slots(loader: &crate::host::Loader) -> Result<(), pinned::Constant> {
    let needed = [
        pinned::voxel::FINEST_LEVEL,
        pinned::voxel::GRIDS,
        pinned::voxel::GRID_PAGES,
        pinned::voxel::GRID_STRIDE,
        pinned::voxel::GRID_DEPTH,
        pinned::voxel::GRID_SLOTS,
    ];
    if let Some(&missing) = needed.iter().find(|&&id| loader.offset(id).is_none()) {
        return Err(missing);
    }
    finest(loader)
        .ok()
        .and_then(|finest| finest.slots)
        .map(drop)
        .ok_or(pinned::voxel::GRID_SLOTS)
}

/// The finest column grid's slots of the editor the voxel API reads; on the
/// game thread at a quiet moment. [`Status::MISSING`] before a world runs,
/// on a build without the slots, or for a grid of pages that are not
/// square, not whole metres or more than [`MAX_PAGES`].
#[cfg(windows)]
pub(crate) fn slots(loader: &crate::host::Loader) -> Result<Slots, Status> {
    use crate::memory::read;

    let finest = finest(loader)?;
    let slots_at = finest.slots.ok_or(Status::MISSING)?;
    let editor = editor(loader)?;
    // SAFETY: `editor` is the server's non-zero voxel editor, read on the
    // game thread that alone replaces it; the table's `voxel.grids`,
    // `voxel.grid_stride` and `voxel.finest_level` place its finest column
    // grid at `finest.grid`, which starts with its packed bricks per page.
    let packed: u64 = unsafe { read(editor, finest.grid) };
    // SAFETY: `editor` is the server's non-zero voxel editor, read on the
    // game thread; the table's `voxel.grid_pages` places the finest grid's
    // pages along x at `finest.width`.
    let width: u32 = unsafe { read(editor, finest.width) };
    // SAFETY: `editor` is the server's non-zero voxel editor, read on the
    // game thread; the table's `voxel.grid_depth` places the finest grid's
    // pages along z at `finest.depth`.
    let depth: u32 = unsafe { read(editor, finest.depth) };
    // SAFETY: `editor` is the server's non-zero voxel editor, read on the
    // game thread; the table's `voxel.grid_slots` places the finest grid's
    // slot pointer at `slots_at`, 8 bytes inside the grid (checked by
    // `grid_field` in `finest`).
    let at: usize = unsafe { read(editor, slots_at) };
    Slots::from_grid(at, packed, width, depth).ok_or(Status::MISSING)
}

/// The voxel words of `request` at the finest level, x fastest, then y, then z;
/// on the game thread at a quiet moment only. [`Status::NOT_LOADED`] while
/// a brick of the box is not resident, [`Status::OUTSIDE_WORLD`] for a box
/// that leaves the world, [`Status::MISSING`] before a world runs or on a
/// build without the read.
#[cfg(windows)]
pub(crate) fn read(loader: &crate::host::Loader, request: &ReadBox) -> Result<Vec<u16>, Status> {
    use crate::memory::read;

    let function = loader
        .address(pinned::voxel::READ_BOX)
        .and_then(|address| usize::try_from(address).ok())
        .ok_or(Status::MISSING)?;
    let store_at = loader.offset(pinned::voxel::STORE).ok_or(Status::MISSING)?;
    let level = finest(loader)?.level;
    let editor = editor(loader)?;
    if !extent(loader)?.holds(request.min, request.size) {
        return Err(Status::OUTSIDE_WORLD);
    }
    // SAFETY: `editor` is the server's non-zero voxel editor (checked by
    // `editor`), read on the game thread that alone replaces it; the
    // table's `voxel.store` offset holds its store pointer.
    let store: usize = unsafe { read(editor, store_at) };
    if store == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the table's `voxel.read_box` is the address of the game
    // function with the `ReadBoxFn` signature on this build, in the image,
    // which stays mapped.
    let read_box = unsafe { core::mem::transmute::<usize, ReadBoxFn>(function) };
    let mut words = vec![0u16; request.count];
    debug_assert_eq!(
        words.len() as u128,
        request
            .size
            .iter()
            .map(|&axis| u128::from(axis))
            .product::<u128>(),
        "the buffer holds exactly the voxels of the box"
    );
    let mut out = Words {
        data: words.as_mut_ptr(),
        count: words.len() as u64,
    };
    // SAFETY: `read_box` is the table's `voxel.read_box`, called on the game
    // thread at a quiet moment, where no step holds the editor's column lock
    // (checked by `crate::world::server` in `editor`), with the server's
    // non-zero voxel editor whose store is non-zero (both checked above),
    // which the read dereferences unconditionally. The read writes the
    // product of `request.size` words at `out.data` without a bound:
    // `ReadBox::new` takes `request.count` from `Voxel::read_count`, which
    // multiplies with overflow checks, so `request.count` is that product
    // and at most `Voxel::READ_MAX`, and `words` holds `request.count`
    // words. Every end lies inside the world's extent (checked above) and
    // within `Voxel::END`, below `SIGNED_END`, where the read's signed loops
    // neither wrap nor run past the box.
    let resident = unsafe {
        read_box(
            &raw mut out,
            &raw const request.size,
            core::ptr::with_exposed_provenance_mut(editor),
            level,
            &raw const request.min,
        )
    };
    if resident == 0 {
        return Err(Status::NOT_LOADED);
    }
    Ok(words)
}

/// `Ok` when the column of bricks holding the voxel at `at` is loaded;
/// [`Status::NOT_LOADED`] when not. The game keeps a column loaded or not
/// as one, and its box read answers false for a column without data and
/// starts loading it. An editor write there computes the edit against air
/// and merges it onto the real ground when that arrives (server 399,
/// server copy).
#[cfg(windows)]
pub(crate) fn column_loaded(loader: &crate::host::Loader, at: [u32; 3]) -> Result<(), Status> {
    read(loader, &ReadBox::new(at, [1; 3], 1)?).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grid of the tested save on server 399: 80 by 80 pages of 32 by
    /// 32 bricks.
    const SAVE_GRID: (u64, u32, u32) = (32 << 3 | 32 << 35, 80, 80);

    #[test]
    fn the_extent_is_the_finest_grid_s_pages_of_bricks() {
        let (packed, width, depth) = SAVE_GRID;
        let extent = WorldExtent::from_grid(packed, width, depth).unwrap();
        assert_eq!(extent, WorldExtent([20_480, HEIGHT, 20_480]));
    }

    /// Pages of `x` by `z` bricks, packed as the finest grid holds them.
    fn packed(x: u64, z: u64) -> u64 {
        x << 3 | z << 35
    }

    #[test]
    fn slots_take_a_grid_of_square_pages_and_refuse_a_grid_read_wrong() {
        let live = Slots::from_grid(0x1000, packed(32, 32), 80, 80);
        assert_eq!(
            live,
            Some(Slots {
                at: 0x1000,
                width: 80,
                count: 6400,
                metres: 128
            })
        );
        assert_eq!(
            Slots::from_grid(0x1000, packed(1, 1), 256, 256).map(|s| s.count),
            Some(MAX_PAGES)
        );
        assert_eq!(
            Slots::from_grid(0x1000, packed(1, 1), 257, 256),
            None,
            "too many pages"
        );
        assert_eq!(
            Slots::from_grid(0x1000, packed(32, 16), 80, 80),
            None,
            "not square"
        );
        assert_eq!(
            Slots::from_grid(0x1000, packed(32, 32), 80, 0),
            None,
            "no rows"
        );
        assert_eq!(Slots::from_grid(0, packed(32, 32), 80, 80), None, "null");
        assert_eq!(
            Slots::from_grid(0x1002, packed(32, 32), 80, 80),
            None,
            "unaligned"
        );
    }

    #[test]
    fn a_grid_field_lies_inside_one_grid_or_reads_nothing() {
        assert_eq!(grid_field(0x40, 0x28, 0x130, 0x48, 4), Some(0x138));
        assert_eq!(grid_field(0x40, 0x28, 0x130, 0x4c, 4), Some(0x13c));
        assert_eq!(grid_field(0x40, 0x28, 0x130, 0x64, 4), Some(0x154));
        assert_eq!(
            grid_field(0x40, 0x28, 0x130, 0x65, 4),
            None,
            "past the grid"
        );
        assert_eq!(grid_field(0x40, 0x28, 0x130, 0x58, 8), Some(0x148));
        assert_eq!(
            grid_field(0x40, 0x28, 0x130, 0x64, 8),
            None,
            "a pointer past the grid"
        );
        assert_eq!(
            grid_field(0x40, 0x28, 0x130, 0x3c, 4),
            None,
            "before the grids"
        );
        assert_eq!(grid_field(0x40, 0x28, 0x130, usize::MAX, 4), None);
        assert_eq!(grid_field(0x40, 0x28, usize::MAX, 0x48, 4), None);
    }

    #[test]
    fn an_empty_or_overflowing_grid_has_no_extent() {
        let (packed, width, depth) = SAVE_GRID;
        assert_eq!(WorldExtent::from_grid(packed, 0, depth), None);
        assert_eq!(WorldExtent::from_grid(0, width, depth), None);
        assert_eq!(WorldExtent::from_grid(packed, u32::MAX, depth), None);
    }

    #[test]
    fn a_box_holds_only_inside_every_edge() {
        let (packed, width, depth) = SAVE_GRID;
        let extent = WorldExtent::from_grid(packed, width, depth).unwrap();
        assert!(extent.holds([0; 3], [1; 3]));
        assert!(extent.holds([20_479, HEIGHT - 1, 20_479], [1; 3]));
        assert!(extent.holds([20_416, 0, 0], [64, 64, 64]));
        assert!(!extent.holds([20_480, 0, 0], [1; 3]));
        assert!(!extent.holds([20_417, 0, 0], [64, 1, 1]));
        assert!(!extent.holds([0, HEIGHT, 0], [1; 3]));
        assert!(!extent.holds([0, 0, 20_479], [1, 1, 2]));
        assert!(!extent.holds([0, 0, u32::MAX], [1, 1, 2]));
    }

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
    fn every_decoded_word_encodes_back_to_itself() {
        for word in 0..=u16::MAX {
            assert_eq!(encode(decode(word)), Ok(word), "{word:#06x}");
        }
    }

    #[test]
    fn a_voxel_whose_kind_and_material_disagree_does_not_encode() {
        let voxel = |kind, material| Voxel {
            kind,
            material,
            fill: 0xff,
            reserved: 0,
        };
        for bad in [
            voxel(Voxel::AIR, 1),
            voxel(Voxel::TERRAIN, 0),
            voxel(Voxel::TERRAIN, 0x80),
            voxel(Voxel::BUILDING, 0x7f),
            voxel(3, 0),
            Voxel {
                reserved: 1,
                ..voxel(Voxel::AIR, 0)
            },
        ] {
            assert_eq!(encode(bad), Err(Status::BAD_ARGS), "{bad:?}");
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
    fn a_box_whose_voxel_count_overflows_is_refused() {
        assert_eq!(
            ReadBox::new([0; 3], [1 << 30, 1 << 30, 16], u64::MAX),
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
