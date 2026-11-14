//! The rules `cargo xtask check` holds the tree to without compiling, one
//! module each, run in the order of [`RULES`].

pub(crate) mod book;
pub(crate) mod changelog;
pub(crate) mod crate_rules;
mod domains;
mod ranges;
pub(crate) mod reports;
pub(crate) mod rules;

use std::path::Path;

use crate::affected::Package;
use crate::commands::{lower_examples, tag_lines};
use crate::say;

/// A rule over the tree at the repository root, given the workspace's
/// packages: its problems, one line each.
type Rule = fn(&Path, &[Package]) -> Result<Vec<String>, String>;

/// Every rule with the label its problems carry, in the order they run.
const RULES: [(&str, Rule); 12] = [
    ("manifest range", |root, packages| {
        Ok(ranges::check(root, packages))
    }),
    ("book", |root, _| Ok(book::check(root))),
    ("book abi coverage", |root, _| {
        Ok(book::coverage_problems(root))
    }),
    ("book prose", |root, _| Ok(book::prose::check(root))),
    ("book stability", |root, packages| {
        Ok(book::stability::check(root, packages))
    }),
    ("book link", |root, _| {
        let mods = book::links::mods_clone(root).map_err(|why| format!("book links: {why}"))?;
        let links = book::links::check(root, mods.as_deref());
        if let Some(note) = &links.skipped {
            say(note);
        }
        Ok(links.problems)
    }),
    ("tag line", |root, packages| {
        Ok(tag_lines::check(root, packages))
    }),
    ("crate rule", |_, packages| crate_rules::check(packages)),
    ("domain rule", |root, _| Ok(domains::check(root))),
    ("internal example", |root, packages| {
        Ok(lower_examples::check(root, packages))
    }),
    ("compatibility report", |root, _| {
        Ok(reports::stale(root, &[]))
    }),
    ("changelog", |root, packages| {
        Ok(changelog::check(root, packages))
    }),
];

/// The problems every rule finds in the tree at `root`, each line opening
/// with its rule's label.
///
/// # Errors
///
/// When a rule's input does not read.
pub(crate) fn problems(root: &Path, packages: &[Package]) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for (label, rule) in RULES {
        problems.extend(
            rule(root, packages)?
                .into_iter()
                .map(|problem| format!("{label}: {problem}")),
        );
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_has_a_label_of_its_own() {
        let mut labels: Vec<&str> = RULES.iter().map(|(label, _)| *label).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), RULES.len());
    }
}
