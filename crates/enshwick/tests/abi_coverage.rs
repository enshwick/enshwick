//! The book's ABI test coverage chapter names every host table entry once,
//! in the table's order, so an appended entry cannot skip the audit.

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// The fields of `HostApi` in the committed layout, in order.
    fn host_api_fields() -> Vec<String> {
        let layout = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/abi/layout.txt"),
        )
        .unwrap();
        layout
            .lines()
            .skip_while(|line| !line.starts_with("HostApi "))
            .skip(1)
            .take_while(|line| line.starts_with(' '))
            .map(|line| line.split_whitespace().next().unwrap().to_owned())
            .collect()
    }

    /// The entries of the rows of the chapter's entries table, in order.
    fn chapter_entries() -> Vec<String> {
        let chapter = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../book/src/abi-coverage.md"),
        )
        .unwrap();
        chapter
            .lines()
            .skip_while(|line| *line != "## The entries")
            .skip(1)
            .take_while(|line| !line.starts_with("## "))
            .filter_map(|line| line.strip_prefix("| `"))
            .filter_map(|rest| rest.split_once('`'))
            .map(|(entry, _)| entry.to_owned())
            .collect()
    }

    #[test]
    fn the_coverage_chapter_lists_every_host_table_entry_in_order() {
        let fields = host_api_fields();
        assert!(fields.len() > 100, "{fields:?}");
        assert_eq!(chapter_entries(), fields);
    }
}
