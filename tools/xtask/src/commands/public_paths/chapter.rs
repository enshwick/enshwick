//! The book's stability chapter against the list: every item its table names
//! is one the crate compiles only with `unstable`, so the API reference, the
//! docs site's coverage table and the chapter say the same.

use std::collections::BTreeMap;

use crate::check::book::stability::chapter_names;

/// The chapter, relative to the workspace root.
pub(crate) const FILE: &str = "book/src/stability.md";

/// The problems with `chapter` against the public path list `list`: an item
/// its table names that the crate lacks, or compiles without `unstable`.
///
/// An item is a name in a row's first cell, read as the book check reads
/// them, that holds `::` or starts with a capital, such as `Field::Slots` or
/// `Notify`. It stands for the list path that is it, or else for the list
/// paths that end in `::` and it with the fewest parts, so `Damage` is
/// `damage::Damage`, not a variant deeper down. A bare lowercase name, such
/// as `world` in "the edits of `world`", is prose about a module, not a
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
        let found = meant(&paths, &name);
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

/// The list paths, with whether each is gated, that the chapter's `name`
/// stands for: the path that is it, or else the shallowest that end in it.
fn meant<'a>(paths: &BTreeMap<&'a str, bool>, name: &str) -> Vec<(&'a str, bool)> {
    if let Some((path, gated)) = paths.get_key_value(name) {
        return vec![(*path, *gated)];
    }
    let suffix = format!("::{name}");
    let ending: Vec<(&str, bool)> = paths
        .iter()
        .filter(|(path, _)| path.ends_with(&suffix))
        .map(|(path, gated)| (*path, *gated))
        .collect();
    let depth = |path: &str| path.matches("::").count();
    let shallowest = ending.iter().map(|(path, _)| depth(path)).min();
    ending
        .into_iter()
        .filter(|(path, _)| Some(depth(path)) == shallowest)
        .collect()
}

/// The items the first cells of the chapter's table rows name.
fn named(chapter: &str) -> Vec<String> {
    chapter_names(chapter)
        .into_iter()
        .filter(|name| name.contains("::") || name.starts_with(|c: char| c.is_ascii_uppercase()))
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
    fn a_short_name_means_its_shallowest_path_not_a_gated_one_deeper_down() {
        let list = "damage::Damage\nstats::StatGroup::Damage unstable\n";
        let found = problems(&chapter("`Damage`"), list);

        assert_eq!(found.len(), 1);
        assert!(found[0].contains("compiles damage::Damage without"));
    }

    #[test]
    fn a_name_after_of_is_context_and_calls_generics_and_indented_rows_are_read() {
        let text = "  | the `Prop` fields of `World` | young |\n| `Notify()` and `Field::Output<T>` | young |\n";
        let found = problems(text, LIST);

        assert_eq!(found.len(), 1);
        assert!(found[0].contains("`Field::Output`"));
    }

    #[test]
    fn a_bare_module_name_text_outside_the_first_cell_and_file_names_are_left_alone() {
        let text = "`Field::Output` in prose\n| `world` and `items.toml` | `Field::Output` |\n";

        assert_eq!(problems(text, LIST), Vec::<String>::new());
    }
}
