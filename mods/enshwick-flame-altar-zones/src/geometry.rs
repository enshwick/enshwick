//! Flame altar zones as integer boxes: clusters and their uniform growth.
//!
//! Altars whose vanilla zones touch or overlap form a cluster. A cluster's
//! area budget is the sum of its members' vanilla footprints, so overlap is
//! never lost. Every member keeps one world-aligned box: all members of a
//! cluster grow by the same number of cells `k` on every horizontal side,
//! which is the same as growing the union of their boxes by `k`. The height
//! of every member becomes the tallest member's. Clusters whose grown
//! footprints touch merge and are recomputed until nothing changes.
//!
//! `k` is the least growth with which no group of the cluster's altars, each
//! at any level up to its current one, is short of its budget (see
//! `growth`). Placing an altar or raising a level therefore never shrinks a
//! zone, and no cluster ever gets less than its budget, to the nearest step.
//!
//! Everything is integer, in voxel-grid cells. Boxes are half-open
//! (`min <= p < max`), like the engine's point test. Axis 1 is up; the
//! footprint is axes 0 and 2. The result depends only on the set of altars,
//! never on their order.
//!
//! # Examples
//!
//! ```
//! use enshwick_flame_altar_zones::geometry::{Altar, Box3, clusters};
//!
//! // Two altars whose zones touch grow together.
//! let a = Altar::new(1, Box3::new([0, 0, 0], [10, 10, 10]));
//! let b = Altar::new(2, Box3::new([10, 0, 0], [20, 10, 10]));
//! let placements = clusters(&[a, b])?;
//! assert_eq!(placements[0].cluster, placements[1].cluster);
//! # Ok::<(), enshwick_flame_altar_zones::geometry::ClusterError>(())
//! ```

mod flow;
mod growth;

/// A world-aligned, half-open box in voxel-grid cells; axis 1 is up.
///
/// # Examples
///
/// ```
/// use enshwick_flame_altar_zones::geometry::Box3;
///
/// let zone = Box3::new([0, 0, 0], [4, 2, 3]);
/// assert_eq!(zone.footprint_area(), 12);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Box3 {
    /// The inclusive lower corner.
    pub min: [i64; 3],
    /// The exclusive upper corner.
    pub max: [i64; 3],
}

/// An altar and its vanilla zones, the input of [`clusters`].
///
/// # Examples
///
/// ```
/// use enshwick_flame_altar_zones::geometry::{Altar, Box3};
///
/// let altar = Altar::new(7, Box3::new([0, 0, 0], [10, 10, 10]));
/// assert_eq!(altar.zone(), Box3::new([0, 0, 0], [10, 10, 10]));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Altar {
    /// Stable identity, unique within the input. The smallest key in a
    /// cluster names the cluster.
    pub key: u64,
    /// Its vanilla zone at every level from the first to the current one,
    /// each inside the next. A zone with an empty footprint, such as level
    /// 0's, takes part in nothing.
    pub levels: Vec<Box3>,
}

impl Altar {
    /// An altar whose zone has only ever been `zone`.
    ///
    /// # Examples
    ///
    /// See [`Altar`].
    #[must_use]
    pub fn new(key: u64, zone: Box3) -> Self {
        Self {
            key,
            levels: vec![zone],
        }
    }

    /// The vanilla zone at the current level; an empty box for an altar with no
    /// level, which [`clusters`] refuses.
    ///
    /// # Examples
    ///
    /// See [`Altar`].
    #[must_use]
    pub fn zone(&self) -> Box3 {
        self.levels
            .last()
            .copied()
            .unwrap_or(Box3::new([0; 3], [0; 3]))
    }
}

/// An altar's zone after clustering, the output of [`clusters`].
///
/// # Examples
///
/// See the [crate documentation](crate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The altar's key.
    pub key: u64,
    /// The smallest key in the altar's cluster.
    pub cluster: u64,
    /// Cells every member of the cluster grew by on each horizontal side.
    pub growth: i64,
    /// The altar's zone.
    pub zone: Box3,
}

/// Why [`clusters`] refused its input.
///
/// # Examples
///
/// ```
/// use enshwick_flame_altar_zones::geometry::{Altar, Box3, ClusterError, clusters};
///
/// let zone = Box3::new([0, 0, 0], [1, 1, 1]);
/// let error = clusters(&[Altar::new(1, zone), Altar::new(1, zone)]).unwrap_err();
/// assert_eq!(error, ClusterError::DuplicateKey(1));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ClusterError {
    /// Two altars share a key.
    #[error("altar key {0} appears twice")]
    DuplicateKey(u64),
    /// `min > max` on some axis.
    #[error("altar {0} has an inverted box")]
    InvertedBox(u64),
    /// No zone at all.
    #[error("altar {0} has no level")]
    NoLevels(u64),
    /// A level's zone does not lie inside the next level's.
    #[error("altar {0} has levels that are not nested")]
    NotNested(u64),
}

impl Box3 {
    /// A box from its corners.
    ///
    /// # Examples
    ///
    /// See [`Box3`].
    #[must_use]
    pub const fn new(min: [i64; 3], max: [i64; 3]) -> Self {
        Self { min, max }
    }

    /// The footprint area, over axes 0 and 2.
    ///
    /// # Examples
    ///
    /// See [`Box3`].
    #[must_use]
    pub fn footprint_area(&self) -> i128 {
        let x = i128::from((self.max[0] - self.min[0]).max(0));
        let z = i128::from((self.max[2] - self.min[2]).max(0));
        x * z
    }

    /// Whether the footprints touch or overlap; a shared edge or corner counts.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_flame_altar_zones::geometry::Box3;
    ///
    /// let a = Box3::new([0, 0, 0], [2, 1, 2]);
    /// assert!(a.footprint_touches(&Box3::new([2, 0, 2], [3, 1, 3])));
    /// assert!(!a.footprint_touches(&Box3::new([3, 0, 0], [4, 1, 1])));
    /// ```
    #[must_use]
    pub fn footprint_touches(&self, other: &Self) -> bool {
        [0, 2]
            .iter()
            .all(|&axis| self.min[axis] <= other.max[axis] && other.min[axis] <= self.max[axis])
    }

    /// The box grown by `k` cells on every horizontal side.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_flame_altar_zones::geometry::Box3;
    ///
    /// let grown = Box3::new([0, 0, 0], [2, 1, 2]).grown(1);
    /// assert_eq!(grown, Box3::new([-1, 0, -1], [3, 1, 3]));
    /// ```
    #[must_use]
    pub fn grown(&self, k: i64) -> Self {
        let mut grown = *self;
        for axis in [0, 2] {
            grown.min[axis] -= k;
            grown.max[axis] += k;
        }
        grown
    }

    /// Whether `point` lies inside: `min <= point < max` on every axis.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_flame_altar_zones::geometry::Box3;
    ///
    /// let zone = Box3::new([0, 0, 0], [2, 2, 2]);
    /// assert!(zone.contains_point([1, 1, 1]));
    /// assert!(!zone.contains_point([2, 1, 1]));
    /// ```
    #[must_use]
    pub fn contains_point(&self, point: [i64; 3]) -> bool {
        (0..3).all(|axis| self.min[axis] <= point[axis] && point[axis] < self.max[axis])
    }

    fn contains(&self, inner: &Self) -> bool {
        (0..3).all(|axis| self.min[axis] <= inner.min[axis] && inner.max[axis] <= self.max[axis])
    }

    fn height(&self) -> i64 {
        self.max[1] - self.min[1]
    }

    /// The box made `height` tall around its own centre. An odd difference
    /// puts the extra cell on top.
    fn with_height(&self, height: i64) -> Self {
        let extra = height - self.height();
        let mut tall = *self;
        tall.min[1] -= extra / 2;
        tall.max[1] += extra - extra / 2;
        tall
    }
}

/// The area of the union of the boxes' footprints.
///
/// # Examples
///
/// ```
/// use enshwick_flame_altar_zones::geometry::{Box3, union_footprint_area};
///
/// let a = Box3::new([0, 0, 0], [2, 1, 2]);
/// let b = Box3::new([1, 0, 1], [3, 1, 3]);
/// assert_eq!(union_footprint_area(&[a, b]), 7);
/// ```
#[must_use]
pub fn union_footprint_area(boxes: &[Box3]) -> i128 {
    let boxes: Vec<&Box3> = boxes.iter().filter(|b| b.footprint_area() > 0).collect();
    let mut xs: Vec<i64> = boxes.iter().flat_map(|b| [b.min[0], b.max[0]]).collect();
    xs.sort_unstable();
    xs.dedup();

    let mut area = 0;
    for strip in xs.windows(2) {
        let (x0, x1) = (strip[0], strip[1]);
        let mut spans: Vec<(i64, i64)> = boxes
            .iter()
            .filter(|b| b.min[0] <= x0 && x1 <= b.max[0])
            .map(|b| (b.min[2], b.max[2]))
            .collect();
        spans.sort_unstable();

        let mut covered: i128 = 0;
        let mut reached = i64::MIN;
        for (z0, z1) in spans {
            let from = z0.max(reached);
            if z1 > from {
                covered += i128::from(z1 - from);
            }
            reached = reached.max(z1);
        }
        area += covered * i128::from(x1 - x0);
    }
    area
}

/// Computes every altar's zone; the result is sorted by key.
///
/// # Errors
///
/// [`ClusterError`] when keys repeat or an altar's levels are malformed.
///
/// # Examples
///
/// See the [crate documentation](crate).
pub fn clusters(altars: &[Altar]) -> Result<Vec<Placement>, ClusterError> {
    let mut altars = altars.to_vec();
    altars.sort_by_key(|a| a.key);
    if let Some(pair) = altars.windows(2).find(|pair| pair[0].key == pair[1].key) {
        return Err(ClusterError::DuplicateKey(pair[0].key));
    }
    for altar in &altars {
        if altar.levels.is_empty() {
            return Err(ClusterError::NoLevels(altar.key));
        }
        if altar
            .levels
            .iter()
            .any(|z| (0..3).any(|axis| z.min[axis] > z.max[axis]))
        {
            return Err(ClusterError::InvertedBox(altar.key));
        }
        if altar
            .levels
            .windows(2)
            .any(|pair| !pair[1].contains(&pair[0]))
        {
            return Err(ClusterError::NotNested(altar.key));
        }
    }

    // Altars with an empty footprint (level 0) take part in nothing.
    let active: Vec<usize> = (0..altars.len())
        .filter(|&i| altars[i].zone().footprint_area() > 0)
        .collect();
    let mut groups: Vec<Vec<usize>> = components(active.len(), |a, b| {
        altars[active[a]]
            .zone()
            .footprint_touches(&altars[active[b]].zone())
    })
    .into_iter()
    .map(|group| group.into_iter().map(|i| active[i]).collect())
    .collect();

    // Each round either merges groups or stops, and the number of groups
    // only decreases, so this terminates.
    let grown = loop {
        let grown: Vec<(i64, Vec<Box3>)> =
            groups.iter().map(|group| grow(&altars, group)).collect();
        let merged = components(groups.len(), |a, b| {
            grown[a]
                .1
                .iter()
                .any(|x| grown[b].1.iter().any(|y| x.footprint_touches(y)))
        });
        if merged.len() == groups.len() {
            break grown;
        }
        groups = merged
            .into_iter()
            .map(|parts| {
                let mut members: Vec<usize> = parts
                    .into_iter()
                    .flat_map(|g| groups[g].iter().copied())
                    .collect();
                members.sort_unstable();
                members
            })
            .collect();
    };

    let mut placements: Vec<Placement> = altars
        .iter()
        .map(|a| Placement {
            key: a.key,
            cluster: a.key,
            growth: 0,
            zone: a.zone(),
        })
        .collect();
    for (group, (growth, boxes)) in groups.iter().zip(grown) {
        // Members are sorted, so the first one has the smallest key.
        let cluster = altars[group[0]].key;
        for (&member, zone) in group.iter().zip(boxes) {
            placements[member] = Placement {
                key: altars[member].key,
                cluster,
                growth,
                zone,
            };
        }
    }
    Ok(placements)
}

/// The growth and the zones of one cluster, in the order of `members`.
fn grow(altars: &[Altar], members: &[usize]) -> (i64, Vec<Box3>) {
    let chains: Vec<&[Box3]> = members
        .iter()
        .map(|&i| altars[i].levels.as_slice())
        .collect();
    let growth = growth::growth(&chains);

    let current: Vec<Box3> = members.iter().map(|&i| altars[i].zone()).collect();
    let height = current.iter().map(Box3::height).max().unwrap_or(0);
    let zones = current
        .iter()
        .map(|b| b.grown(growth).with_height(height))
        .collect();
    (growth, zones)
}

/// Connected components of `0..n` under `touches`. Each component is sorted,
/// and components are ordered by their smallest member.
fn components(n: usize, touches: impl Fn(usize, usize) -> bool) -> Vec<Vec<usize>> {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..n).collect();
    for a in 0..n {
        for b in a + 1..n {
            if touches(a, b) {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut slot = vec![usize::MAX; n];
    for i in 0..n {
        let r = root(&mut parent, i);
        if slot[r] == usize::MAX {
            slot[r] = groups.len();
            groups.push(Vec::new());
        }
        groups[slot[r]].push(i);
    }
    groups
}
