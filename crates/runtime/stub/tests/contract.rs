//! The start contract between the stub and the loader's `enshwick_start`.

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// The value of `const STUB_ABI: u32 = <n>;` in the source file at
    /// `path`, relative to this crate.
    fn stub_abi_in(path: &str) -> u32 {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        let text = std::fs::read_to_string(&file).unwrap();
        let line = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("const STUB_ABI: u32 = "))
            .unwrap_or_else(|| panic!("{} declares no STUB_ABI", file.display()));
        line.trim_end_matches(';').parse().unwrap()
    }

    #[test]
    fn the_stub_and_the_loader_speak_the_same_start_contract() {
        assert_eq!(
            stub_abi_in("src/lib.rs"),
            stub_abi_in("../loader/src/start/game.rs")
        );
    }
}
