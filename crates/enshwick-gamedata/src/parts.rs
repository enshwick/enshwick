//! Which parts of a program work on a build, from the table entries each
//! part names.

use crate::Resolved;

/// A part of a program that names table entries, as one build serves it.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::{Part, PartState};
///
/// let part = Part { name: "weather", named: 2, missing: vec!["weather.set"] };
/// assert_eq!(part.state(), PartState::Partly);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part<'a> {
    /// The part's name.
    pub name: &'a str,
    /// How many of the entries it names the table holds for this side.
    pub named: usize,
    /// The entries it names that this build lacks.
    pub missing: Vec<&'a str>,
}

/// Whether a part works on a build.
///
/// # Examples
///
/// See [`Part`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PartState {
    /// Every entry it names is there.
    Works,
    /// Some entries it names are missing.
    Partly,
    /// Every entry it names is missing.
    Off,
}

impl Part<'_> {
    /// Whether the part works, partly works or is off on the build.
    ///
    /// # Examples
    ///
    /// See [`Part`].
    #[must_use]
    pub fn state(&self) -> PartState {
        if self.missing.is_empty() {
            PartState::Works
        } else if self.missing.len() == self.named {
            PartState::Off
        } else {
            PartState::Partly
        }
    }
}

impl Resolved {
    /// Each of `parts` with the entries it names that this build lacks;
    /// a part naming no entry of this side's table is left out.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::{PartState, Table};
    ///
    /// let table = Table::parse(
    ///     r#"
    ///     name = "1.0 server (1)"
    ///     side = "server"
    ///     guid = "00000000000000000000000000000001"
    ///     age = 2
    ///     [symbols]
    ///     "f.plain" = 0x1000
    ///     "#,
    /// )?;
    /// let parts = table.resolve().parts(&[("f", &["f.plain"][..])]);
    /// assert_eq!(parts[0].state(), PartState::Works);
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    #[must_use]
    pub fn parts<'a>(&self, parts: &[(&'a str, &[&'a str])]) -> Vec<Part<'a>> {
        parts
            .iter()
            .filter_map(|&(name, ids)| {
                let mut named = 0;
                let mut missing = Vec::new();
                for &id in ids {
                    if self.lacks(id) {
                        named += 1;
                        missing.push(id);
                    } else if self.holds(id) {
                        named += 1;
                    }
                }
                (named > 0).then_some(Part {
                    name,
                    named,
                    missing,
                })
            })
            .collect()
    }

    fn lacks(&self, id: &str) -> bool {
        self.missing()
            .binary_search_by(|m| m.as_str().cmp(id))
            .is_ok()
    }

    fn holds(&self, id: &str) -> bool {
        self.symbol(id).is_some()
            || self.constant(id).is_some()
            || self.uncarried().iter().any(|u| u == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Table;

    const TABLE: &str = r#"
        name = "test (2)"
        side = "server"
        guid = "000102030405060708090a0b0c0d0e0f"
        age = 7
        steam_build = 2
        game_version = 9
        [checks]
        "a_system" = 0x10
        [symbols]
        "f.one" = { rva = 0x1000, pattern = "11 22 33 44" }
        "f.two" = { rva = 0x1010, pattern = "55 66 77 88" }
        "g.one" = { rva = 0x1020, pattern = "AA BB CC DD" }
    "#;

    const PARTS: &[(&str, &[&str])] = &[
        ("f", &["f.one", "f.two"]),
        ("g", &["g.one", "g.client_only"]),
        ("h", &["h.client_only"]),
    ];

    #[test]
    fn a_table_made_for_the_build_serves_every_part() {
        let table = Table::parse(TABLE).unwrap();
        let parts = table.resolve().parts(PARTS);
        assert_eq!(
            parts.len(),
            2,
            "a part naming no entry of this side is left out"
        );
        assert!(parts.iter().all(|p| p.state() == PartState::Works));
        assert_eq!(parts[1].named, 1);
    }

    #[test]
    fn a_part_is_partly_there_when_patterns_find_some_of_its_entries() {
        let table = Table::parse(TABLE).unwrap();
        let code = [0x11, 0x22, 0x33, 0x44, 0, 0, 0, 0];
        let parts = table.fall_back(&code, 0x1000).parts(PARTS);
        assert_eq!(parts[0].state(), PartState::Partly);
        assert_eq!(parts[0].missing, ["f.two"]);
        assert_eq!(parts[1].state(), PartState::Off);
        assert_eq!(parts[1].missing, ["g.one"]);
    }
}
