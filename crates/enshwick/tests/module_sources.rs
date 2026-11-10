//! Each module the crate root re-exports names the crate that holds its
//! source, as the docs site reads it: the site finds a module's file in the
//! crate its `pub use` line names.

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// The crate and module of each `pub use <crate>::<module>;` line.
    fn re_exports(lib: &str) -> Vec<(&str, &str)> {
        lib.lines()
            .filter_map(|line| line.strip_prefix("pub use ")?.strip_suffix(';'))
            .filter_map(|path| path.split_once("::"))
            .filter(|(krate, module)| {
                krate.starts_with("enshwick_") && !module.contains(['{', ':'])
            })
            .filter(|(krate, _)| *krate != "enshwick_macros")
            .collect()
    }

    #[test]
    fn every_re_exported_module_has_its_file_in_the_crate_it_is_taken_from() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib = std::fs::read_to_string(crate_dir.join("src/lib.rs")).unwrap();
        let workspace: toml::Table = std::fs::read_to_string(crate_dir.join("../../Cargo.toml"))
            .unwrap()
            .parse()
            .unwrap();
        let deps = &workspace["workspace"]["dependencies"];
        let modules = re_exports(&lib);
        assert!(modules.iter().any(|(_, module)| *module == "abi"));
        let missing: Vec<String> = modules
            .iter()
            .filter(|(krate, module)| {
                let dir = deps[krate.replace('_', "-")]["path"].as_str().unwrap();
                let src = crate_dir.join("../..").join(dir).join("src");
                !src.join(format!("{module}.rs")).is_file() && !src.join(module).is_dir()
            })
            .map(|(krate, module)| format!("{krate}::{module}"))
            .collect();
        assert!(
            missing.is_empty(),
            "re-exported from a crate without their file: {missing:?}"
        );
    }
}
