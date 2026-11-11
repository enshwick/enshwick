//! The path part of [`item_path!`](crate::typed::runtime::item_path): a
//! Rust path without the name of the crate it was written in.

/// `path` from its first `::` on: a Rust path without the crate's name.
#[must_use]
pub const fn after_crate(path: &str) -> &str {
    let bytes = path.as_bytes();
    let mut at = 0;
    while at + 1 < bytes.len() {
        if bytes[at] == b':' && bytes[at + 1] == b':' {
            return path.split_at(at).1;
        }
        at += 1;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::after_crate;

    #[test]
    fn a_path_loses_its_crate_s_name_and_a_bare_name_stays() {
        assert_eq!(
            after_crate("enshwick_world::typed::Health"),
            "::typed::Health"
        );
        assert_eq!(
            after_crate("enshwick_typed::typed::ecs::A"),
            "::typed::ecs::A"
        );
        assert_eq!(after_crate("Health"), "Health");
        assert_eq!(after_crate(""), "");
    }

    #[test]
    fn the_item_path_names_the_facade_s_crate() {
        const PATH: &str = crate::typed::runtime::item_path!(Health);
        assert_eq!(PATH, "enshwick::typed::runtime::path::tests::Health");
    }
}
