//! Which mods load, and in what order.
//!
//! Pure logic over the manifests, so it is tested without the game. A mod that
//! cannot load is skipped with a reason; the others load regardless.

use std::collections::BTreeMap;

use enshwick_api::abi::{Relation, Side};
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
}

/// Decides which `candidates` load on `side` and in what order.
#[must_use]
pub fn resolve(candidates: &[Candidate], side: Side) -> Plan {
    let mut skipped: BTreeMap<usize, String> = BTreeMap::new();
    let mut by_id: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, candidate) in candidates.iter().enumerate() {
        if !candidate.side.includes(side) {
            skipped.insert(i, "not for this side of the game".into());
        } else if let Some(&first) = by_id.get(candidate.id.as_str()) {
            skipped.insert(
                i,
                format!("a second copy of {}, after #{first}", candidate.id),
            );
        } else {
            by_id.insert(&candidate.id, i);
        }
    }

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
                d.relation == Relation::CONFLICTS
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
            // The later of two conflicting mods gives way.
            Relation::CONFLICTS if other.is_some_and(|j| j < me) => {
                return Some(format!("conflicts with {}", dependency.id));
            }
            _ => {}
        }
    }
    None
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
    fn sides_duplicates_and_conflicts() {
        let mods = [
            candidate("ui", "1.0.0", Side::CLIENT, &[]),
            candidate("a", "1.0.0", Side::SERVER, &[]),
            candidate("a", "2.0.0", Side::SERVER, &[]),
            candidate(
                "b",
                "1.0.0",
                Side::SERVER,
                &[("a", "", Relation::CONFLICTS)],
            ),
        ];
        let plan = resolve(&mods, Side::SERVER);
        assert_eq!(plan.order, [1]);
        let reasons: Vec<usize> = plan.skipped.iter().map(|(i, _)| *i).collect();
        assert_eq!(reasons, [0, 2, 3]);
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
