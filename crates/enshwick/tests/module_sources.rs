//! Each module the crate root re-exports names the crate that holds its
//! source, as the docs site reads it: the site finds a module's file in the
//! crate its `pub use` line names.

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// The tail that takes `name`'s module alone; rustdoc 1.100 files a bare
    /// `{self}` under a module called `self`, so the module is named again.
    fn module_alone(name: &str) -> String {
        format!("::{{self as {name}}}")
    }

    /// The `<crate>::<module>` of a re-export path `<crate>::<module>::{self as
    /// <module>}`, which takes the module alone under its own name.
    fn alone(path: &str) -> Option<&str> {
        path.rsplit_once("::")
            .filter(|(head, last)| {
                last.strip_prefix("{self as ")
                    .and_then(|name| name.strip_suffix('}'))
                    .is_some_and(|name| {
                        head.rsplit_once("::")
                            .is_some_and(|(_, module)| module == name)
                    })
            })
            .map(|(head, _)| head)
    }

    /// The path of each `pub use <path>;` line.
    fn use_paths(lib: &str) -> impl Iterator<Item = &str> {
        lib.lines()
            .filter_map(|line| line.strip_prefix("pub use ")?.strip_suffix(';'))
    }

    /// The crate and module of each `pub use <crate>::<module>;` line.
    fn re_exports(lib: &str) -> Vec<(&str, &str)> {
        use_paths(lib)
            .map(|path| alone(path).unwrap_or(path))
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

    #[test]
    fn a_module_taken_alone_is_named_again_with_its_own_name() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib = std::fs::read_to_string(crate_dir.join("src/lib.rs")).unwrap();
        let wrong: Vec<&str> = use_paths(&lib)
            .filter(|path| path.ends_with('}') && path.contains("self"))
            .filter(|path| alone(path).is_none())
            .collect();
        assert!(
            wrong.is_empty(),
            "take a module alone as `<module>::{{self as <module>}}`: {wrong:?}"
        );
    }

    #[test]
    fn only_a_module_named_again_with_its_own_name_counts_as_alone() {
        assert_eq!(
            alone("enshwick_base::config::{self as config}"),
            Some("enshwick_base::config")
        );
        for path in [
            "enshwick_base::config::{self}",
            "enshwick_abi::raw_abi::{self as abi}",
            "enshwick_base::config::{self, Config}",
        ] {
            assert_eq!(alone(path), None, "{path}");
        }
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
            std::fs::read_to_string(crate_dir.join("../internal/macros/src/lib.rs")).unwrap();
        let lines: Vec<&str> = lib.lines().collect();
        // Every internal crate but the macros' own, whose macros this crate
        // takes as they are.
        let from: Vec<String> = crate::internal_crates::internal_crates()
            .into_iter()
            .filter(|(name, _)| name != "macros")
            .map(|(name, _)| format!("enshwick_{name}"))
            .collect();
        let shared: Vec<(&str, &str)> = attribute_macros(&macros)
            .into_iter()
            .flat_map(|name| from.iter().map(move |krate| (krate.as_str(), name)))
            .filter(|(krate, name)| {
                let path = format!("pub use {krate}::{name}");
                lines.iter().any(|line| {
                    line.strip_prefix(path.as_str())
                        .is_some_and(|rest| rest == ";" || rest == module_alone(name) + ";")
                })
            })
            .collect();
        assert!(shared.iter().any(|&(_, name)| name == "config"));
        for (krate, name) in shared {
            // rustdoc drops the docs of the internal crate's re-export of the
            // macro here.
            assert!(
                lines.contains(&format!("pub use {krate}::{name}{};", module_alone(name)).as_str()),
                "`{name}` takes the macro through {krate} instead of the module alone"
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
