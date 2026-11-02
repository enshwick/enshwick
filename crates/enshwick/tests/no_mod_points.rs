//! The API crate declares no mod's extension points: a mod writes its
//! definitions of another mod's points with `cargo enshwick deps`.

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
    fn the_api_crate_declares_no_point() {
        let mut found = Vec::new();
        sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut found,
        );
        let declaring: Vec<&str> = found
            .iter()
            .filter(|(_, text)| {
                text.lines().any(|line| {
                    let line = line.trim_start();
                    !line.starts_with("//") && line.contains("point(name =")
                })
            })
            .map(|(path, _)| path.as_str())
            .collect();
        assert!(declaring.is_empty(), "{declaring:?}");
    }
}
