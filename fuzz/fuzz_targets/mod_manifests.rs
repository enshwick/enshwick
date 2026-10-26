//! The manifests of the mods in `mods/`, read and resolved into a load
//! order: every mod either loads or is skipped with a reason, and a loaded
//! mod's requirements load before it.
//!
//! Input: mods separated by `0xfe`, each with the fields id, version, side
//! (`0` to `3`) and dependencies, one `id range relation` per line.

#![no_main]

use enshwick::abi::{Dependency, Header, Manifest, ManifestFlags, Relation, Side, Str};
use enshwick_fuzz::text_fields;
use enshwick_loader::manifest::{meta, parse};
use enshwick_loader::resolve::{Candidate, notes, resolve};
use libfuzzer_sys::fuzz_target;

/// The byte between mods; like the field separator, no UTF-8 text holds it.
const MOD_SEPARATOR: u8 = 0xfe;
/// Mods in one input, at most.
const MAX_MODS: usize = 8;

/// The dependencies a manifest lists, from `id range relation` lines.
fn dependencies(text: &str) -> Vec<Dependency> {
    text.lines()
        .take(8)
        .map(|line| {
            let mut words = line.split(' ');
            let id = words.next().unwrap_or_default();
            let range = words.next().unwrap_or_default();
            let relation = words.next().and_then(|r| r.parse().ok()).unwrap_or(0);
            Dependency {
                id: Str::new(id),
                range: Str::new(range),
                relation: Relation(relation),
            }
        })
        .collect()
}

fn candidate(record: &[u8]) -> Option<Candidate> {
    let [id, version, side, deps] = text_fields::<4>(record)?;
    let dependencies = dependencies(deps);
    let manifest = Manifest {
        header: Header::of::<Manifest>(),
        id: Str::new(id),
        version: Str::new(version),
        side: Side(side.parse().unwrap_or(3)),
        flags: ManifestFlags::JOIN,
        dependencies: dependencies.as_ptr(),
        dependency_count: dependencies.len(),
        builds: core::ptr::null(),
        build_count: 0,
        name: Str::new(id),
        description: Str::EMPTY,
        authors: core::ptr::null(),
        author_count: 0,
        license: Str::EMPTY,
        homepage: Str::EMPTY,
        points: Str::new(deps),
    };
    // SAFETY: every string borrows the input, and `dependencies` holds
    // `dependency_count` entries; all outlive the calls.
    drop(unsafe { meta(&manifest) });
    // SAFETY: every string borrows the input, and `dependencies` holds
    // `dependency_count` entries; both outlive the call.
    unsafe { parse(&manifest) }.ok()
}

fn check(candidates: &[Candidate], side: Side) {
    let plan = resolve(candidates, side);
    let mut seen = vec![false; candidates.len()];
    for &i in plan.order.iter().chain(plan.skipped.iter().map(|(i, _)| i)) {
        assert!(!seen[i], "#{i} is both loaded and skipped, or twice");
        seen[i] = true;
    }
    assert!(
        seen.iter().all(|&s| s),
        "a mod is neither loaded nor skipped"
    );
    for (at, &i) in plan.order.iter().enumerate() {
        let before = &plan.order[..at];
        for dependency in &candidates[i].dependencies {
            if dependency.relation != Relation::REQUIRES {
                continue;
            }
            let answers = |j: usize| {
                candidates[j].id == dependency.id
                    || candidates[j]
                        .dependencies
                        .iter()
                        .any(|d| d.relation == Relation::WAS && d.id == dependency.id)
            };
            assert!(
                before.iter().any(|&j| answers(j)),
                "#{i} loads before what it requires, {}",
                dependency.id
            );
        }
    }
    drop(notes(candidates, &plan));
}

fuzz_target!(|data: &[u8]| {
    let candidates: Vec<Candidate> = data
        .split(|&b| b == MOD_SEPARATOR)
        .take(MAX_MODS)
        .filter_map(candidate)
        .collect();
    for side in [Side::SERVER, Side::CLIENT] {
        check(&candidates, side);
    }
});
