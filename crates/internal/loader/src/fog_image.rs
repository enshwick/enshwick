//! The loader's copy of the client's fog of war, kept as one image.
//!
//! The game keeps the fog in square tiles of one byte per pixel, the
//! coverage from 0 (hidden) to 255 (uncovered), row 0 at the world's high
//! z edge. The copy is one row-major image in the same orientation, which
//! mods read by pixel.

use core::ops::Range;

/// More tiles across than any world has: a count past it is a misread.
const MAX_TILES: u32 = 256;
/// A larger tile edge is a misread.
const MAX_EDGE: u32 = 256;

/// The shape of the game's fog: its tiles and the world they cover. The
/// game reads only the world's z size; its pixels are square, one scale
/// for both axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogShape {
    tiles_wide: u32,
    tiles_high: u32,
    edge: u32,
    size_z: f32,
}

/// The tiles a reveal touches, by tile column and row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileRect {
    /// Tile columns.
    pub x: Range<u32>,
    /// Tile rows.
    pub y: Range<u32>,
}

impl FogShape {
    /// The shape read from the game, `None` when a value cannot be one the
    /// game holds.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::fog_image::FogShape;
    ///
    /// let shape = FogShape::new(40, 40, 32, 10_240.0).unwrap();
    /// assert_eq!(shape.size(), [1280, 1280]);
    /// assert_eq!(shape.metres_per_pixel(), 8.0);
    /// assert!(FogShape::new(0, 40, 32, 10_240.0).is_none());
    /// ```
    #[must_use]
    pub fn new(tiles_wide: u32, tiles_high: u32, edge: u32, size_z: f32) -> Option<Self> {
        let counts = [tiles_wide, tiles_high]
            .iter()
            .all(|count| (1..=MAX_TILES).contains(count));
        let size = size_z.is_finite() && size_z > 0.0;
        (counts && size && (1..=MAX_EDGE).contains(&edge)).then_some(Self {
            tiles_wide,
            tiles_high,
            edge,
            size_z,
        })
    }

    /// The image's width and height in pixels.
    ///
    /// # Examples
    ///
    /// See [`FogShape::new`].
    #[must_use]
    pub const fn size(&self) -> [u32; 2] {
        [self.tiles_wide * self.edge, self.tiles_high * self.edge]
    }

    /// The tile count across and down.
    #[must_use]
    pub const fn tiles(&self) -> [u32; 2] {
        [self.tiles_wide, self.tiles_high]
    }

    /// A tile's edge in pixels.
    #[must_use]
    pub const fn edge(&self) -> u32 {
        self.edge
    }

    /// The metres one pixel covers along each axis, from the z size and the
    /// pixel rows: the game reads no scale of its own here, and its fixed
    /// 8 m per pixel matches while the world is a multiple of a tile.
    #[must_use]
    pub fn metres_per_pixel(&self) -> f64 {
        f64::from(self.size_z) / f64::from(self.size()[1])
    }

    /// The world's z size the fog covers, in metres.
    #[must_use]
    pub const fn size_z(&self) -> f32 {
        self.size_z
    }

    /// The tiles a reveal at world `xz` with `range` touches: the game
    /// fills a disc of `range` across around the point, z flipped against
    /// the world's size. `None` when the disc misses the fog or a value is
    /// not finite.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::fog_image::FogShape;
    ///
    /// let shape = FogShape::new(40, 40, 32, 10_240.0).unwrap();
    /// let rect = shape.revealed([300.0, 10_000.0], 200.0).unwrap();
    /// assert_eq!((rect.x, rect.y), (0..2, 0..2));
    /// ```
    #[must_use]
    pub fn revealed(&self, xz: [f32; 2], range: f32) -> Option<TileRect> {
        let per_metre = 1.0 / self.metres_per_pixel();
        let centre = [
            f64::from(xz[0]) * per_metre,
            f64::from(self.size_z - xz[1]) * per_metre,
        ];
        // One pixel more than the disc, for the game's rounding.
        let radius = f64::from(range) * 0.5 * per_metre + 1.0;
        if centre.iter().any(|value| !value.is_finite()) || !radius.is_finite() {
            return None;
        }
        let x = self.span(centre[0], radius, self.tiles_wide)?;
        let y = self.span(centre[1], radius, self.tiles_high)?;
        Some(TileRect { x, y })
    }

    /// The tiles from `centre - radius` to `centre + radius` pixels along
    /// one axis of `tiles`, clamped.
    fn span(&self, centre: f64, radius: f64, tiles: u32) -> Option<Range<u32>> {
        let edge = f64::from(self.edge);
        let low = ((centre - radius) / edge).floor().max(0.0);
        let high = (((centre + radius) / edge).floor() + 1.0).min(f64::from(tiles));
        if low >= high {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "both are clamped to 0..=tiles, at most MAX_TILES, before the cast"
        )]
        let range = low as u32..high as u32;
        Some(range)
    }
}

/// Why a read of the image is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The box leaves the image.
    Outside,
    /// The buffer is smaller than the box.
    Short,
}

/// The loader's copy of one fog: its shape, its pixels and how often it
/// changed.
#[derive(Debug)]
pub struct FogImage {
    shape: FogShape,
    pixels: Vec<u8>,
    version: u64,
}

impl FogImage {
    /// A fog of `shape`, every pixel hidden, at version 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::fog_image::{FogImage, FogShape};
    ///
    /// let shape = FogShape::new(2, 1, 4, 4.0).unwrap();
    /// let mut image = FogImage::new(shape);
    /// assert!(image.put_tile(1, 0, &[255; 16]));
    /// assert!(!image.put_tile(1, 0, &[255; 16]));
    /// let mut out = [0u8; 2];
    /// image.read([3, 0], [2, 1], &mut out).unwrap();
    /// assert_eq!(out, [0, 255]);
    /// assert_eq!(image.version(), 1);
    /// ```
    #[must_use]
    pub fn new(shape: FogShape) -> Self {
        let [width, height] = shape.size();
        Self {
            shape,
            pixels: vec![0; width as usize * height as usize],
            version: 0,
        }
    }

    /// Its shape.
    #[must_use]
    pub const fn shape(&self) -> FogShape {
        self.shape
    }

    /// How many puts changed a pixel since it was made.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Writes tile (`tx`, `ty`) from `tile`, its pixels row by row, x
    /// fastest, as the game keeps them, and whether a pixel changed, which
    /// moves the version. A tile outside the fog or of the wrong length is
    /// skipped.
    pub fn put_tile(&mut self, tx: u32, ty: u32, tile: &[u8]) -> bool {
        let edge = self.shape.edge as usize;
        let [tiles_wide, tiles_high] = self.shape.tiles();
        if tx >= tiles_wide || ty >= tiles_high || tile.len() != edge * edge {
            return false;
        }
        let width = self.shape.size()[0] as usize;
        let left = tx as usize * edge;
        let mut changed = false;
        for (row, line) in tile.chunks_exact(edge).enumerate() {
            let start = (ty as usize * edge + row) * width + left;
            let kept = &mut self.pixels[start..start + edge];
            if kept != line {
                kept.copy_from_slice(line);
                changed = true;
            }
        }
        if changed {
            self.version += 1;
        }
        changed
    }

    /// Copies the box of `size` pixels at `min` into `out`, row by row.
    ///
    /// # Errors
    ///
    /// [`ReadError::Outside`] for a box that leaves the image,
    /// [`ReadError::Short`] when `out` cannot hold it.
    pub fn read(&self, min: [u32; 2], size: [u32; 2], out: &mut [u8]) -> Result<(), ReadError> {
        let [width, height] = self.shape.size();
        let inside = |axis: usize, limit: u32| {
            min[axis]
                .checked_add(size[axis])
                .is_some_and(|end| end <= limit)
        };
        if !inside(0, width) || !inside(1, height) {
            return Err(ReadError::Outside);
        }
        let (box_width, rows) = (size[0] as usize, size[1] as usize);
        if out.len() < box_width * rows {
            return Err(ReadError::Short);
        }
        for row in 0..rows {
            let start = (min[1] as usize + row) * width as usize + min[0] as usize;
            out[row * box_width..(row + 1) * box_width]
                .copy_from_slice(&self.pixels[start..start + box_width]);
        }
        Ok(())
    }
}

/// What a reveal copies into the loader's image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// A fog the image does not hold yet: a new image, every tile, a new
    /// session.
    Full,
    /// These tiles of the fog the image holds.
    Tiles(TileRect),
    /// Nothing.
    Nothing,
}

/// The copy a reveal of the fog at `fog` of `shape` makes, when the image
/// holds `held` and tiles `skipped` were left out by earlier reveals.
///
/// # Examples
///
/// ```
/// use enshwick_loader::fog_image::{FogShape, Plan, TileRect, plan};
///
/// let shape = FogShape::new(40, 40, 32, 10_240.0).unwrap();
/// assert_eq!(plan(None, 0x1000, shape, None, None), Plan::Full);
/// let rect = TileRect { x: 1..2, y: 3..4 };
/// assert_eq!(plan(Some((0x1000, shape)), 0x1000, shape, Some(rect.clone()), None), Plan::Tiles(rect));
/// ```
#[must_use]
pub fn plan(
    held: Option<(usize, FogShape)>,
    fog: usize,
    shape: FogShape,
    revealed: Option<TileRect>,
    skipped: Option<TileRect>,
) -> Plan {
    if held != Some((fog, shape)) {
        return Plan::Full;
    }
    match (revealed, skipped) {
        (Some(a), Some(b)) => Plan::Tiles(a.union(&b)),
        (Some(rect), None) | (None, Some(rect)) => Plan::Tiles(rect),
        (None, None) => Plan::Nothing,
    }
}

impl TileRect {
    /// The smallest rectangle holding both.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            x: self.x.start.min(other.x.start)..self.x.end.max(other.x.end),
            y: self.y.start.min(other.y.start)..self.y.end.max(other.y.end),
        }
    }

    /// The rectangle in one word, 16 bits per bound, for a lock-free
    /// record of skipped tiles; 0 is no rectangle.
    #[must_use]
    pub fn pack(&self) -> u64 {
        let bound = |value: u32| u64::from(u16::try_from(value).unwrap_or(u16::MAX));
        // Ends are stored plus one, so no packed rectangle is 0.
        bound(self.x.start)
            | bound(self.x.end + 1) << 16
            | bound(self.y.start) << 32
            | bound(self.y.end + 1) << 48
    }

    /// The rectangle [`TileRect::pack`] made; `None` for 0.
    #[must_use]
    pub fn unpack(word: u64) -> Option<Self> {
        let bound = |shift: u32| u32::try_from((word >> shift) & 0xffff).unwrap_or(0);
        (word != 0).then(|| Self {
            x: bound(0)..bound(16).saturating_sub(1),
            y: bound(32)..bound(48).saturating_sub(1),
        })
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn world() -> FogShape {
        FogShape::new(40, 40, 32, 10_240.0).unwrap()
    }

    #[test]
    fn a_shape_the_game_cannot_hold_is_refused() {
        assert!(FogShape::new(40, 40, 32, f32::NAN).is_none());
        assert!(FogShape::new(40, 40, 32, 0.0).is_none());
        assert!(FogShape::new(MAX_TILES + 1, 40, 32, 10_240.0).is_none());
        assert!(FogShape::new(40, 40, 0, 10_240.0).is_none());
    }

    #[test]
    fn a_reveal_flips_z_against_the_world_size() {
        let north = world().revealed([5_000.0, 10_200.0], 16.0).unwrap();
        assert_eq!(north.y, 0..1);
        let south = world().revealed([5_000.0, 30.0], 16.0).unwrap();
        assert_eq!(south.y, 39..40);
        assert_eq!(south.x, 19..20);
    }

    #[test]
    fn a_reveal_past_the_edge_is_clamped_and_one_outside_touches_nothing() {
        let corner = world().revealed([0.0, 10_240.0], 600.0).unwrap();
        assert_eq!((corner.x.start, corner.y.start), (0, 0));
        assert!(world().revealed([-5_000.0, 5_000.0], 100.0).is_none());
        assert!(world().revealed([f32::NAN, 5_000.0], 100.0).is_none());
    }

    #[test]
    fn a_read_outside_the_image_or_into_a_short_buffer_is_refused() {
        let image = FogImage::new(world());
        let mut out = [0u8; 4];
        assert_eq!(
            image.read([1279, 0], [2, 1], &mut out),
            Err(ReadError::Outside)
        );
        assert_eq!(
            image.read([u32::MAX, 0], [2, 1], &mut out),
            Err(ReadError::Outside)
        );
        assert_eq!(image.read([0, 0], [5, 1], &mut out), Err(ReadError::Short));
        assert_eq!(image.read([0, 0], [2, 2], &mut out), Ok(()));
    }

    proptest! {
        #[test]
        fn every_pixel_of_a_reveal_lies_in_its_tiles(
            x in 0.0f32..10_240.0,
            z in 0.0f32..10_240.0,
            range in 0.0f32..2_000.0,
            angle in 0.0f32..core::f32::consts::TAU,
            reach in 0.0f32..1.0,
        ) {
            let shape = world();
            let rect = shape.revealed([x, z], range).unwrap();
            let r = range * 0.5 * reach;
            let pixel = [(x + r * angle.cos()) * 0.125, (10_240.0 - (z + r * angle.sin())) * 0.125];
            prop_assume!(pixel.iter().all(|p| (0.0..1280.0).contains(p)));
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "checked to lie in 0..1280")]
            let tile = pixel.map(|p| p as u32 / 32);
            prop_assert!(rect.x.contains(&tile[0]) && rect.y.contains(&tile[1]));
        }

        #[test]
        fn a_tile_lands_where_a_read_finds_it(tx in 0u32..40, ty in 0u32..40, seed in any::<u8>()) {
            let mut image = FogImage::new(world());
            let tile: Vec<u8> = (0..1024u32).map(|i| seed.wrapping_add(u8::try_from(i % 251).unwrap())).collect();
            image.put_tile(tx, ty, &tile);
            let mut out = vec![0u8; 1024];
            image.read([tx * 32, ty * 32], [32, 32], &mut out).unwrap();
            prop_assert_eq!(out, tile);
        }
    }

    fn rect(x: Range<u32>, y: Range<u32>) -> TileRect {
        TileRect { x, y }
    }

    #[test]
    fn a_fog_the_image_does_not_hold_is_copied_whole() {
        let shape = world();
        let other = FogShape::new(20, 40, 32, 10_240.0).unwrap();
        assert_eq!(
            plan(None, 1, shape, Some(rect(0..1, 0..1)), None),
            Plan::Full
        );
        assert_eq!(plan(Some((1, shape)), 2, shape, None, None), Plan::Full);
        assert_eq!(plan(Some((1, other)), 1, shape, None, None), Plan::Full);
    }

    #[test]
    fn a_held_fog_copies_the_reveal_and_what_earlier_reveals_skipped() {
        let shape = world();
        let held = Some((1, shape));
        assert_eq!(plan(held, 1, shape, None, None), Plan::Nothing);
        assert_eq!(
            plan(
                held,
                1,
                shape,
                Some(rect(2..3, 2..4)),
                Some(rect(0..1, 5..6))
            ),
            Plan::Tiles(rect(0..3, 2..6))
        );
        assert_eq!(
            plan(held, 1, shape, None, Some(rect(0..1, 5..6))),
            Plan::Tiles(rect(0..1, 5..6))
        );
    }

    #[test]
    fn a_tile_that_holds_the_same_pixels_keeps_the_version() {
        let mut image = FogImage::new(FogShape::new(2, 2, 2, 4.0).unwrap());
        assert!(!image.put_tile(0, 0, &[0; 4]));
        assert_eq!(image.version(), 0);
        assert!(image.put_tile(1, 1, &[0, 0, 9, 0]));
        assert!(!image.put_tile(1, 1, &[0, 0, 9, 0]));
        assert_eq!(image.version(), 1);
    }

    proptest! {
        #[test]
        fn a_packed_rectangle_unpacks_as_it_was(x0 in 0u32..256, w in 0u32..256, y0 in 0u32..256, h in 0u32..256) {
            let given = rect(x0..x0 + w, y0..y0 + h);
            prop_assert_ne!(given.pack(), 0);
            prop_assert_eq!(TileRect::unpack(given.pack()), Some(given));
        }
    }
}
