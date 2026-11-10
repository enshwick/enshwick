//! The committed C header states the ABI version this crate is built with.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use enshwick::abi::{ABI_MAJOR, ABI_MINOR};

    #[test]
    fn the_c_header_defines_this_abi_version() {
        let header = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("include/enshwick.h"),
        )
        .unwrap();
        for define in [
            format!("#define ENSHWICK_ABI_MAJOR {ABI_MAJOR}"),
            format!("#define ENSHWICK_ABI_MINOR {ABI_MINOR}"),
        ] {
            assert!(
                header.lines().any(|line| line == define),
                "include/enshwick.h lacks `{define}`: run `cargo xtask header`"
            );
        }
    }
}
