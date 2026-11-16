//! The orders the joins walk the loader's parts in, apart from the parts
//! themselves, which call into the running game: the feature table by name
//! and each stage of a step in layer order.

/// The rows of the feature table: each part's name with the build table ids
/// it names, those that name none left out, sorted by name.
pub(crate) fn feature_rows(
    rows: impl Iterator<Item = (&'static str, &'static [&'static str])>,
) -> Box<[(&'static str, &'static [&'static str])]> {
    let mut rows: Vec<_> = rows.filter(|(_, ids)| !ids.is_empty()).collect();
    rows.sort_unstable_by_key(|&(name, _)| name);
    rows.into_boxed_slice()
}

/// The `items` of each of `stages`, by its index there, each kept in the
/// order given; `items` gives each item with its stage.
pub(crate) fn by_stage<S: Copy + PartialEq, T, I, const N: usize>(
    stages: [S; N],
    items: impl Fn() -> I,
) -> [Box<[T]>; N]
where
    I: Iterator<Item = (S, T)>,
{
    stages.map(|stage| {
        items()
            .filter(|(of, _)| *of == stage)
            .map(|(_, item)| item)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feature_table_lists_the_parts_that_name_ids_by_name() {
        let rows = feature_rows(
            [
                ("zeta", &["b.one"][..]),
                ("high", &[][..]),
                ("alpha", &["a.one", "a.two"][..]),
            ]
            .into_iter(),
        );
        let names: Vec<_> = rows.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["alpha", "zeta"]);
        assert_eq!(rows[0].1, ["a.one", "a.two"]);
    }

    #[test]
    fn each_stage_keeps_its_items_in_the_order_given() {
        let items = [(1, "low output"), (0, "low follow"), (0, "high follow")];
        let [follow, input, output] = by_stage([0, 2, 1], || items.into_iter());
        assert_eq!(*follow, ["low follow", "high follow"]);
        assert!(input.is_empty());
        assert_eq!(*output, ["low output"]);
    }
}
