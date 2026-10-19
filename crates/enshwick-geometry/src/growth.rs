//! The growth of one cluster.
//!
//! A cluster's growth `k` is the least one with which no group of its
//! altars, each at any level up to its current one, is short of its budget.
//! A group is short at `k` unless `2 B < A(k) + A(k + 1)`, where `B` is the
//! sum of its footprints and `A(k)` the area of their union grown by `k`:
//! that is the group's own growth rounded to the nearest step, with half a
//! step rounding up. Every earlier state of the cluster is such a group, so
//! adding an altar or raising a level never lowers `k`, and `k` is the least
//! growth with that property that never gives a group less than its budget.
//!
//! Finding the shortest group is a closure problem. Every altar level is an
//! element worth twice the area it adds over the level below; choosing it
//! costs the cells its zone covers at `k` and at `k + 1`. Union area is a
//! coverage function, so the best group is the maximum-weight closure, found
//! with one minimum cut.

use std::collections::BTreeMap;

use crate::Box3;
use crate::flow::{INFINITE, Network};

/// One altar level within a cluster.
struct Element {
    zone: Box3,
    /// Twice the area this level adds over the level below.
    weight: i128,
    /// The altar's first level: every nonempty group contains one.
    first: bool,
}

/// The least growth for a cluster, given each member's zones from level 1
/// up to its current level.
pub(crate) fn growth(chains: &[&[Box3]]) -> i64 {
    let mut elements = Vec::new();
    for chain in chains {
        let mut below = 0;
        for zone in chain.iter().filter(|z| z.footprint_area() > 0) {
            let area = zone.footprint_area();
            elements.push(Element {
                zone: *zone,
                weight: 2 * (area - below),
                first: below == 0,
            });
            below = area;
        }
    }

    if fits(&elements, 0) {
        return 0;
    }
    // Shortness only falls as k grows, so search for the first k that fits.
    let mut short = 0;
    let mut enough = 1;
    while !fits(&elements, enough) {
        short = enough;
        enough *= 2;
    }
    while enough - short > 1 {
        let middle = short + (enough - short) / 2;
        if fits(&elements, middle) {
            enough = middle;
        } else {
            short = middle;
        }
    }
    enough
}

/// Whether no nonempty group is short at `k`.
fn fits(elements: &[Element], k: i64) -> bool {
    let mut cells = BTreeMap::new();
    cover(elements, k, &mut cells);
    cover(elements, k + 1, &mut cells);
    let cells: Vec<(Vec<usize>, i128)> = cells.into_iter().collect();

    (0..elements.len())
        .filter(|&e| elements[e].first)
        .all(|forced| best_group(elements, &cells, forced) < 0)
}

/// Adds the area of the cells covered by the elements' zones grown by `k`,
/// keyed by the set of elements covering them.
fn cover(elements: &[Element], k: i64, cells: &mut BTreeMap<Vec<usize>, i128>) {
    let zones: Vec<Box3> = elements.iter().map(|e| e.zone.grown(k)).collect();
    let mut xs: Vec<i64> = zones.iter().flat_map(|z| [z.min[0], z.max[0]]).collect();
    let mut zs: Vec<i64> = zones.iter().flat_map(|z| [z.min[2], z.max[2]]).collect();
    for edges in [&mut xs, &mut zs] {
        edges.sort_unstable();
        edges.dedup();
    }
    for x in xs.windows(2) {
        let across: Vec<usize> = (0..zones.len())
            .filter(|&e| zones[e].min[0] <= x[0] && x[1] <= zones[e].max[0])
            .collect();
        if across.is_empty() {
            continue;
        }
        for z in zs.windows(2) {
            let covering: Vec<usize> = across
                .iter()
                .copied()
                .filter(|&e| zones[e].min[2] <= z[0] && z[1] <= zones[e].max[2])
                .collect();
            if !covering.is_empty() {
                *cells.entry(covering).or_insert(0) +=
                    i128::from(x[1] - x[0]) * i128::from(z[1] - z[0]);
            }
        }
    }
}

/// The largest `2 B - A(k) - A(k + 1)` over the groups that contain
/// `forced`, as a maximum-weight closure.
///
/// Levels need no edge to the level below: a lower level covers no cell its
/// upper level does not, and its weight is never negative, so a best group
/// with a level always has the levels below it as well.
fn best_group(elements: &[Element], cells: &[(Vec<usize>, i128)], forced: usize) -> i128 {
    let (source, sink) = (0, 1);
    let element_node = |e: usize| 2 + e;
    let cell_node = |c: usize| 2 + elements.len() + c;
    let mut network = Network::new(2 + elements.len() + cells.len());

    let total: i128 = elements.iter().map(|e| e.weight).sum();
    for (e, element) in elements.iter().enumerate() {
        // The forced element can never be cut away from the source.
        let weight = if e == forced {
            INFINITE
        } else {
            element.weight
        };
        network.add_edge(source, element_node(e), weight);
    }
    for (c, (covering, area)) in cells.iter().enumerate() {
        for &e in covering {
            network.add_edge(element_node(e), cell_node(c), INFINITE);
        }
        network.add_edge(cell_node(c), sink, *area);
    }
    // With the forced weight raised to infinity the closure keeps it, and
    // the true value is the sum of the real weights minus the cut.
    total - network.max_flow(source, sink)
}
