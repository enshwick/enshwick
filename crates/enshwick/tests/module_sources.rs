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
            .map(|path| path.strip_suffix("::{self}").unwrap_or(path))
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

    /// The names of the attribute macros `enshwick-macros` defines.
    fn attribute_macros(text: &str) -> Vec<&str> {
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        lines
            .windows(2)
            .filter(|pair| pair[0] == "#[proc_macro_attribute]")
            .filter_map(|pair| pair[1].strip_prefix("pub fn ")?.split('(').next())
            .collect()
    }

    #[test]
    fn a_macro_named_like_a_module_comes_from_the_macro_crate_with_its_docs_here() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib = std::fs::read_to_string(crate_dir.join("src/lib.rs")).unwrap();
        let macros =
            std::fs::read_to_string(crate_dir.join("../enshwick-macros/src/lib.rs")).unwrap();
        let lines: Vec<&str> = lib.lines().collect();
        let shared: Vec<&str> = attribute_macros(&macros)
            .into_iter()
            .filter(|name| {
                let path = format!("pub use enshwick_core::{name}");
                lines.iter().any(|line| {
                    line.strip_prefix(path.as_str())
                        .is_some_and(|rest| rest == ";" || rest == "::{self};")
                })
            })
            .collect();
        assert!(shared.contains(&"config"));
        for name in shared {
            // rustdoc drops the docs of core's re-export of the macro here.
            assert!(
                lines.contains(&format!("pub use enshwick_core::{name}::{{self}};").as_str()),
                "`{name}` takes the macro through core instead of the module alone"
            );
            let at = lines
                .iter()
                .position(|line| *line == format!("pub use enshwick_macros::{name};"))
                .unwrap_or_else(|| panic!("`{name}` is not taken from enshwick_macros"));
            let documented = lines[..at]
                .iter()
                .rev()
                .take_while(|line| line.starts_with('#') || line.starts_with("///"))
                .any(|line| line.starts_with("///"));
            assert!(documented, "`{name}` has no docs on its re-export here");
        }
    }
}
