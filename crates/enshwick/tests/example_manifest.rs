//! The doc examples of this crate, `enshwick-core` and `enshwick-fake`
//! declare the same mod: `declare_mod!` and `deps!` read the `Cargo.toml` of
//! the crate whose doctest runs, so each crate holds the table, and the
//! three stay equal.

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn mod_table(manifest: &Path) -> toml::Value {
        let text = std::fs::read_to_string(manifest).unwrap();
        let table: toml::Table = text.parse().unwrap();
        table["package"]["metadata"]["enshwick"].clone()
    }

    #[test]
    fn the_examples_of_the_facade_its_core_and_its_fake_declare_one_mod() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let facade = mod_table(&crate_dir.join("Cargo.toml"));
        for part in ["core", "fake"] {
            let manifest = crate_dir.join(format!("../internal/{part}/Cargo.toml"));
            assert_eq!(facade, mod_table(&manifest), "{part}");
        }
    }
}
