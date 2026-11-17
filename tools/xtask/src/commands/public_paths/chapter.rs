//! The book's stability chapter against the list: every item its table names
//! is one the crate compiles only with `unstable`, so the API reference, the
//! docs site's coverage table and the chapter say the same.

use std::collections::BTreeMap;

/// The chapter, relative to the workspace root.
pub(crate) const FILE: &str = "book/src/stability.md";

/// The problems with `chapter` against the public path list `list`: an item
/// its table names that the crate lacks, or compiles without `unstable`.
///
/// An item is a backticked name in a row's first cell that holds `::` or
/// starts with a capital, such as `Field::Slots` or `Notify`, and stands for
/// every list path that is it or ends in `::` and it. A bare lowercase name,
/// such as `world` in "the edits of `world`", is prose about a module, not a
/// claim that the whole module is unstable, and is left alone.
pub(super) fn problems(chapter: &str, list: &str) -> Vec<String> {
    let paths: BTreeMap<&str, bool> = list
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            match line
                .strip_suffix(super::UNSTABLE)
                .and_then(|path| path.strip_suffix(' '))
            {
                Some(path) => (path, true),
                None => (line, false),
            }
        })
        .collect();
    let mut problems = Vec::new();
    for name in named(chapter) {
        let suffix = format!("::{name}");
        let found: Vec<(&str, bool)> = paths
            .iter()
            .filter(|(path, _)| **path == name || path.ends_with(&suffix))
            .map(|(path, gated)| (*path, *gated))
            .collect();
        if found.is_empty() {
            problems.push(format!(
                "{FILE} names `{name}` unstable, but the crate has no such public item"
            ));
        } else if found.iter().all(|(_, gated)| !gated) {
            let paths: Vec<&str> = found.iter().map(|(path, _)| *path).collect();
            problems.push(format!(
                "{FILE} names `{name}` unstable, but the crate compiles {} without the `unstable` feature",
                paths.join(", ")
            ));
        }
    }
    problems
}

/// The items the first cells of the chapter's table rows name, in order.
fn named(chapter: &str) -> Vec<&str> {
    chapter
        .lines()
        .filter_map(|line| line.strip_prefix('|')?.split('|').next())
        .flat_map(|cell| cell.split('`').skip(1).step_by(2))
        .filter(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
                && (name.contains("::") || name.starts_with(|c: char| c.is_ascii_uppercase()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::problems;

    const LIST: &str = "\
# header
world
world::Prop unstable
world::World
content::changes::Field::MaxStackSize unstable
content::changes::Field::Output
effects::Notify unstable
";

    fn chapter(row: &str) -> String {
        format!("| Part | Why |\n| --- | --- |\n| {row} | young |\n")
    }

    #[test]
    fn an_item_the_list_marks_unstable_passes_by_its_path_or_its_last_parts() {
        let row = "The edits of `world`: `world::Prop`, `Field::MaxStackSize`, `Notify`";

        assert_eq!(problems(&chapter(row), LIST), Vec::<String>::new());
    }

    #[test]
    fn an_item_the_crate_compiles_without_unstable_is_a_problem() {
        let found = problems(&chapter("Changes of `Field::Output`"), LIST);

        assert_eq!(found.len(), 1);
        assert!(found[0].contains("content::changes::Field::Output"));
    }

    #[test]
    fn an_item_the_crate_lacks_is_a_problem() {
        let found = problems(&chapter("`World::gone` and `Missing`"), LIST);

        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .all(|problem| problem.contains("no such public item"))
        );
    }

    #[test]
    fn a_bare_module_name_text_outside_the_first_cell_and_file_names_are_left_alone() {
        let text = "`Field::Output` in prose\n| `world` and `items.toml` | `Field::Output` |\n";

        assert_eq!(problems(text, LIST), Vec::<String>::new());
    }
}
