//! The whole 0.5 m cells altar zones are measured in, and the boxes of them.

use crate::abi::Bounds;

/// A world-aligned, half-open box of whole 0.5 m cells; axis 1 is up.
///
/// # Examples
///
/// ```
/// use enshwick::altars::CellBox;
///
/// let zone = CellBox::new([0, 0, 0], [80, 80, 80]);
/// assert!(zone.contains([79, 0, 0]));
/// assert!(!zone.contains([80, 0, 0]));
/// assert_eq!(zone.footprint_area(), 6400);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a value of fixed parts, which never gains one"
)]
pub struct CellBox {
    /// The inclusive lower corner.
    pub min: [i64; 3],
    /// The exclusive upper corner.
    pub max: [i64; 3],
}

impl CellBox {
    /// The box from `min` (inclusive) to `max` (exclusive).
    ///
    /// # Examples
    ///
    /// See [`CellBox`].
    #[must_use]
    pub const fn new(min: [i64; 3], max: [i64; 3]) -> Self {
        Self { min, max }
    }

    /// Whether the box holds no cell.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::CellBox;
    ///
    /// assert!(CellBox::default().is_empty());
    /// assert!(!CellBox::new([0, 0, 0], [1, 1, 1]).is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        (0..3).any(|a| self.min[a] >= self.max[a])
    }

    /// Whether the cell `cell` lies inside.
    ///
    /// # Examples
    ///
    /// See [`CellBox`].
    #[must_use]
    pub fn contains(&self, cell: [i64; 3]) -> bool {
        (0..3).all(|a| self.min[a] <= cell[a] && cell[a] < self.max[a])
    }

    /// Whether the two boxes share a cell boundary or more, edges and corners
    /// included, as zones that the engine groups into one base do.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::CellBox;
    ///
    /// let a = CellBox::new([0, 0, 0], [10, 10, 10]);
    /// assert!(a.touches(&CellBox::new([10, 0, 0], [20, 10, 10])));
    /// assert!(!a.touches(&CellBox::new([11, 0, 0], [20, 10, 10])));
    /// ```
    #[must_use]
    pub fn touches(&self, other: &Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && (0..3).all(|a| self.min[a] <= other.max[a] && other.min[a] <= self.max[a])
    }

    /// The smallest box holding both.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::CellBox;
    ///
    /// let a = CellBox::new([0, 0, 0], [10, 10, 10]);
    /// let b = CellBox::new([10, -5, 0], [20, 10, 10]);
    /// assert_eq!(a.union(&b), CellBox::new([0, -5, 0], [20, 10, 10]));
    /// ```
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: [0, 1, 2].map(|a| self.min[a].min(other.min[a])),
            max: [0, 1, 2].map(|a| self.max[a].max(other.max[a])),
        }
    }

    /// The footprint's area in cells: axes 0 and 2.
    ///
    /// # Examples
    ///
    /// See [`CellBox`].
    #[must_use]
    pub fn footprint_area(&self) -> i64 {
        if self.is_empty() {
            return 0;
        }
        (self.max[0] - self.min[0]) * (self.max[2] - self.min[2])
    }

    /// Whether the two footprints share a cell boundary or more, edges and
    /// corners included, whatever the heights: as zones that grow sideways
    /// meet. A box with no cells, flat ones included, has no footprint.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::CellBox;
    ///
    /// let a = CellBox::new([0, 0, 0], [2, 1, 2]);
    /// assert!(a.footprint_touches(&CellBox::new([2, 50, 2], [3, 51, 3])));
    /// assert!(!a.footprint_touches(&CellBox::new([3, 0, 0], [4, 1, 1])));
    /// ```
    #[must_use]
    pub fn footprint_touches(&self, other: &Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && [0, 2]
                .iter()
                .all(|&a| self.min[a] <= other.max[a] && other.min[a] <= self.max[a])
    }

    /// The box grown by `k` cells on every horizontal side, its height kept;
    /// a negative `k` shrinks it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::CellBox;
    ///
    /// let grown = CellBox::new([0, 0, 0], [2, 1, 2]).grown(1);
    /// assert_eq!(grown, CellBox::new([-1, 0, -1], [3, 1, 3]));
    /// ```
    #[must_use]
    pub const fn grown(&self, k: i64) -> Self {
        let mut grown = *self;
        grown.min[0] -= k;
        grown.max[0] += k;
        grown.min[2] -= k;
        grown.max[2] += k;
        grown
    }

    /// The area in cells that the footprints of `boxes` cover together,
    /// overlaps counted once, at most `i64::MAX`; a box with no cells, flat
    /// ones included, covers none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::CellBox;
    ///
    /// let a = CellBox::new([0, 0, 0], [2, 1, 2]);
    /// let b = CellBox::new([1, 0, 1], [3, 1, 3]);
    /// assert_eq!(CellBox::union_footprint_area(&[a, b]), 7);
    /// ```
    #[must_use]
    pub fn union_footprint_area(boxes: &[Self]) -> i64 {
        let boxes: Vec<&Self> = boxes.iter().filter(|b| !b.is_empty()).collect();
        let mut xs: Vec<i64> = boxes.iter().flat_map(|b| [b.min[0], b.max[0]]).collect();
        xs.sort_unstable();
        xs.dedup();

        let mut area: i128 = 0;
        for strip in xs.windows(2) {
            let (x0, x1) = (strip[0], strip[1]);
            let mut spans: Vec<(i64, i64)> = boxes
                .iter()
                .filter(|b| b.min[0] <= x0 && x1 <= b.max[0])
                .map(|b| (b.min[2], b.max[2]))
                .collect();
            spans.sort_unstable();

            let mut covered = 0;
            let mut reached = i64::MIN;
            for (z0, z1) in spans {
                let from = z0.max(reached);
                if z1 > from {
                    covered += i128::from(z1) - i128::from(from);
                }
                reached = reached.max(z1);
            }
            area += covered * (i128::from(x1) - i128::from(x0));
        }
        i64::try_from(area).unwrap_or(i64::MAX)
    }

    pub(super) const fn from_bounds(bounds: Bounds) -> Self {
        Self {
            min: bounds.min,
            max: bounds.max,
        }
    }
}

/// The cell that holds a point given in the engine's 32.32 fixed-point
/// metres, as `CurrentTransform` stores positions.
///
/// # Examples
///
/// ```
/// use enshwick::altars::cell_of_fixed;
///
/// assert_eq!(cell_of_fixed([3 << 32, 1 << 31, -1]), [6, 1, -1]);
/// ```
#[must_use]
pub const fn cell_of_fixed(fixed: [i64; 3]) -> [i64; 3] {
    [fixed[0] >> 31, fixed[1] >> 31, fixed[2] >> 31]
}

/// The cell that holds a point given in metres.
///
/// # Examples
///
/// ```
/// use enshwick::altars::cell_of_metres;
///
/// assert_eq!(cell_of_metres([3.0, 0.75, -0.25]), [6, 1, -1]);
/// ```
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "world coordinates fit in i64 cells"
)]
pub fn cell_of_metres(metres: [f64; 3]) -> [i64; 3] {
    metres.map(|m| (m * 2.0).floor() as i64)
}
