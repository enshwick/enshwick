//! The fake loader reads core's hidden items only through `__fake`, the one
//! module that lists them and that only a test build holds.

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn sources(dir: &Path, found: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                found.push((path.display().to_string(), text));
            }
        }
    }

    #[test]
    fn the_fake_reads_core_s_hidden_items_only_through_its_fake_module() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found = Vec::new();
        sources(&crate_dir.join("../internal/fake/src"), &mut found);
        let past: Vec<String> = found
            .iter()
            .flat_map(|(path, text)| {
                text.lines()
                    .enumerate()
                    .filter(|(_, line)| {
                        !line.trim_start().starts_with("//") && line.contains("__private")
                    })
                    .map(move |(at, _)| format!("{path}:{}", at + 1))
            })
            .collect();
        assert!(past.is_empty(), "{past:?}");
    }
}
