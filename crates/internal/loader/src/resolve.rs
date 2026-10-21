//! Which mods load, and in what order.
//!
//! Pure logic over the manifests, so it is tested without the game. A mod that
//! cannot load is skipped with a reason; the others load regardless.

use std::collections::BTreeMap;

use enshwick_loader_api::abi::{Relation, Side};
use semver::{Version, VersionReq};

/// A mod's manifest, parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The mod id.
    pub id: String,
    /// Its version.
    pub version: Version,
    /// The sides it runs on.
    pub side: Side,
    /// Its relations to other mods.
    pub dependencies: Vec<Dependency>,
}

/// A relation to another mod.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    /// The other mod's id.
    pub id: String,
    /// The versions that satisfy it; any version when `None`.
    pub range: Option<VersionReq>,
    /// How the two relate.
    pub relation: Relation,
}

/// The outcome: candidates by index, in load order, and the skipped ones with
/// the reason.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Indices into the candidates, dependencies first.
    pub order: Vec<usize>,
    /// Indices of skipped candidates and why.
    pub skipped: Vec<(usize, String)>,
    /// Skipped candidates that a present mod replaces: the replaced index
    /// and the replacing one.
    pub replaced: Vec<(usize, usize)>,
}

/// Why a mod made for the other side of the game is skipped, which is
/// expected rather than a fault: `enshwick.zip` carries client mods to
/// servers too.
pub const OTHER_SIDE: &str = "not for this side of the game";

/// Whether a mod for `mod_side` loads in the game binary `process`. The
/// dedicated server loads server mods; the client loads every mod, since it
/// runs the server's code while it hosts a game, solo play included.
#[must_use]
pub fn loads(mod_side: Side, process: Side) -> bool {
    mod_side.includes(process) || (process == Side::CLIENT && mod_side.includes(Side::SERVER))
}

/// Decides which `candidates` load on `side` and in what order.
#[must_use]
pub fn resolve(candidates: &[Candidate], side: Side) -> Plan {
    let mut skipped: BTreeMap<usize, String> = BTreeMap::new();
    let mut by_id: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, candidate) in candidates.iter().enumerate() {
        if !loads(candidate.side, side) {
            skipped.insert(i, OTHER_SIDE.into());
        } else if let Some(&first) = by_id.get(candidate.id.as_str()) {
            skipped.insert(
                i,
                format!("a second copy of {}, after #{first}", candidate.id),
            );
        } else {
            by_id.insert(&candidate.id, i);
        }
    }

    // An old id gives way to every present mod that replaces it, whatever
    // else is wrong with either; a mod never replaces the one it gave way to.
    let mut replaced: Vec<(usize, usize)> = Vec::new();
    for (i, candidate) in candidates.iter().enumerate() {
        if by_id.get(candidate.id.as_str()) != Some(&i) {
            continue;
        }
        for dependency in &candidate.dependencies {
            if dependency.relation != Relation::REPLACES || dependency.id == candidate.id {
                continue;
            }
            for (old, _) in candidates
                .iter()
                .enumerate()
                .filter(|(_, c)| c.id == dependency.id)
            {
                if replaced.iter().any(|&(o, _)| o == old) || replaced.contains(&(i, old)) {
                    continue;
                }
                skipped.insert(old, format!("replaced by {}", candidate.id));
                replaced.push((old, i));
            }
        }
    }
    replaced.sort_unstable();

    // Requirements and conflicts, until nothing changes: skipping one mod can
    // break another's requirement.
    loop {
        let mut changed = false;
        for (i, candidate) in candidates.iter().enumerate() {
            if skipped.contains_key(&i) {
                continue;
            }
            if let Some(reason) = unmet(candidate, candidates, &by_id, &skipped, i) {
                skipped.insert(i, reason);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut order = Vec::new();
    let mut placed = vec![false; candidates.len()];
    let mut pending: Vec<usize> = (0..candidates.len())
        .filter(|i| !skipped.contains_key(i))
        .collect();
    pending.sort_by(|&a, &b| candidates[a].id.cmp(&candidates[b].id));
    while !pending.is_empty() {
        let ready = pending.iter().position(|&i| {
            candidates[i].dependencies.iter().all(|d| {
                d.relation == Relation::BREAKS
                    || d.relation == Relation::REPLACES
                    || by_id
                        .get(d.id.as_str())
                        .is_none_or(|&j| placed[j] || skipped.contains_key(&j))
            })
        });
        let Some(at) = ready else {
            for &i in &pending {
                skipped.insert(i, "in a dependency cycle".into());
            }
            break;
        };
        let i = pending.remove(at);
        placed[i] = true;
        order.push(i);
    }

    Plan {
        order,
        skipped: skipped.into_iter().collect(),
        replaced,
    }
}

fn unmet(
    candidate: &Candidate,
    candidates: &[Candidate],
    by_id: &BTreeMap<&str, usize>,
    skipped: &BTreeMap<usize, String>,
    me: usize,
) -> Option<String> {
    for dependency in &candidate.dependencies {
        let other = by_id
            .get(dependency.id.as_str())
            .copied()
            .filter(|j| !skipped.contains_key(j));
        match dependency.relation {
            Relation::REQUIRES => {
                let Some(j) = other else {
                    return Some(format!("requires {}, which is not loaded", dependency.id));
                };
                if let Some(range) = &dependency.range
                    && !range.matches(&candidates[j].version)
                {
                    return Some(format!(
                        "requires {} {range}, found {}",
                        dependency.id, candidates[j].version
                    ));
                }
            }
            // The mod that declares the break gives way.
            Relation::BREAKS => {
                if let Some(j) = other.filter(|&j| j != me)
                    && dependency
                        .range
                        .as_ref()
                        .is_none_or(|range| range.matches(&candidates[j].version))
                {
                    return Some(format!(
                        "does not work beside {} {}; remove one of them",
                        dependency.id, candidates[j].version
                    ));
                }
            }
            _ => {}
        }
    }
    None
}

/// Notes on the mods of `plan` whose recommended mods are missing or in
/// another version: the candidate's index and the note.
#[must_use]
pub fn notes(candidates: &[Candidate], plan: &Plan) -> Vec<(usize, String)> {
    let loaded = |id: &str| {
        plan.order
            .iter()
            .map(|&j| &candidates[j])
            .find(|c| c.id == id)
    };
    plan.order
        .iter()
        .flat_map(|&i| {
            candidates[i]
                .dependencies
                .iter()
                .filter(|d| d.relation == Relation::RECOMMENDS)
                .filter_map(move |d| {
                    let wanted = d
                        .range
                        .as_ref()
                        .map_or_else(|| d.id.clone(), |range| format!("{} {range}", d.id));
                    match loaded(&d.id) {
                        None => Some(format!("recommends {wanted}, which is not loaded")),
                        Some(other)
                            if d.range.as_ref().is_some_and(|r| !r.matches(&other.version)) =>
                        {
                            Some(format!("recommends {wanted}, found {}", other.version))
                        }
                        Some(_) => None,
                    }
                    .map(|note| (i, note))
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        id: &str,
        version: &str,
        side: Side,
        dependencies: &[(&str, &str, Relation)],
    ) -> Candidate {
        Candidate {
            id: id.into(),
            version: version.parse().unwrap(),
            side,
            dependencies: dependencies
                .iter()
                .map(|&(id, range, relation)| Dependency {
                    id: id.into(),
                    range: (!range.is_empty()).then(|| range.parse().unwrap()),
                    relation,
                })
                .collect(),
        }
    }

    #[test]
    fn dependencies_load_first_and_order_is_stable() {
        let mods = [
            candidate(
                "survivors",
                "0.1.0",
                Side::BOTH,
                &[("zones", "^0.1", Relation::REQUIRES)],
            ),
            candidate("count", "0.1.0", Side::BOTH, &[]),
            candidate("zones", "0.1.3", Side::BOTH, &[]),
        ];
        let plan = resolve(&mods, Side::SERVER);
        assert_eq!(plan.order, [1, 2, 0]);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn a_missing_or_old_requirement_skips_only_its_dependents() {
        let mods = [
            candidate(
                "survivors",
                "0.1.0",
                Side::BOTH,
                &[("zones", "^0.2", Relation::REQUIRES)],
            ),
            candidate("zones", "0.1.3", Side::BOTH, &[]),
            candidate(
                "extra",
                "1.0.0",
                Side::BOTH,
                &[("survivors", "", Relation::REQUIRES)],
            ),
        ];
        let plan = resolve(&mods, Side::SERVER);
        assert_eq!(plan.order, [1]);
        assert_eq!(
            plan.skipped[0],
            (0, "requires zones ^0.2, found 0.1.3".into())
        );
        assert_eq!(
            plan.skipped[1],
            (2, "requires survivors, which is not loaded".into())
        );
    }

    #[test]
    fn sides_duplicates_and_breaks() {
        let mods = [
            candidate("ui", "1.0.0", Side::CLIENT, &[]),
            candidate("a", "1.0.0", Side::SERVER, &[]),
            candidate("a", "2.0.0", Side::SERVER, &[]),
            candidate("b", "1.0.0", Side::SERVER, &[("a", "", Relation::BREAKS)]),
        ];
        let plan = resolve(&mods, Side::SERVER);
        assert_eq!(plan.order, [1]);
        let reasons: Vec<usize> = plan.skipped.iter().map(|(i, _)| *i).collect();
        assert_eq!(reasons, [0, 2, 3]);
        assert_eq!(
            plan.skipped[2].1,
            "does not work beside a 1.0.0; remove one of them"
        );
    }

    #[test]
    fn breaks_only_in_their_range_and_once_per_pair() {
        let mods = [
            candidate("a", "1.2.0", Side::BOTH, &[("b", "<1.0", Relation::BREAKS)]),
            candidate("b", "1.0.0", Side::BOTH, &[("a", "", Relation::BREAKS)]),
            candidate("c", "0.1.0", Side::BOTH, &[("a", "<1.0", Relation::BREAKS)]),
        ];
        let plan = resolve(&mods, Side::SERVER);
        // `a` allows b 1.0, so b, which breaks every a, gives way; c allows a 1.2.
        assert_eq!(plan.order, [0, 2]);
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].0, 1);
    }

    #[test]
    fn recommendations_order_and_note() {
        let mods = [
            candidate(
                "a",
                "1.0.0",
                Side::BOTH,
                &[
                    ("b", "^2", Relation::RECOMMENDS),
                    ("gone", "", Relation::RECOMMENDS),
                    ("c", "", Relation::RECOMMENDS),
                ],
            ),
            candidate("b", "1.0.0", Side::BOTH, &[]),
            candidate("c", "1.0.0", Side::BOTH, &[]),
        ];
        let plan = resolve(&mods, Side::SERVER);
        assert_eq!(plan.order, [1, 2, 0]);
        assert_eq!(
            notes(&mods, &plan),
            [
                (0, "recommends b ^2, found 1.0.0".into()),
                (0, "recommends gone, which is not loaded".into()),
            ]
        );
    }

    #[test]
    fn a_renamed_mod_replaces_its_old_id() {
        let mods = [
            candidate(
                "optimizer",
                "0.2.0",
                Side::BOTH,
                &[("proton-optimizer", "", Relation::REPLACES)],
            ),
            candidate("proton-optimizer", "0.1.0", Side::BOTH, &[]),
            candidate("proton-optimizer", "0.1.1", Side::BOTH, &[]),
            candidate(
                "uses-old",
                "1.0.0",
                Side::BOTH,
                &[("proton-optimizer", "", Relation::REQUIRES)],
            ),
        ];
        let plan = resolve(&mods, Side::SERVER);
        assert_eq!(plan.order, [0]);
        assert_eq!(plan.replaced, [(1, 0), (2, 0)]);
        assert_eq!(
            plan.skipped,
            [
                (1, "replaced by optimizer".into()),
                (2, "replaced by optimizer".into()),
                (3, "requires proton-optimizer, which is not loaded".into()),
            ]
        );
        // Alone, the old id loads.
        let plan = resolve(&mods[1..2], Side::SERVER);
        assert_eq!(plan.order, [0]);
        assert!(plan.replaced.is_empty());
    }

    #[test]
    fn replacing_needs_the_new_mod_on_this_side_and_never_goes_both_ways() {
        let mods = [
            candidate(
                "new",
                "1.0.0",
                Side::CLIENT,
                &[("old", "", Relation::REPLACES)],
            ),
            candidate("old", "1.0.0", Side::SERVER, &[]),
        ];
        assert_eq!(resolve(&mods, Side::SERVER).order, [1]);
        let mutual = [
            candidate("a", "1.0.0", Side::BOTH, &[("b", "", Relation::REPLACES)]),
            candidate("b", "1.0.0", Side::BOTH, &[("a", "", Relation::REPLACES)]),
            candidate("c", "1.0.0", Side::BOTH, &[("c", "", Relation::REPLACES)]),
        ];
        let plan = resolve(&mutual, Side::SERVER);
        assert_eq!(plan.order, [0, 2]);
        assert_eq!(plan.replaced, [(1, 0)]);
    }

    #[test]
    fn the_client_loads_server_mods_for_the_games_it_hosts() {
        assert!(loads(Side::SERVER, Side::CLIENT));
        assert!(loads(Side::CLIENT, Side::CLIENT));
        assert!(loads(Side::BOTH, Side::SERVER));
        assert!(!loads(Side::CLIENT, Side::SERVER));
        let mods = [
            candidate("comfort", "1.0.0", Side::SERVER, &[]),
            candidate("ui", "1.0.0", Side::CLIENT, &[]),
        ];
        assert_eq!(resolve(&mods, Side::CLIENT).order, [0, 1]);
        assert_eq!(resolve(&mods, Side::SERVER).order, [0]);
    }

    #[test]
    fn after_orders_without_requiring_and_cycles_are_skipped() {
        let mods = [
            candidate(
                "a",
                "1.0.0",
                Side::BOTH,
                &[("b", "", Relation::AFTER), ("gone", "", Relation::AFTER)],
            ),
            candidate("b", "1.0.0", Side::BOTH, &[]),
            candidate("x", "1.0.0", Side::BOTH, &[("y", "", Relation::REQUIRES)]),
            candidate("y", "1.0.0", Side::BOTH, &[("x", "", Relation::REQUIRES)]),
        ];
        let plan = resolve(&mods, Side::CLIENT);
        assert_eq!(plan.order, [1, 0]);
        assert_eq!(
            plan.skipped,
            [
                (2, "in a dependency cycle".into()),
                (3, "in a dependency cycle".into())
            ]
        );
    }
}
