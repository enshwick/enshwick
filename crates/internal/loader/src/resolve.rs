//! Which mods load, and in what order.
//!
//! Pure logic over the manifests, so it is tested without the game. A mod that
//! cannot load is skipped with a reason; the others load regardless.

use std::collections::BTreeMap;
use std::time::SystemTime;

use enshwick_core::abi::{Relation, Side};
use semver::{Version, VersionReq};

/// A mod's manifest, parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The mod id.
    pub id: String,
    /// Its version.
    pub version: Version,
    /// Where it runs, [`Side::SERVER`] or [`Side::BOTH`]: every game binary
    /// loads both, since a client runs the server's code while it hosts a
    /// game, solo play included.
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
    pub skipped: Vec<(usize, Skip)>,
}

/// Why a mod does not load: the log names it in English, the player reads
/// [`Skip::shown`] in their language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Skip {
    /// A second copy of a mod id.
    Copy {
        /// The mod id.
        id: String,
        /// The index of the copy that loads.
        first: usize,
    },
    /// A required mod is not loaded.
    Missing {
        /// The required mod's id.
        id: String,
    },
    /// A required mod loads in a version outside the range asked for.
    Version {
        /// The required mod's id.
        id: String,
        /// The versions asked for.
        range: VersionReq,
        /// The version that loads.
        found: Version,
    },
    /// The mod does not work beside another that loads.
    Breaks {
        /// The other mod's id.
        id: String,
        /// The other mod's version.
        version: Version,
    },
    /// The mod is in a cycle of requirements.
    Cycle,
}

/// A mod that does not load, as the Mods screen and the chat line at world
/// load name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    /// The mod id.
    pub id: String,
    /// Its version.
    pub version: String,
    /// The name players read, or the id when the manifest names none.
    pub label: String,
    /// Its file relative to the game's folder, so a player finds the copy
    /// to act on.
    pub file: String,
    /// Why it does not load.
    pub reason: Skip,
}

impl Skipped {
    /// Whether it counts as a mod that is off: a second copy of a mod that
    /// runs does not, since that mod runs; `loads` tells whether an id runs.
    #[must_use]
    pub fn counts_off(&self, loads: impl Fn(&str) -> bool) -> bool {
        !matches!(&self.reason, Skip::Copy { id, .. } if loads(id))
    }
}

impl core::fmt::Display for Skip {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Copy { id, first } => write!(f, "a second copy of {id}, after #{first}"),
            Self::Missing { id } => write!(f, "requires {id}, which is not loaded"),
            Self::Version { id, range, found } => write!(f, "requires {id} {range}, found {found}"),
            Self::Breaks { id, version } => {
                write!(f, "does not work beside {id} {version}; remove one of them")
            }
            Self::Cycle => f.write_str("in a dependency cycle"),
        }
    }
}

impl Skip {
    /// The reason in the player's language, as the Mods screen and the chat
    /// line at world load show it.
    #[must_use]
    pub fn shown(&self) -> String {
        match self {
            Self::Copy { id, .. } => crate::texts::fill("mods.skipped.copy", &[("id", id)]),
            Self::Missing { id } => crate::texts::fill("mods.skipped.missing", &[("id", id)]),
            Self::Version { id, range, found } => crate::texts::fill(
                "mods.skipped.version",
                &[
                    ("id", id),
                    ("range", &range.to_string()),
                    ("found", &found.to_string()),
                ],
            ),
            Self::Breaks { id, version } => crate::texts::fill(
                "mods.skipped.breaks",
                &[("id", id), ("version", &version.to_string())],
            ),
            Self::Cycle => crate::texts::fill("mods.skipped.cycle", &[]),
        }
    }
}

/// A file in `mods/` that declares a mod, weighed by [`duplicates`] against
/// other files that declare the same id.
#[derive(Clone, Copy, Debug)]
pub struct Installed<'a> {
    /// The mod id its manifest declares.
    pub id: &'a str,
    /// The version its manifest declares.
    pub version: &'a Version,
    /// When the file last changed; `None`, when the system cannot tell,
    /// counts as the oldest.
    pub modified: Option<SystemTime>,
    /// Its file name, as the log names it.
    pub file: &'a str,
}

/// Chooses which of several files declaring one mod id runs, so a renamed DLL
/// left beside its old name runs the newer build.
///
/// The highest version runs (build metadata aside), then the newest file, then
/// the first by name. Returns each other copy's index with the index of the
/// copy that runs instead, in the order of `files`.
#[must_use]
pub fn duplicates(files: &[Installed<'_>]) -> Vec<(usize, usize)> {
    let outranks = |a: &Installed<'_>, b: &Installed<'_>| {
        a.version
            .cmp_precedence(b.version)
            .then(a.modified.cmp(&b.modified))
            .then(b.file.cmp(a.file))
            .is_gt()
    };
    let mut runs: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, file) in files.iter().enumerate() {
        runs.entry(file.id)
            .and_modify(|best| {
                if outranks(file, &files[*best]) {
                    *best = i;
                }
            })
            .or_insert(i);
    }
    files
        .iter()
        .enumerate()
        .filter_map(|(i, file)| {
            let best = *runs.get(file.id)?;
            (best != i).then_some((i, best))
        })
        .collect()
}

/// Decides which `candidates` load and in what order.
#[must_use]
pub fn resolve(candidates: &[Candidate]) -> Plan {
    let mut skipped: BTreeMap<usize, Skip> = BTreeMap::new();
    let mut by_id: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, candidate) in candidates.iter().enumerate() {
        if let Some(&first) = by_id.get(candidate.id.as_str()) {
            skipped.insert(
                i,
                Skip::Copy {
                    id: candidate.id.clone(),
                    first,
                },
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
        // `soft`: whether `AFTER` holds the mod back too.
        let ready = |soft: bool| {
            pending.iter().position(|&i| {
                candidates[i].dependencies.iter().all(|d| {
                    d.relation == Relation::BREAKS
                        || (!soft && d.relation != Relation::REQUIRES)
                        || by_id
                            .get(d.id.as_str())
                            .is_none_or(|&j| placed[j] || skipped.contains_key(&j))
                })
            })
        };
        // A cycle of soft orders only gives way: its mods load anyway.
        let Some(at) = ready(true).or_else(|| ready(false)) else {
            for &i in &pending {
                skipped.insert(i, Skip::Cycle);
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

/// Whether `version` is a dev build, the version of every crate before the
/// first release: like two dev builds joining each other, it meets every
/// range, and no break names it.
fn dev(version: &Version) -> bool {
    (version.major, version.minor, version.patch) == (0, 0, 0) && version.pre.as_str() == "dev"
}

/// Whether `version` meets `range`, a dev build always.
fn meets(range: &VersionReq, version: &Version) -> bool {
    dev(version) || range.matches(version)
}

fn unmet(
    candidate: &Candidate,
    candidates: &[Candidate],
    by_id: &BTreeMap<&str, usize>,
    skipped: &BTreeMap<usize, Skip>,
    me: usize,
) -> Option<Skip> {
    for dependency in &candidate.dependencies {
        let other = by_id
            .get(dependency.id.as_str())
            .copied()
            .filter(|j| !skipped.contains_key(j));
        match dependency.relation {
            Relation::REQUIRES => {
                let Some(j) = other else {
                    return Some(Skip::Missing {
                        id: dependency.id.clone(),
                    });
                };
                if let Some(range) = &dependency.range
                    && !meets(range, &candidates[j].version)
                {
                    return Some(Skip::Version {
                        id: dependency.id.clone(),
                        range: range.clone(),
                        found: candidates[j].version.clone(),
                    });
                }
            }
            // The mod that declares the break gives way.
            Relation::BREAKS => {
                if let Some(j) = other.filter(|&j| j != me)
                    && dependency.range.as_ref().is_none_or(|range| {
                        !dev(&candidates[j].version) && range.matches(&candidates[j].version)
                    })
                {
                    return Some(Skip::Breaks {
                        id: dependency.id.clone(),
                        version: candidates[j].version.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

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
    fn only_a_second_copy_of_a_mod_that_runs_does_not_count_as_off() {
        let skipped = |reason| Skipped {
            id: "you.safes".into(),
            version: "1.0.0".into(),
            label: "Safes".into(),
            file: "you.safes/you.safes.dll".into(),
            reason,
        };
        let copy = skipped(Skip::Copy {
            id: "you.safes".into(),
            first: 0,
        });
        assert!(!copy.counts_off(|id| id == "you.safes"));
        assert!(copy.counts_off(|_| false), "no copy of it runs");
        let missing = skipped(Skip::Missing { id: "x".into() });
        assert!(missing.counts_off(|_| true));
    }

    #[test]
    fn dependencies_load_first_and_order_is_stable() {
        let mods = [
            candidate(
                "you.market-stalls",
                "0.1.0",
                Side::BOTH,
                &[("zones", "^0.1", Relation::REQUIRES)],
            ),
            candidate("count", "0.1.0", Side::BOTH, &[]),
            candidate("zones", "0.1.3", Side::BOTH, &[]),
        ];
        let plan = resolve(&mods);
        assert_eq!(plan.order, [1, 2, 0]);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn a_dev_build_meets_every_range_and_no_break() {
        let mods = [
            candidate(
                "stalls",
                "0.0.0-dev",
                Side::BOTH,
                &[
                    ("post", ">=0.1, <1.0", Relation::REQUIRES),
                    ("you.trading-post", "", Relation::AFTER),
                    ("zones", "<0.3", Relation::BREAKS),
                ],
            ),
            candidate("post", "0.0.0-dev", Side::BOTH, &[]),
            candidate("you.trading-post", "0.0.0-dev", Side::BOTH, &[]),
            candidate("zones", "0.0.0-dev", Side::BOTH, &[]),
        ];
        let plan = resolve(&mods);
        assert!(plan.skipped.is_empty(), "{:?}", plan.skipped);
        let old = [
            candidate(
                "stalls",
                "0.0.0-dev",
                Side::BOTH,
                &[("post", "^0.2", Relation::REQUIRES)],
            ),
            candidate("post", "0.1.0", Side::BOTH, &[]),
        ];
        assert_eq!(resolve(&old).skipped.len(), 1);
    }

    #[test]
    fn a_missing_or_old_requirement_skips_only_its_dependents() {
        let mods = [
            candidate(
                "you.market-stalls",
                "0.1.0",
                Side::BOTH,
                &[("zones", "^0.2", Relation::REQUIRES)],
            ),
            candidate("zones", "0.1.3", Side::BOTH, &[]),
            candidate(
                "extra",
                "1.0.0",
                Side::BOTH,
                &[("you.market-stalls", "", Relation::REQUIRES)],
            ),
        ];
        let plan = resolve(&mods);
        assert_eq!(plan.order, [1]);
        assert_eq!(
            reasons(&plan),
            [
                (0, "requires zones ^0.2, found 0.1.3".into()),
                (2, "requires you.market-stalls, which is not loaded".into())
            ]
        );
    }

    #[test]
    fn duplicates_and_breaks_are_skipped_and_both_sides_load() {
        let mods = [
            candidate("ui", "1.0.0", Side::BOTH, &[]),
            candidate("a", "1.0.0", Side::SERVER, &[]),
            candidate("a", "2.0.0", Side::SERVER, &[]),
            candidate("b", "1.0.0", Side::SERVER, &[("a", "", Relation::BREAKS)]),
        ];
        let plan = resolve(&mods);
        assert_eq!(plan.order, [1, 0]);
        let reasons: Vec<usize> = plan.skipped.iter().map(|(i, _)| *i).collect();
        assert_eq!(reasons, [2, 3]);
        assert_eq!(
            plan.skipped[1].1.to_string(),
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
        let plan = resolve(&mods);
        // `a` allows b 1.0, so b, which breaks every a, gives way; c allows a 1.2.
        assert_eq!(plan.order, [0, 2]);
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].0, 1);
    }

    #[test]
    fn an_after_dependency_orders_and_its_absence_skips_nothing() {
        let mods = [
            candidate(
                "a",
                "1.0.0",
                Side::BOTH,
                &[("b", "", Relation::AFTER), ("gone", "", Relation::AFTER)],
            ),
            candidate("b", "1.0.0", Side::SERVER, &[]),
        ];
        let plan = resolve(&mods);
        assert_eq!(plan.order, [1, 0]);
        assert!(plan.skipped.is_empty());
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
        let plan = resolve(&mods);
        assert_eq!(plan.order, [1, 0]);
        assert_eq!(
            reasons(&plan),
            [
                (2, "in a dependency cycle".into()),
                (3, "in a dependency cycle".into())
            ]
        );
    }

    #[test]
    fn a_cycle_of_soft_orders_still_loads_every_mod() {
        let mods = [
            candidate("c", "1.0.0", Side::BOTH, &[("d", "", Relation::AFTER)]),
            candidate("d", "1.0.0", Side::BOTH, &[("c", "", Relation::AFTER)]),
            candidate("e", "1.0.0", Side::BOTH, &[("x", "", Relation::AFTER)]),
            candidate("x", "1.0.0", Side::BOTH, &[("y", "", Relation::REQUIRES)]),
            candidate("y", "1.0.0", Side::BOTH, &[("x", "", Relation::REQUIRES)]),
        ];
        let plan = resolve(&mods);
        assert_eq!(plan.order, [0, 1, 2]);
        assert_eq!(
            reasons(&plan),
            [
                (3, "in a dependency cycle".into()),
                (4, "in a dependency cycle".into())
            ]
        );
    }

    fn reasons(plan: &Plan) -> Vec<(usize, String)> {
        plan.skipped
            .iter()
            .map(|(i, skip)| (*i, skip.to_string()))
            .collect()
    }

    #[test]
    fn a_skipped_mod_s_reason_reads_in_the_player_s_language() {
        let skip = Skip::Missing {
            id: "enshwick.not-installed".into(),
        };
        let uk = Some(enshwick_core::i18n::Language::UK_UA);
        let shown = crate::texts::for_reader(uk, || skip.shown());
        assert!(shown.contains("enshwick.not-installed"), "{shown}");
        assert_ne!(shown, crate::texts::for_reader(None, || skip.shown()));
    }

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn installed<'a>(
        id: &'a str,
        version: &'a Version,
        modified: SystemTime,
        file: &'a str,
    ) -> Installed<'a> {
        Installed {
            id,
            version,
            modified: Some(modified),
            file,
        }
    }

    #[test]
    fn a_renamed_file_with_the_same_version_runs_when_it_is_newer() {
        let dev: Version = "0.0.0-dev".parse().unwrap();
        let files = [
            installed("you.lanterns", &dev, at(100), "lanterns.dll"),
            installed("you.lanterns", &dev, at(200), "lanterns-renamed.dll"),
        ];
        assert_eq!(duplicates(&files), [(0, 1)]);
    }

    #[test]
    fn the_highest_version_runs_whatever_the_file_times_and_names() {
        let old: Version = "1.2.0".parse().unwrap();
        let new: Version = "1.10.0".parse().unwrap();
        let files = [
            installed("a", &new, at(100), "z.dll"),
            installed("a", &old, at(900), "a.dll"),
        ];
        assert_eq!(duplicates(&files), [(1, 0)]);
    }

    #[test]
    fn equal_versions_and_times_run_the_first_file_by_name() {
        let version: Version = "1.0.0+one".parse().unwrap();
        let other: Version = "1.0.0+two".parse().unwrap();
        let files = [
            installed("a", &other, at(100), "b.dll"),
            installed("a", &version, at(100), "a.dll"),
        ];
        assert_eq!(duplicates(&files), [(0, 1)]);
    }

    #[test]
    fn a_file_of_unknown_age_counts_as_the_oldest() {
        let version: Version = "1.0.0".parse().unwrap();
        let files = [
            Installed {
                id: "a",
                version: &version,
                modified: None,
                file: "a.dll",
            },
            installed("a", &version, at(1), "b.dll"),
        ];
        assert_eq!(duplicates(&files), [(0, 1)]);
    }

    #[test]
    fn every_extra_copy_names_the_one_that_runs_and_other_ids_stay() {
        let low: Version = "0.1.0".parse().unwrap();
        let high: Version = "0.2.0".parse().unwrap();
        let files = [
            installed("a", &low, at(5), "a1.dll"),
            installed("b", &low, at(5), "b.dll"),
            installed("a", &high, at(1), "a2.dll"),
            installed("a", &low, at(9), "a3.dll"),
        ];
        assert_eq!(duplicates(&files), [(0, 2), (3, 2)]);
    }
}
