//! Voxels a mod writes over a box: their words and how they lie over each brick.

use enshwick::abi::{Status, Voxel};

use super::cover::{BRICK, BRICK_VOXELS, Brick, Bricks};

/// A box of voxels, not empty and within [`Voxel::END`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Region {
    min: [u32; 3],
    size: [u32; 3],
}

impl Region {
    /// The box of `size` from `min`; [`Status::BAD_ARGS`] for an empty box
    /// or one past [`Voxel::END`].
    pub(crate) fn new(min: [u32; 3], size: [u32; 3]) -> Result<Self, Status> {
        let fits = (0..3).all(|axis| {
            size[axis] > 0
                && min[axis]
                    .checked_add(size[axis])
                    .is_some_and(|end| end <= Voxel::END)
        });
        fits.then_some(Self { min, size }).ok_or(Status::BAD_ARGS)
    }

    /// Its voxels; at most 2⁹⁰, so it fits a `u128`.
    fn count(self) -> u128 {
        self.size.iter().map(|&axis| u128::from(axis)).product()
    }

    /// Its voxels, as an index; saturating past `usize`.
    #[cfg(test)]
    pub(crate) fn len(self) -> usize {
        usize::try_from(self.count()).unwrap_or(usize::MAX)
    }

    /// Its lowest and highest corner in metres, half a metre a voxel.
    pub(crate) fn metres(self) -> ([f64; 3], [f64; 3]) {
        let min = self.min.map(|axis| f64::from(axis) / 2.0);
        let max = [0, 1, 2].map(|axis| f64::from(self.min[axis] + self.size[axis]) / 2.0);
        (min, max)
    }

    /// The bricks it touches.
    pub(crate) const fn bricks(self) -> Bricks {
        Bricks::of_box(self.min, self.size)
    }

    /// Its lowest voxel and its voxels along each axis.
    #[cfg(test)]
    pub(crate) const fn bounds(self) -> ([u32; 3], [u32; 3]) {
        (self.min, self.size)
    }

    /// Calls `row` for each row along x of the part inside `brick`, with
    /// the row's first index in the box (x fastest, then y, then z), its
    /// first index in the brick and its length.
    fn rows(self, brick: Brick, mut row: impl FnMut(usize, usize, usize)) {
        let corner = brick.min();
        let mut low = [0; 3];
        let mut high = [0; 3];
        for axis in 0..3 {
            low[axis] = self.min[axis].max(corner[axis]);
            high[axis] = (self.min[axis] + self.size[axis]).min(corner[axis] + BRICK);
            if low[axis] >= high[axis] {
                return;
            }
        }
        let [width, height, _] = self.size.map(|axis| axis as usize);
        let run = (high[0] - low[0]) as usize;
        for z in low[2]..high[2] {
            for y in low[1]..high[1] {
                let from = (low[0] - self.min[0]) as usize
                    + width * ((y - self.min[1]) as usize + height * (z - self.min[2]) as usize);
                let to = (low[0] - corner[0]
                    + BRICK * (y - corner[1])
                    + BRICK * BRICK * (z - corner[2])) as usize;
                row(from, to, run);
            }
        }
    }
}

/// The game's voxel words for a box, x fastest, then y, then z.
#[derive(Clone, Debug)]
pub(crate) struct Paste {
    region: Region,
    words: Box<[u16]>,
}

impl Paste {
    /// [`Status::BAD_ARGS`] for an empty box, one past [`Voxel::END`] or
    /// `words` that do not fill it.
    pub(crate) fn new(min: [u32; 3], size: [u32; 3], words: Vec<u16>) -> Result<Self, Status> {
        let region = Region::new(min, size)?;
        if region.count() != words.len() as u128 {
            return Err(Status::BAD_ARGS);
        }
        Ok(Self {
            region,
            words: words.into_boxed_slice(),
        })
    }

    /// The bricks it writes into.
    pub(crate) const fn bricks(&self) -> Bricks {
        self.region.bricks()
    }

    /// Its box.
    pub(crate) const fn region(&self) -> Region {
        self.region
    }

    /// Writes the part of the paste inside `brick` over `image`, the
    /// brick's voxels x fastest, then y, then z.
    pub(crate) fn overlay(&self, brick: Brick, image: &mut [u16; BRICK_VOXELS]) {
        self.region.rows(brick, |from, to, run| {
            image[to..to + run].copy_from_slice(&self.words[from..from + run]);
        });
    }
}

/// The building voxels of a box, to be cleared to air; its ground stays.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Erase {
    region: Region,
}

impl Erase {
    /// [`Status::BAD_ARGS`] for an empty box or one past [`Voxel::END`].
    pub(crate) fn new(min: [u32; 3], size: [u32; 3]) -> Result<Self, Status> {
        Region::new(min, size).map(|region| Self { region })
    }

    /// The bricks it writes into.
    pub(crate) const fn bricks(&self) -> Bricks {
        self.region.bricks()
    }

    /// Its box.
    pub(crate) const fn region(&self) -> Region {
        self.region
    }

    /// Clears the building voxels of the part of the box inside `brick`;
    /// how many it cleared.
    pub(crate) fn overlay(&self, brick: Brick, image: &mut [u16; BRICK_VOXELS]) -> usize {
        let mut cleared = 0;
        self.region.rows(brick, |_, to, run| {
            for word in &mut image[to..to + run] {
                if crate::voxels::decode(*word).kind == Voxel::BUILDING {
                    *word = 0;
                    cleared += 1;
                }
            }
        });
        cleared
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transactions::cover::{BRICK_VOXELS, Brick};

    /// A paste of `size` from `min` whose word at each place is its index + 1.
    fn counting(min: [u32; 3], size: [u32; 3]) -> Paste {
        let count = size.iter().product::<u32>() as usize;
        let words = (1..=count).map(|i| u16::try_from(i).unwrap()).collect();
        Paste::new(min, size, words).unwrap()
    }

    #[test]
    fn a_paste_covers_the_bricks_its_box_touches() {
        let paste = counting([6, 0, 15], [4, 1, 2]);
        let bricks: Vec<[u32; 3]> = paste.bricks().iter().map(|b| b.0).collect();
        assert_eq!(bricks, [[0, 0, 1], [1, 0, 1], [0, 0, 2], [1, 0, 2]]);
    }

    #[test]
    fn an_overlay_writes_only_the_part_inside_the_brick() {
        let paste = counting([6, 0, 7], [4, 1, 2]);
        let mut image = [0u16; BRICK_VOXELS];
        paste.overlay(Brick([0, 0, 0]), &mut image);
        // Inside brick 0: x 6..8, y 0, z 7; the paste's x 0..2 of row z 0.
        assert_eq!(image[6 + 64 * 7], 1);
        assert_eq!(image[7 + 64 * 7], 2);
        assert_eq!(image.iter().filter(|&&w| w != 0).count(), 2);

        let mut next = [0u16; BRICK_VOXELS];
        paste.overlay(Brick([1, 0, 0]), &mut next);
        // Brick 1 along x: x 8..10 at local 0..2, z 7; paste x 2..4.
        assert_eq!(next[64 * 7], 3);
        assert_eq!(next[1 + 64 * 7], 4);
        assert_eq!(next.iter().filter(|&&w| w != 0).count(), 2);
    }

    #[test]
    fn every_voxel_of_a_paste_lands_once_over_its_bricks() {
        let paste = counting([3, 5, 9], [11, 6, 9]);
        let mut seen = 0;
        for brick in paste.bricks().iter() {
            let mut image = [0u16; BRICK_VOXELS];
            paste.overlay(brick, &mut image);
            seen += image.iter().filter(|&&w| w != 0).count();
        }
        assert_eq!(seen, 11 * 6 * 9);
    }

    #[test]
    fn an_erase_clears_only_building_voxels_inside_its_box() {
        let erase = Erase::new([2, 0, 0], [3, 1, 1]).unwrap();
        let mut image = [0u16; BRICK_VOXELS];
        image[1] = 0xff83;
        image[2] = 0xff83;
        image[3] = 0x4009;
        image[4] = 0x0080;
        image[5] = 0xff90;
        erase.overlay(Brick([0, 0, 0]), &mut image);
        assert_eq!(&image[1..6], &[0xff83, 0, 0x4009, 0, 0xff90]);
        assert_eq!(erase.bricks().count(), 1);
    }

    #[test]
    fn a_paste_whose_words_do_not_fill_its_box_is_refused() {
        assert_eq!(
            Paste::new([0; 3], [2, 2, 2], vec![0; 7]).err(),
            Some(Status::BAD_ARGS)
        );
        assert_eq!(
            Paste::new([0; 3], [0, 2, 2], Vec::new()).err(),
            Some(Status::BAD_ARGS)
        );
        assert_eq!(
            Paste::new([Voxel::END, 0, 0], [1; 3], vec![0]).err(),
            Some(Status::BAD_ARGS)
        );
    }
}
