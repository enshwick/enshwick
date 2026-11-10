//! The doc examples of this crate and of `enshwick-core` declare the same
//! mod: `declare_mod!` and `deps!` read the `Cargo.toml` of the crate whose
//! doctest runs, so each crate holds the table, and the two stay equal.

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn mod_table(manifest: &Path) -> toml::Value {
        let text = std::fs::read_to_string(manifest).unwrap();
        let table: toml::Table = text.parse().unwrap();
        table["package"]["metadata"]["enshwick"].clone()
    }

    #[test]
    fn the_examples_of_the_facade_and_its_core_declare_one_mod() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            mod_table(&crate_dir.join("Cargo.toml")),
            mod_table(&crate_dir.join("../internal/core/Cargo.toml"))
        );
    }
}
